use mago_syntax::cst::{Expression, Node, NodeKind, Statement};

use super::facts::facts;
use super::silent_void::{Place, has_silent_void};
use super::{SCALAR_TYPES, add_return_type, class_scope, returns_scoped, untyped};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct AddArrayReturn;

impl Rule for AddArrayReturn {
    fn id(&self) -> &'static str {
        "lint/types/add-array-return-type"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(SCALAR_TYPES)
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
        let direct = f.body.statements.iter().any(|s| {
            matches!(
                s,
                Statement::Return(r) if matches!(r.value, Some(Expression::Array(_) | Expression::LegacyArray(_)))
            )
        });
        if !direct {
            return;
        }
        let returns = returns_scoped(f.body);
        let facts = facts(ctx);
        let scope = class_scope(ancestors);
        let place = Place {
            facts: &facts,
            ancestors,
            scope: scope.as_ref(),
        };
        let arrays = returns.iter().all(|r| {
            r.value.is_some_and(|v| {
                facts
                    .native_type(node_span(v))
                    .is_some_and(mago_codex::ttype::union::TUnion::is_array)
            })
        });
        if !returns.is_empty() && arrays && !has_silent_void(&place, f.body.statements.iter()) {
            out.push(add_return_type(f.span, f.at, "array"));
        }
    }
}
