//! Rector 2.8.4's `IsCountableRector`, and `IsArrayAndDualCheckToAble`, which `IsIterableRector` shares.

use mago_syntax::cst::{Binary, BinaryOperator, Expression, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::unpacked;
use crate::rules::support::expressions::{call_to, equal, text};
use crate::rules::support::names::class_name;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferIsCountable;

impl Rule for PreferIsCountable {
    fn id(&self) -> &'static str {
        "lint/php-7.3/prefer-is-countable"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70300)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Binary])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Binary(binary) = node else {
            return;
        };
        if let Some(report) = dual_check(ancestors, ctx, binary, "Countable", "is_countable") {
            out.push(report);
        }
    }
}

/// `processBooleanOr`: `is_array($x) || $x instanceof $class`, in either order, becomes `$function($x)`.
pub(crate) fn dual_check(
    ancestors: &[Node<'_, '_>],
    ctx: &Context<'_>,
    binary: &Binary<'_>,
    class: &str,
    function: &str,
) -> Option<Report> {
    let BinaryOperator::Or(_) = binary.operator else {
        return None;
    };
    // `matchFirstAndSecondConditionNode($booleanOr, Instanceof_::class, FuncCall::class)`.
    let (instanceof, call) = match (unparenthesize(binary.lhs), unparenthesize(binary.rhs)) {
        (Expression::Binary(i), Expression::Call(_)) if i.operator.is_instanceof() => {
            (i, binary.rhs)
        }
        (Expression::Call(_), Expression::Binary(i)) if i.operator.is_instanceof() => {
            (i, binary.lhs)
        }
        _ => return None,
    };
    let Expression::Identifier(name) = instanceof.rhs else {
        return None;
    };
    if class_name(ancestors, name) != class {
        return None;
    }
    let call = call_to(call, "is_array")?;
    let first = call.argument_list.arguments.first()?;
    if unpacked(first) {
        return None;
    }
    let first = crate::rules::support::arguments::value(first);
    if !equal(ctx, instanceof.lhs, first) {
        return None;
    }
    let span = node_span(binary);
    Some(Report::fixed(
        span,
        format!("Use `{function}()` instead of `is_array()` and an `instanceof {class}` check."),
        Safety::Safe,
        vec![Edit::new(span, format!("{function}({})", text(ctx, first)))],
    ))
}
