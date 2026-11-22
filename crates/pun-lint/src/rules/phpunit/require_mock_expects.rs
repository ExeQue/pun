use crate::problem::{Edit, Safety, Span};
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{Array, Call, Expression, MethodBody, Node, NodeKind, Statement};

use super::annotations::named_tags;
use super::prefer_create_stub_expression::is_mock_object;
use super::{AnyCall, is_public};
use crate::rules::support::docblocks::Doc;
use crate::rules::support::names::has_attribute;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct RequireMockExpects;

impl Rule for RequireMockExpects {
    fn id(&self) -> &'static str {
        "lint/phpunit/require-mock-expects"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Method(method) = node else { return };
        if method.name.value.eq_ignore_ascii_case(b"setUp") {
            return;
        }
        let is_test = is_public(method)
            && (method.name.value.starts_with(b"test")
                || has_attribute(
                    ancestors,
                    &method.attribute_lists,
                    "PHPUnit\\Framework\\Attributes\\Test",
                )
                || Doc::of(ctx, ancestors, node_span(method).start).is_some_and(|doc| {
                    named_tags(ctx, ancestors, &doc, "test").is_some_and(|t| !t.is_empty())
                }));
        if !is_test {
            return;
        }
        let MethodBody::Concrete(_) = &method.body else {
            return;
        };
        let mut stack = vec![node];
        while let Some(node) = stack.pop() {
            if let Node::Statement(Statement::Expression(statement)) = node
                && let Some(edit) = refactor_expression(ctx, statement.expression)
            {
                out.push(Report::fixed(
                    node_span(statement),
                    "Add expects() to a mock configured with willReturnMap().",
                    Safety::Safe,
                    vec![edit],
                ));
            }
            node.visit_children(|child| stack.push(child));
        }
    }
}

fn refactor_expression(ctx: &Context<'_>, expr: &Expression<'_>) -> Option<Edit> {
    let Expression::Call(Call::Method(call)) = expr else {
        return None;
    };
    let method_call = AnyCall::of(Node::MethodCall(call))?;
    if !method_call.is_name(&["willReturnMap"]) {
        return None;
    }
    let mut topmost = call;
    while let Expression::Call(Call::Method(inner)) = topmost.object {
        topmost = inner;
    }
    if AnyCall::of(Node::MethodCall(topmost)).is_some_and(|c| c.is_name(&["expects"])) {
        return None;
    }
    let facts = facts(ctx);
    let receiver = unparenthesize(topmost.object);
    if !facts
        .full_type(node_span(receiver))
        .is_some_and(|t| is_mock_object_type(&facts.codebase, t))
    {
        return None;
    }
    let arg = method_call.args().into_iter().next()?;
    let Expression::Array(Array { elements, .. }) = unparenthesize(arg.value()) else {
        return None;
    };
    let at = node_span(receiver).end;
    Some(Edit::new(
        Span::new(at, at),
        format!("->expects($this->atLeast({}))", elements.len()),
    ))
}

fn is_mock_object_type(codebase: &mago_codex::metadata::CodebaseMetadata, t: &TUnion) -> bool {
    let mut kept = t.types.iter().filter(|a| match a {
        TAtomic::Null => false,
        TAtomic::Scalar(TScalar::Bool(b)) => !b.is_false(),
        _ => true,
    });
    let mut any = false;
    kept.all(|a| {
        any = true;
        is_mock_object(codebase, a)
    }) && any
}
