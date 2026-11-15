use mago_syntax::cst::{
    BinaryOperator, Expression, Node, NodeKind, Statement, UnaryPrefixOperator,
};
use pun_phpstan::types::Type;
use pun_problem::{Edit, Fix, Safety, Span};

use crate::rules::phpdoc::no_useless_var_tag::var_removal_if_not_comment;
use crate::rules::support::conditions::{if_parts, is_stmts_aware};
use crate::rules::support::docblocks::Doc;
use crate::rules::support::expressions::is_constant;
use crate::rules::support::statements::{enclosing, tokens};
use crate::rules::types::compare::RType;
use crate::rules::types::compare::classes::with_classes;
use crate::rules::types::compare::mapper::{TypeScope, expr_type};
use crate::rules::types::facts::facts;
use crate::rules::types::names::resolve_class_name;
use crate::syntax::{node_span, same_expression, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoRedundantNullableCheck;

impl Rule for NoRedundantNullableCheck {
    fn id(&self) -> &'static str {
        "lint/complexity/no-redundant-null-check"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Statement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Statement(Statement::Return(stmt)) = node else {
            return;
        };
        let n = ancestors.len();
        if n < 2 || !is_stmts_aware(ancestors[n - 1], &ancestors[..n - 1]) {
            return;
        }
        let source = ctx.source;
        let tokens = tokens(ctx);
        let mut chain = ancestors.to_vec();
        chain.push(node);
        let (list, key) = enclosing(source, tokens, &chain);
        let Some(Statement::If(r#if)) = key.checked_sub(1).map(|k| list.stmts[k]) else {
            return;
        };
        let parts = if_parts(tokens, r#if);
        if parts.has_else || parts.has_elseifs || parts.stmts.len() != 1 {
            return;
        }
        let Statement::Return(return_if) = parts.stmts.stmts[0] else {
            return;
        };
        let (negated, instanceof) = match unparenthesize(r#if.condition) {
            Expression::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Not(_)) => {
                (true, unparenthesize(u.operand))
            }
            cond => (false, cond),
        };
        let Expression::Binary(instanceof) = instanceof else {
            return;
        };
        if !matches!(instanceof.operator, BinaryOperator::Instanceof(_)) {
            return;
        }
        let variable = instanceof.lhs;
        let Expression::Identifier(class) = instanceof.rhs else {
            return;
        };
        let is_null = |e: Option<&Expression<'_>>| e.is_some_and(|e| is_constant(e, "null"));
        let is_variable =
            |e: Option<&Expression<'_>>| e.is_some_and(|e| same_expression(source, variable, e));
        let (if_returns, next_returns) = (return_if.value, stmt.value);
        if negated && !(is_null(if_returns) && is_variable(next_returns)) {
            return;
        }
        if !negated && !(is_variable(if_returns) && is_null(next_returns)) {
            return;
        }
        let Some(Statement::Expression(expression)) = key.checked_sub(2).map(|k| list.stmts[k])
        else {
            return;
        };
        let Expression::Assignment(assign) = expression.expression else {
            return;
        };
        if !assign.operator.is_assign() || !same_expression(source, assign.lhs, variable) {
            return;
        }
        let facts = facts(ctx);
        let Type::Union(types) = expr_type(&facts, &chain, assign.rhs, false) else {
            return;
        };
        let class_name = resolve_class_name(&chain, class);
        let [first, second] = types.as_slice() else {
            return;
        };
        let ((Type::Object(object, _, _), Type::Null) | (Type::Null, Type::Object(object, _, _))) =
            (first, second)
        else {
            return;
        };
        if *object != class_name {
            return;
        }
        let assign_start = node_span(list.stmts[key - 2]).start;
        let end = node_span(stmt).end;
        let rhs = node_span(unparenthesize(assign.rhs));
        let code = format!("return {};", &source[rhs.start..rhs.end]);
        let doc = Doc::last_of(ctx, &chain[..n], assign_start);
        let scope = TypeScope::new(&chain, facts.documented());
        let removal = doc.as_ref().and_then(|doc| {
            with_classes(facts.documented(), || {
                var_removal_if_not_comment(doc, &RType::T(Type::Union(types.clone())), &scope)
            })
        });
        let edit = match (doc, removal) {
            (Some(doc), Some(removed)) => {
                let edit = doc.remove_last(source, &removed);
                let between = &source[edit.span.end..assign_start];
                Edit::new(
                    Span::new(edit.span.start, end),
                    format!("{}{between}{code}", edit.text),
                )
            }
            _ => Edit::new(Span::new(assign_start, end), code),
        };
        out.push(Report {
            span: node_span(r#if),
            message: "Return the value directly; the `instanceof` check returns it or null anyway."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![edit],
            }),
        });
    }
}
