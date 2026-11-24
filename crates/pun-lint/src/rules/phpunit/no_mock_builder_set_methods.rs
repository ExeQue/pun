//! rector-phpunit's `RemoveSetMethodsMethodCallRector` (PHPUnit 10.0): `MockBuilder::setMethods()` is removed, or
//! becomes `addMethods()` with the mocked class's `@method` names it lists.

use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{ArrayElement, Expression, Node, NodeKind};

use super::{AnyCall, class_string, is_in_test_class, receiver_is, string_value};
use crate::rules::refactor::configured_codemods::single_quoted;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const MOCK_BUILDER: &str = "PHPUnit\\Framework\\MockObject\\MockBuilder";

pub struct NoMockBuilderSetMethods;

impl Rule for NoMockBuilderSetMethods {
    fn id(&self) -> &'static str {
        "lint/phpunit/no-mock-builder-set-methods"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::MethodCall(method_call) = node else {
            return;
        };
        let call = AnyCall::of(node).unwrap();
        if !call.is_name(&["setMethods"])
            || !is_in_test_class(ctx, ancestors, None)
            || !receiver_is(ctx, ancestors, &call, &[MOCK_BUILDER])
        {
            return;
        }
        let magic = mocked_magic_method_names(ctx, ancestors, method_call.object, &call);
        let (message, edits) = if magic.is_empty() {
            let from = node_span(method_call.object).end;
            (
                "Remove `setMethods()`, which PHPUnit 10 removed.",
                vec![Edit::new(Span::new(from, call.span.end), "")],
            )
        } else {
            let names: Vec<String> = magic.iter().map(|n| single_quoted(n)).collect();
            let inner = Span::new(
                call.args.left_parenthesis.end.offset as usize,
                call.args.right_parenthesis.start.offset as usize,
            );
            (
                "Use `addMethods()` for the magic methods; PHPUnit 10 removed `setMethods()`.",
                vec![
                    Edit::new(call.name_span(), "addMethods"),
                    Edit::new(inner, format!("[{}]", names.join(", "))),
                ],
            )
        };
        out.push(Report::fixed(
            call.name_span(),
            message,
            Safety::Unsafe,
            edits,
        ));
    }
}

/// `resolvedMockedMagicMethodNames()`: the listed methods the mocked class declares with `@method`, in list order.
fn mocked_magic_method_names(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    object: &Expression<'_>,
    call: &AnyCall<'_>,
) -> Vec<String> {
    // the getMockBuilder() call up the chain
    let mut at = unparenthesize(object);
    let mocked = loop {
        let Expression::Call(mago_syntax::cst::Call::Method(m)) = at else {
            return Vec::new();
        };
        let inner = AnyCall::of(Node::MethodCall(m)).unwrap();
        if inner.is_name(&["getMockBuilder"]) {
            let Some(first) = inner.args().first().copied() else {
                return Vec::new();
            };
            break class_string(ctx, ancestors, first.value());
        }
        at = unparenthesize(m.object);
    };
    let Some(mocked) = mocked else {
        return Vec::new();
    };
    let facts = facts(ctx);
    let Some(class) = facts.documented().get_class_like(mocked.as_bytes()) else {
        return Vec::new();
    };
    let magic: Vec<String> = class
        .pseudo_methods
        .iter()
        .map(ToString::to_string)
        .collect();
    if magic.is_empty() {
        return Vec::new();
    }
    let Some(first) = call.args().first().copied() else {
        return Vec::new();
    };
    let Expression::Array(array) = unparenthesize(first.value()) else {
        return Vec::new();
    };
    let mut listed = Vec::new();
    for element in &array.elements {
        let ArrayElement::Value(v) = element else {
            return Vec::new();
        };
        let Some(name) = string_value(v.value) else {
            return Vec::new();
        };
        listed.push(name);
    }
    listed
        .into_iter()
        .filter(|n| magic.iter().any(|m| m.eq_ignore_ascii_case(n)))
        .collect()
}
