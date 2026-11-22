//! Port of Mago 1.61.0's `prefer-null-coalescing-assignment`: `$x ?? ($x = $y)` becomes `$x ??= $y`. The fix is safe
//! when the target is a plain reference and unsafe (Mago's "potentially unsafe") otherwise. When the assigned value is
//! a reference (`$x ?? ($x = &$y)`), Mago's fix would write `$x ??= &$y`, which does not parse, so Mago rejects it;
//! Pun reports the same issue without a fix.

use crate::problem::{Edit, Fix, Safety};
use mago_span::HasSpan;
use mago_syntax::cst::{
    Access, ClassLikeMemberSelector, Expression, Node, NodeKind, UnaryPrefixOperator, Variable,
};

use crate::syntax::{span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferCoalesceAssignOverFallback;

impl Rule for PreferCoalesceAssignOverFallback {
    fn id(&self) -> &'static str {
        "lint/operators/prefer-coalesce-assign-over-fallback"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70400)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Binary])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Binary(binary) = node else { return };
        if !binary.operator.is_null_coalesce() {
            return;
        }
        let Expression::Assignment(assignment) = unparenthesize(binary.rhs) else {
            return;
        };
        let text = |e: &Expression<'_>| {
            let s = span(e.span());
            &ctx.source[s.start..s.end]
        };
        if !assignment.operator.is_assign() || text(binary.lhs) != text(assignment.lhs) {
            return;
        }
        let by_reference = matches!(
            assignment.rhs,
            Expression::UnaryPrefix(prefix) if matches!(prefix.operator, UnaryPrefixOperator::Reference(_))
        );
        let fix = (!by_reference).then(|| Fix {
            safety: if is_plain_reference(binary.lhs) {
                Safety::Safe
            } else {
                Safety::Unsafe
            },
            edits: vec![
                Edit::new(
                    span(binary.operator.span().to_end(assignment.rhs.span().start)),
                    "??= ",
                ),
                Edit::new(
                    span(binary.span().from_start(assignment.rhs.span().end)),
                    "",
                ),
            ],
        });
        out.push(Report {
            span: span(binary.span()),
            message: "Use the null coalescing assignment operator `??=`.".to_string(),
            help: Some(
                "Replace `$value ?? ($value = $default)` with `$value ??= $default`.".to_string(),
            ),
            fix,
        });
    }
}

/// Whether evaluating the expression twice, as `??` followed by `=` does, reads the same target without side effects.
fn is_plain_reference(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::Variable(Variable::Direct(_)) => true,
        Expression::Access(Access::Property(access)) => {
            matches!(access.property, ClassLikeMemberSelector::Identifier(_))
                && is_plain_reference(access.object)
        }
        Expression::Access(Access::StaticProperty(access)) => {
            matches!(
                access.class,
                Expression::Identifier(_)
                    | Expression::Self_(_)
                    | Expression::Static(_)
                    | Expression::Parent(_)
            ) && matches!(access.property, Variable::Direct(_))
        }
        Expression::ArrayAccess(access) => {
            matches!(
                access.index,
                Expression::Literal(_) | Expression::Variable(Variable::Direct(_))
            ) && is_plain_reference(access.array)
        }
        _ => false,
    }
}
