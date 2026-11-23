//! Rector 2.8.4's `RandomFunctionRector`.

use mago_syntax::cst::{Argument, Expression, Literal, Node, NodeKind, UnaryPrefixOperator};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{named, text};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

/// `OLD_TO_NEW_FUNCTION_NAMES`.
const OLD_TO_NEW_FUNCTION_NAMES: &[(&str, &str)] = &[
    ("getrandmax", "mt_getrandmax"),
    ("srand", "mt_srand"),
    ("rand", "random_int"),
];

pub struct PreferRandomInt;

impl Rule for PreferRandomInt {
    fn id(&self) -> &'static str {
        "lint/php-7.0/prefer-random-int"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70000)
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
        let Node::FunctionCall(call) = node else {
            return;
        };
        let arguments = call.argument_list.arguments.as_slice();
        if matches!(arguments, [Argument::Positional(a)] if a.ellipsis.is_some()) {
            return;
        }
        let Some((old, new)) = OLD_TO_NEW_FUNCTION_NAMES
            .iter()
            .find(|(old, _)| named(call, old))
        else {
            return;
        };
        let mut edits = vec![Edit::new(node_span(call.function), *new)];
        if *new == "random_int" {
            match arguments {
                [] => edits.push(Edit::new(
                    Span::new(
                        call.argument_list.left_parenthesis.start.offset as usize,
                        call.argument_list.right_parenthesis.end.offset as usize,
                    ),
                    "(0, mt_getrandmax())",
                )),
                [min, max] => {
                    if let (Some(low), Some(high)) = (int(value(min)), int(value(max)))
                        && low > high
                    {
                        edits.push(Edit::new(node_span(min), text(ctx, max)));
                        edits.push(Edit::new(node_span(max), text(ctx, min)));
                    }
                }
                _ => {}
            }
        }
        out.push(Report::fixed(
            node_span(&node),
            format!("Call `{new}()` instead of `{old}()`."),
            Safety::Unsafe,
            edits,
        ));
    }
}

/// `valueResolver->getValue()` when it is an `int`: an integer literal, negated or not.
fn int(expr: &Expression<'_>) -> Option<i128> {
    match unparenthesize(expr) {
        Expression::Literal(Literal::Integer(i)) => i.value.map(i128::from),
        Expression::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Negation(_)) => {
            int(u.operand).map(|v| -v)
        }
        _ => None,
    }
}
