//! Rector 2.8.4's `TernaryToSpaceshipRector`.

use mago_syntax::cst::{BinaryOperator, Conditional, Expression, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::expressions::{equal, operand};
use crate::rules::support::values::{Value, value};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferSpaceship;

impl Rule for PreferSpaceship {
    fn id(&self) -> &'static str {
        "lint/php-7.0/prefer-spaceship"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70000)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Conditional])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Conditional(ternary) = node else {
            return;
        };
        let Some((cond, nested, nested_cond)) = should_skip(ctx, ternary) else {
            return;
        };
        let values = [ternary.then, nested.then, Some(nested.r#else)];
        if !are_values(&values, [-1, 1, 0]) {
            return;
        }
        // `processSmallerThanTernary`, then `processGreaterThanTernary`.
        let (first, second) = match (&cond.operator, &nested_cond.operator) {
            (BinaryOperator::LessThan(_), BinaryOperator::GreaterThan(_)) => (cond.lhs, cond.rhs),
            (BinaryOperator::GreaterThan(_), BinaryOperator::LessThan(_)) => (cond.rhs, cond.lhs),
            _ => return,
        };
        out.push(Report::fixed(
            node_span(ternary),
            "Use the spaceship operator `<=>` instead of this nested ternary.",
            Safety::Safe,
            vec![Edit::new(node_span(ternary), spaceship(ctx, first, second))],
        ));
    }
}

type Matched<'a> = (
    &'a mago_syntax::cst::Binary<'a>,
    &'a Conditional<'a>,
    &'a mago_syntax::cst::Binary<'a>,
);

/// `shouldSkip`, returning the two conditions and the nested ternary when the rule applies.
fn should_skip<'a>(ctx: &Context<'_>, ternary: &'a Conditional<'a>) -> Option<Matched<'a>> {
    let Expression::Binary(cond) = unparenthesize(ternary.condition) else {
        return None;
    };
    let Expression::Conditional(nested) = unparenthesize(ternary.r#else) else {
        return None;
    };
    let Expression::Binary(nested_cond) = unparenthesize(nested.condition) else {
        return None;
    };
    // $a X $b ? . : ($a X $b ? . : .)
    if !equal(ctx, cond.lhs, nested_cond.lhs) || !equal(ctx, cond.rhs, nested_cond.rhs) {
        return None;
    }
    Some((cond, nested, nested_cond))
}

/// `ValueResolver::areValuesEqual` on integer literals.
pub(crate) fn are_values<const N: usize>(
    exprs: &[Option<&Expression<'_>>; N],
    expected: [i64; N],
) -> bool {
    exprs
        .iter()
        .zip(expected)
        .all(|(expr, int)| expr.and_then(value) == Some(Value::Int(int)))
}

/// `new Spaceship($first, $second)`, printed.
pub(crate) fn spaceship(
    ctx: &Context<'_>,
    first: &Expression<'_>,
    second: &Expression<'_>,
) -> String {
    format!(
        "{} <=> {}",
        operand(ctx, first, 80, Some(80)),
        operand(ctx, second, 80, None)
    )
}
