use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{BinaryOperator, Expression};

use super::facts::Facts;
use crate::syntax::{node_span, unparenthesize};

pub(crate) enum RectorType<'f> {
    Built,
    Scope(Option<&'f TUnion>),
}

#[derive(Clone, Copy)]
struct Shape {
    union: bool,
    null: bool,
}

impl RectorType<'_> {
    fn shape(&self) -> Shape {
        match self {
            RectorType::Built => Shape {
                union: true,
                null: false,
            },
            RectorType::Scope(t) => Shape {
                union: t.is_some_and(is_union),
                null: t.is_some_and(|t| {
                    !t.types.is_empty() && t.types.iter().all(|a| matches!(a, TAtomic::Null))
                }),
            },
        }
    }
}

pub(crate) fn is_union(t: &TUnion) -> bool {
    let bools = (t.types.iter())
        .filter(|a| matches!(a, TAtomic::Scalar(s) if s.is_bool()))
        .count();
    t.types.len() - bools.saturating_sub(1) > 1
}

fn union_typeable(first: Shape, second: Shape) -> bool {
    !first.union && !second.union && !second.null
}

pub(crate) fn get_type<'f>(facts: &'f Facts, expr: &Expression<'_>) -> RectorType<'f> {
    let expr = unparenthesize(expr);
    let scope = || RectorType::Scope(facts.full_type(node_span(expr)));
    let built = match expr {
        Expression::Conditional(c) => {
            let branches = c.then.is_some_and(|then| {
                union_typeable(
                    get_type(facts, then).shape(),
                    get_type(facts, c.r#else).shape(),
                )
            });
            let nullable = || match get_type(facts, c.condition) {
                RectorType::Scope(Some(t)) => {
                    is_union(t) && t.types.iter().any(|a| matches!(a, TAtomic::Null))
                }
                _ => false,
            };
            branches || (nullable() && union_typeable(SINGLE, get_type(facts, c.r#else).shape()))
        }
        Expression::Binary(b) if matches!(b.operator, BinaryOperator::NullCoalesce(_)) => {
            union_typeable(
                get_type(facts, b.lhs).shape(),
                get_type(facts, b.rhs).shape(),
            )
        }
        _ => false,
    };
    if built { RectorType::Built } else { scope() }
}

const SINGLE: Shape = Shape {
    union: false,
    null: false,
};
