use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{
    ArrayElement, Assignment, AssignmentOperator, Block, Call, Expression, ForeachTarget, Hint,
    Node, NodeKind, Statement, Try, UnaryPrefixOperator, Variable,
};

use super::{
    Item, comments_start, expect_argument, function_name, indent_at, is_expect_chain, is_name,
    kept_text, method_call, old_statements, p_stmts, statement_expression,
};
use crate::rules::support::options::part_on;
use crate::rules::support::statements::{block_list, tokens};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct PreferToThrow {
    /// `try-catch` is left out of the rule's `checks`.
    off: bool,
}

const TEST_FUNCTIONS: [&[u8]; 7] = [
    b"test",
    b"it",
    b"describe",
    b"beforeEach",
    b"afterEach",
    b"beforeAll",
    b"afterAll",
];

const SUPERGLOBALS: [&[u8]; 9] = [
    b"$GLOBALS",
    b"$_SERVER",
    b"$_GET",
    b"$_POST",
    b"$_FILES",
    b"$_COOKIE",
    b"$_SESSION",
    b"$_REQUEST",
    b"$_ENV",
];

impl Rule for PreferToThrow {
    fn id(&self) -> &'static str {
        "lint/pest/prefer-to-throw"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !checks(options, "try-catch")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if self.off {
            return;
        }
        let Node::FunctionCall(call) = node else {
            return;
        };
        if !function_name(call)
            .is_some_and(|n| TEST_FUNCTIONS.iter().any(|f| f.eq_ignore_ascii_case(n)))
        {
            return;
        }
        let mut edits = Vec::new();
        for argument in &call.argument_list.arguments {
            if let Expression::Closure(closure) = unparenthesize(argument.value()) {
                find_try(ctx, Node::Block(&closure.body), &mut edits);
            }
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report::fixed(
            node_span(call),
            "Use `expect(...)->toThrow()` instead of a `try`/`catch`.",
            Safety::Safe,
            edits,
        ));
    }
}

fn find_try(ctx: &Context<'_>, node: Node<'_, '_>, edits: &mut Vec<Edit>) {
    match node {
        Node::Closure(_) => return,
        Node::Try(t) => {
            if let Some(edit) = convert(ctx, t) {
                edits.push(edit);
                return;
            }
        }
        _ => {}
    }
    for child in node.children() {
        find_try(ctx, child, edits);
    }
}

fn convert(ctx: &Context<'_>, t: &Try<'_>) -> Option<Edit> {
    let ([catch], None) = (t.catch_clauses.as_slice(), &t.finally_clause) else {
        return None;
    };
    if matches!(catch.hint, Hint::Union(_)) {
        return None;
    }
    let tokens = tokens(ctx);
    let body = block_list(tokens, &t.block);
    if body.is_empty() {
        return None;
    }
    let message = message_assertion(ctx, &catch.block, catch.variable.as_ref().map(|v| v.name));
    if !block_list(tokens, &catch.block).is_empty() && message.is_none() {
        return None;
    }
    let span = node_span(t);
    let indent = indent_at(ctx, span.start);
    let callable = match (body.stmts.as_slice(), body.nop) {
        ([statement], false) if statement_expression(statement).is_some() => {
            let expr = unparenthesize(statement_expression(statement)?);
            format!("fn() => {}", kept_text(ctx, node_span(expr), indent))
        }
        _ => {
            let uses = use_variables(&body.stmts);
            let uses = match uses.is_empty() {
                true => String::new(),
                false => format!(" use ({})", uses.join(", ")),
            };
            let olds = old_statements(ctx, &body);
            let items: Vec<Item<'_>> = (0..olds.len()).map(Item::Old).collect();
            format!(
                "function (){uses} {{{}\n{}}}",
                p_stmts(ctx, &olds, &items, indent + 4),
                " ".repeat(indent)
            )
        }
    };
    let mut args = format!("{}::class", kept_text(ctx, node_span(&catch.hint), indent));
    if let Some(m) = message {
        args.push_str(", ");
        args.push_str(&kept_text(ctx, node_span(unparenthesize(m)), indent));
    }
    Some(Edit::new(
        Span::new(comments_start(ctx, span.start), span.end),
        format!("expect({callable})->toThrow({args});"),
    ))
}

