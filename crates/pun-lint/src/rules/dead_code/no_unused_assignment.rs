use crate::problem::{Fix, Safety};
use mago_codex::metadata::CodebaseMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::*;

use crate::rules::support::conditions::comments_between;
use crate::rules::support::expressions::is_constant;
use crate::rules::support::names::called_function_name;
use crate::rules::support::options::part_on;
use crate::rules::support::side_effects::{any_node, call_has_side_effect};
use crate::rules::support::statement_edits::{
    expression, is_plain_assign, is_static_or_global, is_used_by_reference, variable_name,
};
use crate::rules::support::statement_splice::{Change, Code, NewStatement, run_edits};
use crate::rules::support::statements::{Token, previous_token_end, tokens};
use crate::rules::support::variable_usage::used_in;
use crate::rules::types::facts::{Facts, facts};
use crate::rules::types::names::{class_imports, resolve_class_name};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct NoUnusedAssignment {
    /// `unused` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for NoUnusedAssignment {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-unused-assignment"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !checks(options, "unused")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ExpressionStatement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if self.off {
            return;
        }
        if !matches!(node, Node::ExpressionStatement(_)) || body(ancestors).is_none() {
            return;
        }
        let source = ctx.source;
        let tokens = tokens(ctx);
        let facts = facts(ctx);
        let edits = run_edits(source, tokens, ancestors, &|chain| {
            change(ctx, tokens, &facts, chain)
        });
        if let Some(edits) = edits {
            out.push(Report {
                span: node_span(&ancestors[ancestors.len() - 1]),
                message: "Remove this assignment; nothing reads the variable after it.".to_string(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits,
                }),
            });
        }
    }
}

fn body<'ast, 'arena>(chain: &[Node<'ast, 'arena>]) -> Option<&'ast Block<'arena>> {
    let n = chain.len();
    let Node::Block(block) = *chain.get(n.checked_sub(2)?)? else {
        return None;
    };
    let is_body = chain[..n - 2].iter().rev().take(2).any(|a| match a {
        Node::Function(f) => std::ptr::eq(&raw const f.body, block),
        Node::Method(m) => matches!(&m.body, MethodBody::Concrete(b) if std::ptr::eq(b, block)),
        _ => false,
    });
    is_body.then_some(block)
}

fn change(
    ctx: &Context<'_>,
    tokens: &[Token],
    facts: &Facts,
    chain: &[Node<'_, '_>],
) -> Option<Change> {
    let source = ctx.source;
    let n = chain.len();
    let Node::Statement(statement) = chain[n - 1] else {
        return None;
    };
    // Only an assignment to a variable can be removed: rule out the other statements before scanning the body.
    let assign = match expression(statement)? {
        Expression::Assignment(a) if is_plain_assign(a) => a,
        _ => return None,
    };
    let name = variable_name(assign.lhs)?;
    let block = body(chain)?;
    let stmts: Vec<&Statement<'_>> = block.statements.iter().collect();
    let key = stmts
        .iter()
        .position(|s| node_span(*s) == node_span(statement))?;
    // The answer is the body's, so it is worked out once per body.
    if *ctx.memo(("unused-assignment-skips", node_span(block).start), || {
        stmts
            .iter()
            .any(|s| any_node(Node::Statement(s), &skips_body))
    }) {
        return None;
    }
    let mut refs: Vec<Vec<u8>> = Vec::new();
    for s in &stmts[..key] {
        collect_refs(s, &mut refs);
    }
    collect_refs(statement, &mut refs);
    if is_native_variable(name)
        || is_static_or_global(chain, name)
        || is_used_by_reference(ctx, chain, name, node_span(assign.lhs).start)
        || refs.iter().any(|r| r == name)
        || is_no_discard_call(facts, chain, unparenthesize(assign.rhs))
    {
        return None;
    }
    if stmts[key + 1..]
        .iter()
        .any(|s| used_in(Node::Statement(s), None, name))
    {
        return None;
    }
    if is_object_with_destruct_method(facts, assign.rhs)
        || is_null_reset_of_internal_object(facts, chain, assign, node_span(statement).start)
    {
        return None;
    }
    let has_call_like = any_node(Node::Expression(assign.rhs), &|node| match node {
        Node::Expression(e) => call_has_side_effect(chain, e),
        _ => false,
    });
    if !has_call_like {
        return Some(Change::Remove);
    }
    let mut expr = unparenthesize(assign.rhs);
    while let Expression::UnaryPrefix(u) = expr
        && u.operator.is_cast()
    {
        expr = unparenthesize(u.operand);
    }
    let span = node_span(expr);
    let start = node_span(statement).start;
    Some(Change::Replace(vec![NewStatement {
        comments: comments_between(tokens, previous_token_end(source, tokens, start), start),
        code: Code::Text(format!("{};", &source[span.start..span.end])),
    }]))
}

