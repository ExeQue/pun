use mago_syntax::cst::{Expression, Literal, Node, NodeKind};

use super::facts::facts;
use super::silent_void::Place;
use super::{add_return_type, class_scope, has_only_return_with_expr, returns_scoped, untyped};
use crate::{Context, Report, Rule};

pub struct AddBoolReturnFromConst;

impl Rule for AddBoolReturnFromConst {
    fn id(&self) -> &'static str {
        "lint/types/add-bool-return-type-from-const"
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
        let boolean = |e: &Expression<'_>| {
            matches!(e, Expression::Literal(Literal::True(_) | Literal::False(_)))
        };
        // The literals are checked first: the check of the body's paths needs the types.
        if !returns.iter().all(|r| r.value.is_some_and(boolean)) {
            return;
        }
        let scope = class_scope(ancestors);
        let place = Place {
            facts: &facts(ctx),
            ancestors,
            scope: scope.as_ref(),
        };
        if has_only_return_with_expr(&place, f.body, &returns) {
            out.push(add_return_type(f.span, f.at, "bool"));
        }
    }
}