fn message_assertion<'a>(
    ctx: &Context<'_>,
    block: &'a Block<'a>,
    var: Option<&[u8]>,
) -> Option<&'a Expression<'a>> {
    let tokens = tokens(ctx);
    let list = block_list(tokens, block);
    let ([statement], false) = (list.stmts.as_slice(), list.nop) else {
        return None;
    };
    let call = method_call(statement_expression(statement)?)?;
    if !is_expect_chain(call) || !is_name(&call.method, "toBe") {
        return None;
    }
    // Only `expect($e->getMessage())->toBe(...)` itself. Rector also takes the last `toBe()` of a longer chain
    // (`->toBe('m')->and($e->code())->toBe([...])`, `->not->toBe('m')`), which drops the other assertions and can
    // pass a value `toThrow()` does not accept.
    if !matches!(
        unparenthesize(call.object),
        Expression::Call(Call::Function(_))
    ) {
        return None;
    }
    let [argument] = call.argument_list.arguments.as_slice() else {
        return None;
    };
    let subject = method_call(expect_argument(call)?)?;
    if !is_name(&subject.method, "getMessage") {
        return None;
    }
    let Expression::Variable(Variable::Direct(object)) = unparenthesize(subject.object) else {
        return None;
    };
    (Some(object.name) == var).then(|| argument.value())
}

fn use_variables(stmts: &[&Statement<'_>]) -> Vec<String> {
    let mut assigned_spans = Vec::new();
    let mut variables = Vec::new();
    for statement in stmts {
        collect(
            Node::Statement(statement),
            &mut assigned_spans,
            &mut variables,
        );
    }
    let mut assigned: Vec<&[u8]> = Vec::new();
    let mut uses: Vec<&[u8]> = Vec::new();
    for (name, span) in variables {
        if name.eq_ignore_ascii_case(b"$this") || SUPERGLOBALS.contains(&name) {
            continue;
        }
        if assigned_spans.contains(&span) {
            assigned.push(name);
        }
        if !assigned.contains(&name) && !uses.contains(&name) {
            uses.push(name);
        }
    }
    uses.iter()
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect()
}

fn collect<'a>(
    node: Node<'_, 'a>,
    assigned: &mut Vec<Span>,
    variables: &mut Vec<(&'a [u8], Span)>,
) {
    let mut mark = |expr: &Expression<'_>| {
        let expr = match expr {
            Expression::UnaryPrefix(u)
                if matches!(u.operator, UnaryPrefixOperator::Reference(_)) =>
            {
                u.operand
            }
            e => e,
        };
        if let Expression::Variable(Variable::Direct(v)) = expr {
            assigned.push(node_span(v));
        }
    };
    match node {
        Node::DirectVariable(v) => variables.push((v.name, node_span(v))),
        Node::Assignment(Assignment {
            lhs,
            operator: AssignmentOperator::Assign(_),
            ..
        }) => {
            mark(lhs);
            let elements = match lhs {
                Expression::Array(a) => Some(a.elements.as_slice()),
                Expression::List(l) => Some(l.elements.as_slice()),
                _ => None,
            };
            for element in elements.unwrap_or_default() {
                match element {
                    ArrayElement::KeyValue(e) => mark(e.value),
                    ArrayElement::Value(e) => mark(e.value),
                    _ => {}
                }
            }
        }
        Node::Foreach(f) => match &f.target {
            ForeachTarget::Value(t) => mark(t.value),
            ForeachTarget::KeyValue(t) => {
                mark(t.key);
                mark(t.value);
            }
        },
        Node::FunctionLikeParameter(p) => assigned.push(node_span(&p.variable)),
        _ => {}
    }
    for child in node.children() {
        collect(child, assigned, variables);
    }
}

const CHECKS: [&str; 2] = ["expect-exception", "try-catch"];

/// Pun's `checks`: the ones that run, one for each of Rector's 2 rules (`expect-exception` is
/// `ConvertExpectExceptionToThrowRector`, `try-catch` is `UseToThrowRector`).
pub(super) fn checks(options: &serde_json::Value, check: &str) -> Result<bool, OptionsError> {
    part_on(options, "checks", &CHECKS, check)
}
