//! Rector 2.8.4's `MbStrrposEncodingArgumentPositionRector`.

use mago_syntax::cst::{
    Argument, BinaryOperator, Expression, Hint, Literal, Node, NodeKind, Variable,
};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{named, text};
use crate::rules::support::syntax_types::{local_of, parameter_hint};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoMbStrrposEncodingOffset;

impl Rule for NoMbStrrposEncodingOffset {
    fn id(&self) -> &'static str {
        "lint/php-7.4/no-mb-strrpos-encoding-offset"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        if !named(call, "mb_strrpos") {
            return;
        }
        let arguments = call.argument_list.arguments.as_slice();
        // `getArg('encoding', 3)` and `getArg('offset', 2)`.
        let named_as = |name: &[u8]| {
            arguments
                .iter()
                .any(|a| matches!(a, Argument::Named(n) if n.name.value == name))
        };
        if arguments.len() > 3 || named_as(b"encoding") {
            return;
        }
        let Some(Argument::Positional(offset)) = arguments.get(2) else {
            return;
        };
        if offset.ellipsis.is_some() || !native_string(ancestors, offset.value) {
            return;
        }
        let arg = &arguments[2];
        out.push(Report::fixed(
            node_span(&node),
            "Pass the encoding of `mb_strrpos()` as its fourth argument, not as its offset, which PHP 8.0 no longer allows.",
            Safety::Safe,
            vec![Edit::new(
                node_span(arg),
                format!("0, {}", text(ctx, value(arg))),
            )],
        ));
    }
}

/// `getNativeType()->isString()->yes()`, read from the syntax: a string literal or concatenation, a parameter
/// declared `string`, or a local every write of which is one of those.
fn native_string(ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    if let Expression::Variable(Variable::Direct(v)) = unparenthesize(expr) {
        return match parameter_hint(ancestors, v.name) {
            Some(hint) => matches!(hint, Hint::String(_)),
            None => local_of(ancestors, v.name, &string_value),
        };
    }
    string_value(expr)
}

fn string_value(expr: &Expression<'_>) -> bool {
    match unparenthesize(expr) {
        Expression::Literal(Literal::String(_)) | Expression::CompositeString(_) => true,
        Expression::Binary(b) => matches!(b.operator, BinaryOperator::StringConcat(_)),
        _ => false,
    }
}