fn skips_body(node: Node<'_, '_>) -> bool {
    match node {
        Node::IncludeConstruct(_)
        | Node::IncludeOnceConstruct(_)
        | Node::RequireConstruct(_)
        | Node::RequireOnceConstruct(_) => true,
        Node::FunctionCall(call) => match call.function {
            Expression::Identifier(id) => {
                let name = String::from_utf8_lossy(id.value());
                let name = name.strip_prefix('\\').unwrap_or(match id {
                    Identifier::Local(_) => &name,
                    _ => "",
                });
                name.eq_ignore_ascii_case("compact")
                    || name.eq_ignore_ascii_case("get_defined_vars")
            }
            _ => false,
        },
        _ => false,
    }
}

fn collect_refs(statement: &Statement<'_>, refs: &mut Vec<Vec<u8>>) {
    let Some(expr) = expression(statement) else {
        let holds_statements = matches!(
            statement,
            Statement::Block(_)
                | Statement::DoWhile(_)
                | Statement::For(_)
                | Statement::Foreach(_)
                | Statement::Function(_)
                | Statement::If(_)
                | Statement::Namespace(_)
                | Statement::Try(_)
                | Statement::While(_)
                | Statement::Declare(_)
        );
        if holds_statements {
            let mut stack = vec![Node::Statement(statement)];
            while let Some(node) = stack.pop() {
                if let Node::Assignment(a) = node
                    && is_assign_ref(a)
                    && let Some(name) = variable_name(a.lhs)
                {
                    refs.push(name.to_vec());
                }
                node.visit_children(|child| stack.push(child));
            }
        }
        return;
    };
    let Expression::Assignment(assign) = expr else {
        return;
    };
    if is_assign_ref(assign) {
        if let Some(name) = variable_name(assign.lhs) {
            refs.push(name.to_vec());
        }
        return;
    }
    if !is_plain_assign(assign) {
        return;
    }
    let mut stack = vec![Node::Expression(assign.rhs)];
    while let Some(node) = stack.pop() {
        match node {
            Node::AnonymousClass(_) | Node::Function(_) => continue,
            Node::Closure(c) => {
                for item in c.use_clause.iter().flat_map(|u| u.variables.iter()) {
                    if item.ampersand.is_some() {
                        refs.push(item.variable.name[1..].to_vec());
                    }
                }
            }
            _ => {}
        }
        node.visit_children(|child| stack.push(child));
    }
}

fn is_assign_ref(a: &Assignment<'_>) -> bool {
    matches!(a.operator, AssignmentOperator::Assign(_))
        && matches!(a.rhs, Expression::UnaryPrefix(u) if u.operator.is_reference())
}

pub(crate) fn is_native_variable(name: &[u8]) -> bool {
    [
        &b"GLOBALS"[..],
        b"_SERVER",
        b"_GET",
        b"_POST",
        b"_FILES",
        b"_COOKIE",
        b"_SESSION",
        b"_REQUEST",
        b"_ENV",
    ]
    .contains(&name)
}

fn is_no_discard_call(facts: &Facts, chain: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    let codebase = &facts.codebase;
    let no_discard = |attributes: &[mago_codex::metadata::attribute::AttributeMetadata]| {
        attributes.iter().any(|a| a.name.as_bytes() == b"NoDiscard")
    };
    let (classes, method) = match expr {
        Expression::Call(Call::Function(call)) => {
            let Some(name) = called_function_name(chain, call.function) else {
                return false;
            };
            let local = matches!(call.function, Expression::Identifier(Identifier::Local(_)));
            let namespaced = class_imports(chain)
                .0
                .filter(|_| local && !name.contains('\\'));
            let function = namespaced
                .and_then(|ns| codebase.get_function(format!("{ns}\\{name}").as_bytes()))
                .or_else(|| codebase.get_function(name.as_bytes()));
            return function.is_some_and(|f| no_discard(&f.attributes));
        }
        Expression::Call(Call::StaticMethod(call)) => {
            (static_class_names(facts, chain, call.class), &call.method)
        }
        Expression::Call(Call::Method(call)) => {
            (object_class_names(facts, call.object), &call.method)
        }
        Expression::Call(Call::NullSafeMethod(call)) => {
            (object_class_names(facts, call.object), &call.method)
        }
        _ => return false,
    };
    let ClassLikeMemberSelector::Identifier(method) = method else {
        return false;
    };
    classes.iter().any(|class| {
        codebase
            .get_declaring_method(class.as_bytes(), method.value)
            .is_some_and(|m| no_discard(&m.attributes))
    })
}

