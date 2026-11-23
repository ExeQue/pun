use mago_syntax::cst::{Call, Expression, Node, NodeKind};

use super::add_bool_return::is_bool_expr;
use super::facts::{Facts, facts};
use super::silent_void::Place;
use super::{add_return_type, class_scope, has_only_return_with_expr, returns_scoped, untyped};
use crate::rules::support::expressions::is_constant;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct AddBoolReturnFromMixed;

impl Rule for AddBoolReturnFromMixed {
    fn id(&self) -> &'static str {
        "lint/types/add-bool-return-type-from-mixed-returns"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(super::SCALAR_TYPES)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Function, NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(f) = untyped(node, ancestors, ctx) else {
            return;
        };
        let returns = returns_scoped(f.body);
        let scope = class_scope(ancestors);
        let facts = facts(ctx);
        let place = Place {
            facts: &facts,
            ancestors,
            scope: scope.as_ref(),
        };
        if !has_only_return_with_expr(&place, f.body, &returns) {
            return;
        }
        // Every return must be bool, mixing at least one true/false constant with at least one strict bool
        // expression: the other two bool rules take the returns that are all of one kind.
        let (mut constant, mut strict) = (false, false);
        for value in returns.iter().filter_map(|r| r.value) {
            if is_constant(value, "true") || is_constant(value, "false") {
                constant = true;
            } else if is_bool_expr(unparenthesize(value))
                || is_call_returning_native_bool(&facts, value)
            {
                strict = true;
            } else {
                return;
            }
        }
        if constant && strict {
            out.push(add_return_type(f.span, f.at, "bool"));
        }
    }
}

/// Rector's `isCallLikeReturnNativeBool()`: a call or `new` whose native type is `bool`.
fn is_call_returning_native_bool(facts: &Facts, expr: &Expression<'_>) -> bool {
    let expr = unparenthesize(expr);
    matches!(
        expr,
        Expression::Call(
            Call::Function(_) | Call::Method(_) | Call::NullSafeMethod(_) | Call::StaticMethod(_)
        )
    ) && facts
        .native_type(node_span(expr))
        .is_some_and(mago_codex::ttype::union::TUnion::is_bool)
}
