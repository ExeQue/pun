//! The walker's entry into conditions, branches, loops and assignments.

use super::super::*;

impl<'arena> Checker<'_, 'arena> {
    pub(super) fn visit_if(&mut self, r#if: &If<'arena>, c: &mut ()) {
        dead_code::if_statement(self, r#if);
        if self
            .strict
            .contains(crate::StrictRules::BOOLEANS_IN_CONDITIONS)
        {
            operators::condition(
                self,
                r#if.span(),
                r#if.condition,
                "an if condition",
                "if.condNotBoolean",
            );
            for (at, cond) in operators::else_if_conditions(r#if) {
                operators::condition(
                    self,
                    at,
                    cond,
                    "an elseif condition",
                    "elseif.condNotBoolean",
                );
            }
        }
        self.walk_expression(r#if.condition, c);
        let mut branches: Vec<(Option<&Expression<'arena>>, Vec<&Statement<'arena>>)> = Vec::new();
        let has_else = match &r#if.body {
            IfBody::Statement(b) => {
                branches.push((Some(r#if.condition), vec![b.statement]));
                branches.extend(
                    b.else_if_clauses
                        .iter()
                        .map(|e| (Some(e.condition), vec![e.statement])),
                );
                if let Some(e) = &b.else_clause {
                    branches.push((None, vec![e.statement]));
                }
                b.else_if_clauses
                    .iter()
                    .for_each(|e| phpdoc_tags::clause(self, e.span()));
                b.else_clause
                    .iter()
                    .for_each(|e| phpdoc_tags::clause(self, e.span()));
                b.else_clause.is_some()
            }
            IfBody::ColonDelimited(b) => {
                branches.push((Some(r#if.condition), b.statements.iter().collect()));
                branches.extend(
                    b.else_if_clauses
                        .iter()
                        .map(|e| (Some(e.condition), e.statements.iter().collect())),
                );
                if let Some(e) = &b.else_clause {
                    branches.push((None, e.statements.iter().collect()));
                }
                b.else_if_clauses
                    .iter()
                    .for_each(|e| phpdoc_tags::clause(self, e.span()));
                b.else_clause
                    .iter()
                    .for_each(|e| phpdoc_tags::clause(self, e.span()));
                b.else_clause.is_some()
            }
        };
        let mut previous_false = Vec::new();
        let mut survivors: Vec<Vec<Guard>> = Vec::new();
        let mut expressions = Vec::new();
        let mut assignment_paths = Vec::new();
        let mut remaining_assignment_path = true;
        for (i, (cond, statements)) in branches.into_iter().enumerate() {
            let mut entry = previous_false.clone();
            let mut known = None;
            if let Some(cond) = cond {
                if i > 0 {
                    let mut assignments = self.assign.expressions.clone();
                    self.with_guards(previous_false.clone(), |s| {
                        s.walk_expression(cond, &mut ());
                        assignments.clone_from(&s.assign.expressions);
                    });
                    self.assign.expressions = assignments;
                }
                known = match types::flow::unparen(cond) {
                    Expression::Literal(Literal::True(_)) => Some(true),
                    Expression::Literal(Literal::False(_) | Literal::Null(_)) => Some(false),
                    _ => self.type_of(cond).and_then(|t| {
                        if t.is_always_truthy() {
                            Some(true)
                        } else {
                            t.is_always_falsy().then_some(false)
                        }
                    }),
                };
                entry.extend(self.truthy(cond));
                previous_false.extend(self.falsy(cond));
            }
            let (terminates, outgoing, assignments) = self.walk_branch(entry.clone(), &statements);
            if !terminates {
                expressions.push(outgoing);
                survivors.push(entry);
                if remaining_assignment_path && known != Some(false) {
                    assignment_paths.push(assignments);
                }
            }
            remaining_assignment_path &= known != Some(true);
        }
        if !has_else {
            let mut outgoing = self.expression_guards();
            outgoing.extend(
                previous_false
                    .iter()
                    .filter(|g| scope::definite_expression(g))
                    .cloned(),
            );
            expressions.push(outgoing);
            survivors.push(previous_false);
            if remaining_assignment_path {
                assignment_paths.push(self.assign.expressions.clone());
            }
        }
        if let Some((first, rest)) = survivors.split_first() {
            let after: Vec<Guard> = first
                .iter()
                .filter(|g| !scope::definite_expression(g) && rest.iter().all(|r| r.contains(g)))
                .cloned()
                .collect();
            self.guards.extend(after);
        }
        let assignments = match assignment_paths.len() {
            0 => self.assign.expressions.clone(),
            1 => assignment_paths.pop().expect("one surviving scope"),
            _ => HashSet::new(),
        };
        self.join_expression_guards(&expressions);
        self.assign.expressions = assignments;
    }

    pub(super) fn visit_conditional(&mut self, cond: &Conditional<'arena>, c: &mut ()) {
        dead_code::ternary(self, cond);
        if self
            .strict
            .contains(crate::StrictRules::BOOLEANS_IN_CONDITIONS)
            && cond.then.is_some()
        {
            let place = "a ternary operator condition";
            operators::condition(
                self,
                cond.span(),
                cond.condition,
                place,
                "ternary.condNotBoolean",
            );
        }
        self.walk_expression(cond.condition, c);
        let t = self.truthy(cond.condition);
        let yes = if let Some(then) = cond.then {
            self.guarded_expression(t, then)
        } else {
            self.expression_guards()
                .into_iter()
                .chain(t.into_iter().filter(scope::definite_expression))
                .collect()
        };
        let f = self.falsy(cond.condition);
        let no = self.guarded_expression(f, cond.r#else);
        self.join_expression_guards(&[yes, no]);
    }

    pub(super) fn visit_binary(&mut self, b: &Binary<'arena>, c: &mut ()) {
        types::binary(self, b);
        types::binary_operation(self, b);
        dead_code::binary(self, b);
        if b.operator.is_null_coalesce() {
            dead_code::null_coalesce(self, b.lhs, b.rhs, "??");
        }
        operators::binary(self, b);
        if matches!(b.operator, BinaryOperator::Instanceof(_)) {
            names::instanceof(self, b.lhs, b.rhs);
        }
        self.walk_expression(b.lhs, c);
        let extra = match b.operator {
            BinaryOperator::And(_) | BinaryOperator::LowAnd(_) => Some(self.truthy(b.lhs)),
            BinaryOperator::Or(_) | BinaryOperator::LowOr(_) => Some(self.falsy(b.lhs)),
            _ => None,
        };
        if let Some(extra) = extra {
            let skipped = self.expression_guards();
            let evaluated = self.guarded_expression(extra, b.rhs);
            self.join_expression_guards(&[skipped, evaluated]);
        } else {
            self.walk_expression(b.rhs, c);
        }
    }

    pub(super) fn visit_match(&mut self, m: &Match<'arena>, c: &mut ()) {
        types::void_match(self, m);
        dead_code::match_expression(self, m);
        self.walk_expression(m.expression, c);
        let on_true = matches!(m.expression, Expression::Literal(Literal::True(_)));
        let assignments = self.assign.expressions.clone();
        for arm in &m.arms {
            self.assign.expressions.clone_from(&assignments);
            match arm {
                MatchArm::Expression(arm) => {
                    for cond in &arm.conditions {
                        self.walk_expression(cond, c);
                    }
                    let extra = match (
                        on_true,
                        arm.conditions.iter().collect::<Vec<_>>().as_slice(),
                    ) {
                        (true, [one]) => self.truthy(one),
                        _ => Vec::new(),
                    };
                    self.mark_top_level(arm.expression);
                    self.with_guards(extra, |s| s.walk_expression(arm.expression, &mut ()));
                }
                MatchArm::Default(arm) => {
                    self.mark_top_level(arm.expression);
                    self.walk_expression(arm.expression, c);
                }
            }
        }
        if m.arms.len() > 1 {
            self.assign.expressions.clear();
        }
    }

    pub(super) fn visit_in_assignment(&mut self, a: &Assignment<'arena>) {
        if matches!(a.operator, AssignmentOperator::Coalesce(_)) {
            dead_code::null_coalesce(self, a.lhs, a.rhs, "??=");
        }
        property_access::assign(self, a);
        assign::assignment(self, a);
        assign::property_assign_ref(self, a);
        types::assignment(self, a);
        types::property_assignment(self, a);
        types::offset_assignment(self, a);
        types::assign_operation(self, a);
        operators::assignment(self, a);
        if let AssignmentOperator::Assign(_) = a.operator {
            arrays::destructuring(self, a.lhs, true);
            if matches!(
                types::flow::unparen(a.lhs),
                Expression::Array(_) | Expression::List(_)
            ) {
                let value = self.phpstan_type(a.rhs);
                types::destructuring(self, a.span(), a.lhs, value, Some(a.rhs.span()));
            }
        }
        if let Expression::Variable(Variable::Direct(v)) = a.lhs {
            let guards = self
                .truthy(a.rhs)
                .into_iter()
                .filter(scope::remembered_guard)
                .collect::<Vec<_>>();
            if guards.is_empty() {
                self.vars.remove(v.name);
            } else {
                self.vars.insert(v.name.to_vec(), guards);
            }
        }
    }

    pub(super) fn visit_in_unary_prefix(&mut self, u: &UnaryPrefix<'arena>) {
        dead_code::boolean_not(self, u);
        statements::cast(self, u);
        strict::cast(self, u);
        types::prefix(self, u);
        types::unary(self, u);
        operators::unary_prefix(self, u);
        internal::string_cast(self, u);
        if matches!(
            u.operator,
            UnaryPrefixOperator::PreIncrement(_) | UnaryPrefixOperator::PreDecrement(_)
        ) {
            assign::increment(self, u.operand);
            self.incremented(u.operand);
            self.invalidate_expression(u.operand);
        }
        if u.operator.is_increment_or_decrement() {
            types::property_increment(self, u.operand, u.operator.span(), u.span());
        }
    }

    pub(super) fn visit_in_foreach(&mut self, f: &Foreach<'arena>) {
        self.syntax.parents.push(syntax::Parent::Loop);
        assign::foreach(self, f);
        types::foreach(self, f);
        let (key, value) = match &f.target {
            ForeachTarget::Value(t) => (None, t.value),
            ForeachTarget::KeyValue(t) => (Some(t.key), t.value),
        };
        for target in key.into_iter().chain([value]) {
            self.invalidate_expression(target);
            arrays::destructuring(self, target, true);
        }
        if matches!(
            types::flow::unparen(value),
            Expression::Array(_) | Expression::List(_)
        ) {
            let iterated = self
                .phpstan_type(f.expression)
                .map(|t| declared::with_classes(self, || t.iterable_value_type()));
            types::destructuring(self, value.span(), value, iterated, None);
        }
    }

    pub(super) fn visit_while(&mut self, w: &While<'arena>, c: &mut ()) {
        dead_code::while_loop(self, w);
        if self
            .strict
            .contains(crate::StrictRules::BOOLEANS_IN_LOOP_CONDITIONS)
        {
            operators::condition(
                self,
                w.span(),
                w.condition,
                "a while condition",
                "while.condNotBoolean",
            );
        }
        self.syntax.parents.push(syntax::Parent::Loop);
        self.walk_expression(w.condition, c);
        let statements: Vec<&Statement<'arena>> = match &w.body {
            WhileBody::Statement(s) => vec![s],
            WhileBody::ColonDelimited(b) => b.statements.iter().collect(),
        };
        let guards = self.truthy(w.condition);
        self.walk_branch(guards, &statements);
        self.syntax.parents.pop();
    }
}
