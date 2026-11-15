use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_syntax::cst::{Expression, Literal};

use crate::Context;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};

pub(crate) fn native_is_integer(ctx: &Context<'_>, expr: &Expression<'_>) -> bool {
    native_parts(ctx, expr, TScalar::is_int)
}

pub(crate) fn native_is_string(ctx: &Context<'_>, expr: &Expression<'_>) -> bool {
    native_parts(ctx, expr, TScalar::is_any_string)
}

fn native_parts(ctx: &Context<'_>, expr: &Expression<'_>, part: fn(&TScalar) -> bool) -> bool {
    // A literal's type comes from its syntax: Mago types no expression in code after a `never` call, where PHPStan
    // still does.
    match unparenthesize(expr) {
        Expression::Literal(Literal::Integer(_)) => return part(&TScalar::int()),
        Expression::Literal(Literal::String(_)) => return part(&TScalar::string()),
        Expression::Literal(_) => return false,
        _ => {}
    }
    let facts = facts(ctx);
    let Some(t) = facts.native_type(node_span(unparenthesize(expr))) else {
        return false;
    };
    let parts: Vec<&TAtomic> = t
        .types
        .iter()
        .filter(|a| !matches!(a, TAtomic::Null))
        .collect();
    !parts.is_empty()
        && parts
            .iter()
            .all(|a| matches!(a, TAtomic::Scalar(s) if part(s)))
}