fn object_class_names(facts: &Facts, expr: &Expression<'_>) -> Vec<String> {
    fn names(atomic: &TAtomic, out: &mut Vec<String>) {
        match atomic {
            TAtomic::Object(TObject::Named(object)) => {
                out.push(String::from_utf8_lossy(object.name.as_bytes()).into_owned());
                object
                    .intersection_types
                    .iter()
                    .flat_map(|ts| ts.iter())
                    .for_each(|t| names(t, out));
            }
            TAtomic::Object(TObject::Enum(e)) => {
                out.push(String::from_utf8_lossy(e.name.as_bytes()).into_owned());
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    for atomic in facts
        .full_type(node_span(expr))
        .iter()
        .flat_map(|t| t.types.iter())
    {
        names(atomic, &mut out);
    }
    out
}

fn static_class_names(
    facts: &Facts,
    chain: &[Node<'_, '_>],
    class: &Expression<'_>,
) -> Vec<String> {
    let parent = match class {
        Expression::Identifier(id) => return vec![resolve_class_name(chain, id)],
        Expression::Self_(_) | Expression::Static(_) => false,
        Expression::Parent(_) => true,
        _ => return object_class_names(facts, class),
    };
    let Some(own) = chain.iter().rev().find_map(|n| match n {
        Node::Class(c) => Some(c.name.value),
        Node::Trait(t) => Some(t.name.value),
        Node::Enum(e) => Some(e.name.value),
        _ => None,
    }) else {
        return Vec::new();
    };
    let own = String::from_utf8_lossy(own);
    let own = match class_imports(chain).0 {
        Some(ns) => format!("{ns}\\{own}"),
        None => own.into_owned(),
    };
    if !parent {
        return vec![own];
    }
    parent_of(&facts.codebase, &own).into_iter().collect()
}

fn parent_of(codebase: &CodebaseMetadata, class: &str) -> Option<String> {
    let parent = codebase
        .get_class_like(class.as_bytes())?
        .direct_parent_class?;
    Some(
        String::from_utf8_lossy(
            codebase
                .get_class_like(parent.as_bytes())?
                .original_name
                .as_bytes(),
        )
        .into_owned(),
    )
}

fn object_class(facts: &Facts, span: crate::problem::Span) -> Option<&[u8]> {
    match facts.full_type(span)?.types.as_ref() {
        [TAtomic::Object(TObject::Named(object))] if !object.is_this => {
            Some(object.name.as_bytes())
        }
        _ => None,
    }
}

fn is_object_with_destruct_method(facts: &Facts, expr: &Expression<'_>) -> bool {
    object_class(facts, node_span(expr)).is_some_and(|class| {
        facts
            .codebase
            .get_declaring_method(class, b"__destruct")
            .is_some()
    })
}

fn is_null_reset_of_internal_object(
    facts: &Facts,
    chain: &[Node<'_, '_>],
    assign: &Assignment<'_>,
    start: usize,
) -> bool {
    let Expression::Variable(Variable::Direct(variable)) = unparenthesize(assign.lhs) else {
        return false;
    };
    if !is_constant(assign.rhs, "null") {
        return false;
    }
    let Some(function) = chain
        .iter()
        .rev()
        .find(|n| matches!(n, Node::Function(_) | Node::Method(_)))
    else {
        return false;
    };
    let mut read = None;
    find_last(*function, variable.name, start, &mut read);
    read.and_then(|span| object_class(facts, span))
        .and_then(|class| facts.codebase.get_class_like(class))
        .is_some_and(|c| c.flags.is_built_in())
}

/// Where the variable `name` last got its value under `node` before `before`, not inside a class or a function-like
/// other than `node`: the last place that names it, or the right side of a later plain assignment to it.
pub(crate) fn find_last(
    node: Node<'_, '_>,
    name: &[u8],
    before: usize,
    found: &mut Option<crate::problem::Span>,
) {
    node.visit_children(|child| {
        if node_span(&child).start >= before
            || matches!(
                child,
                Node::Closure(_)
                    | Node::ArrowFunction(_)
                    | Node::Function(_)
                    | Node::Class(_)
                    | Node::AnonymousClass(_)
            )
        {
            return;
        }
        match child {
            Node::Assignment(a)
                if matches!(a.operator, AssignmentOperator::Assign(_))
                    && matches!(a.lhs, Expression::Variable(Variable::Direct(v)) if v.name == name) =>
            {
                find_last(Node::Expression(a.rhs), name, before, found);
                *found = Some(node_span(a.rhs));
                return;
            }
            Node::DirectVariable(v) if v.name == name => *found = Some(node_span(v)),
            _ => {}
        }
        find_last(child, name, before, found);
    });
}

const CHECKS: [&str; 2] = ["double", "unused"];

/// Pun's `checks`: the ones that run, one for each of Rector's 2 rules (`double` is `RemoveDoubleAssignRector`,
/// `unused` is `RemoveUnusedVariableAssignRector`).
pub(super) fn checks(options: &serde_json::Value, check: &str) -> Result<bool, OptionsError> {
    part_on(options, "checks", &CHECKS, check)
}
