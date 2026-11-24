//! Rector 2.8.4's `TernaryToNullsafeCoalesceRector`.

use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{
    Access, BinaryOperator, Call, ClassLikeMemberSelector, Expression, Node, NodeKind,
};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::expressions::{Prec, equal, in_parent, is_constant, operand, text};
use crate::rules::types::compare::mapper::expr_type;
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferNullsafeOverTernary;

impl Rule for PreferNullsafeOverTernary {
    fn id(&self) -> &'static str {
        "lint/php-8.0/prefer-nullsafe-over-ternary"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80000)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Conditional])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Conditional(ternary) = node else {
            return;
        };
        // short ternary "$a ?: $b" has no "if" branch
        let Some(then) = ternary.then else {
            return;
        };
        let Some((checked, not_identical)) = match_null_compared_expr(ternary.condition) else {
            return;
        };
        let (call, fallback) = if not_identical {
            // null !== $a ? $a->call() : $fallback
            (then, ternary.r#else)
        } else {
            // null === $a ? $fallback : $a->call()
            (ternary.r#else, then)
        };
        let (call, fallback) = (unparenthesize(call), unparenthesize(fallback));
        // re-evaluating the checked expression must stay free of side effects
        if !is_pure_expr(checked) {
            return;
        }
        let Some(arrow) = nullsafe_arrow(ctx, call, checked) else {
            return;
        };
        let start = node_span(call).start;
        let call_text = text(ctx, call);
        let nullsafe = format!(
            "{}?{}",
            &call_text[..arrow - start],
            &call_text[arrow - start..]
        );
        let span = node_span(ternary);
        let code = if is_constant(fallback, "null") {
            // "$a !== null ? $a->call() : null" needs no fallback at all
            nullsafe
        } else {
            if may_be_null(&facts(ctx), ancestors, call) {
                return;
            }
            // reading a property on null is already suppressed by "??", so the plain "->" chain stays for
            // property-only access, while a method call must stay nullsafe
            let left = if is_pure_property_chain(ctx, call, checked) {
                call_text.to_string()
            } else {
                nullsafe
            };
            in_parent(
                span,
                ancestors,
                format!("{left} ?? {}", operand(ctx, fallback, 141, None)),
                Prec::Infix(140, 140, 141),
            )
        };
        out.push(Report::fixed(
            span,
            "Use the nullsafe operator instead of a ternary that checks for `null`.",
            Safety::Unsafe,
            vec![Edit::new(span, code)],
        ));
    }
}

/// The expression compared against null, and whether the comparison is `!==`.
fn match_null_compared_expr<'a>(expr: &'a Expression<'a>) -> Option<(&'a Expression<'a>, bool)> {
    let Expression::Binary(binary) = unparenthesize(expr) else {
        return None;
    };
    let not_identical = match binary.operator {
        BinaryOperator::NotIdentical(_) => true,
        BinaryOperator::Identical(_) => false,
        _ => return None,
    };
    if is_constant(binary.lhs, "null") {
        return Some((unparenthesize(binary.rhs), not_identical));
    }
    if is_constant(binary.rhs, "null") {
        return Some((unparenthesize(binary.lhs), not_identical));
    }
    None
}

/// `createNullsafeChain`: the offset of the `->` of the deepest link of the chain whose object is `checked`.
fn nullsafe_arrow(
    ctx: &Context<'_>,
    expr: &Expression<'_>,
    checked: &Expression<'_>,
) -> Option<usize> {
    let (object, arrow): (&Expression<'_>, Span) = match unparenthesize(expr) {
        Expression::Call(Call::Method(call)) => (call.object, crate::syntax::span(call.arrow)),
        Expression::Access(Access::Property(fetch)) => {
            (fetch.object, crate::syntax::span(fetch.arrow))
        }
        _ => return None,
    };
    if equal(ctx, object, checked) {
        return Some(arrow.start);
    }
    nullsafe_arrow(ctx, object, checked)
}

/// A chain of plain property fetches rooted in the checked expression, e.g. "$a->b->c".
fn is_pure_property_chain(
    ctx: &Context<'_>,
    expr: &Expression<'_>,
    checked: &Expression<'_>,
) -> bool {
    if equal(ctx, expr, checked) {
        return true;
    }
    match unparenthesize(expr) {
        Expression::Access(Access::Property(fetch)) => {
            is_pure_property_chain(ctx, fetch.object, checked)
        }
        _ => false,
    }
}

fn is_pure_expr(expr: &Expression<'_>) -> bool {
    match unparenthesize(expr) {
        Expression::Variable(_) => true,
        Expression::Access(Access::Property(fetch)) => is_pure_expr(fetch.object),
        Expression::Access(Access::StaticProperty(_)) => true,
        _ => false,
    }
}

/// `shouldSkipCoalesceFallback`: `! $this->getType($callExpr)->isNull()->no()`. PHPStan types a fetch or call on a
/// nullable object by the member's declared type, so that type is read when the object's classes are known.
fn may_be_null(facts: &Facts, ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    let declared = match unparenthesize(expr) {
        Expression::Access(Access::Property(fetch)) => match &fetch.property {
            ClassLikeMemberSelector::Identifier(name) => {
                let name = [b"$", name.value].concat();
                member_types(facts, fetch.object, |class| {
                    let p = facts.documented().get_declaring_property(class, &name)?;
                    p.type_metadata.as_ref().map(|t| &t.type_union)
                })
            }
            _ => None,
        },
        Expression::Call(Call::Method(call)) => match &call.method {
            ClassLikeMemberSelector::Identifier(name) => {
                member_types(facts, call.object, |class| {
                    let m = facts.documented().get_declaring_method(class, name.value)?;
                    (m.return_type_metadata.as_ref()).map(|t| &t.type_union)
                })
            }
            _ => None,
        },
        _ => None,
    };
    match declared {
        Some(types) => types.iter().any(|t| t.is_mixed() || t.has_null()),
        None => !expr_type(facts, ancestors, expr, false).is_null().no(),
    }
}

fn member_types<'f>(
    facts: &'f Facts,
    object: &Expression<'_>,
    member: impl Fn(&[u8]) -> Option<&'f TUnion>,
) -> Option<Vec<&'f TUnion>> {
    let t = facts.full_type(node_span(unparenthesize(object)))?;
    let classes: Vec<&[u8]> = (t.types.iter())
        .filter(|a| !a.is_null())
        .map(|a| match a {
            TAtomic::Object(TObject::Named(n)) => Some(n.name.as_bytes()),
            _ => None,
        })
        .collect::<Option<_>>()?;
    if classes.is_empty() {
        return None;
    }
    classes.into_iter().map(member).collect()
}
