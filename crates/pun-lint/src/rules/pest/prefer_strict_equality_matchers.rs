use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{BinaryOperator, Expression, Node, NodeKind};

use super::{arguments_text, expect_call, first_value, has_not, identifier, set_first_argument};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferStrictEqualityMatchers;

impl Rule for PreferStrictEqualityMatchers {
    fn id(&self) -> &'static str {
        "lint/pest/prefer-strict-equality-matchers"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::MethodCall(call) = node else {
            return;
        };
        let Some(method @ (b"toBeTrue" | b"toBeFalse")) = identifier(&call.method) else {
            return;
        };
        let Some(expect) = expect_call(call) else {
            return;
        };
        let Some(Expression::Binary(comparison)) =
            first_value(&expect.argument_list).map(unparenthesize)
        else {
            return;
        };
        let identical = match comparison.operator {
            BinaryOperator::Identical(_) => true,
            BinaryOperator::NotIdentical(_) => false,
            _ => return,
        };
        let mut edits = set_first_argument(ctx, &expect.argument_list, comparison.lhs);
        let not = (identical == (method == b"toBeFalse")) != has_not(call);
        let not = if not { "->not" } else { "" };
        let tail = Edit::new(
            Span::new(node_span(expect).end, node_span(call).end),
            format!("{not}->toBe({})", arguments_text(ctx, &[comparison.rhs])),
        );
        edits.push(tail);
        out.push(Report::fixed(
            node_span(call),
            "Use `toBe()` instead of checking a strict comparison.",
            Safety::Safe,
            edits,
        ));
    }
}
