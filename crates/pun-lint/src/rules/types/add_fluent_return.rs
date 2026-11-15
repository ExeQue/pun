use mago_syntax::cst::{Node, NodeKind};

use super::facts::facts;
use super::native::{Kind, returned_kinds};
use super::silent_void::Place;
use super::{add_return_type, class_scope, returns_scoped, untyped};
use crate::{Context, Report, Rule};

pub struct AddFluentReturn;

const HAS_RETURN_TYPE: u32 = 70000;
const STATIC_RETURN_TYPE: u32 = 80000;

impl Rule for AddFluentReturn {
    fn id(&self) -> &'static str {
        "lint/types/add-fluent-return-type"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(HAS_RETURN_TYPE)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method])
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
        let Some(scope) = class_scope(ancestors) else {
            return;
        };
        let facts = facts(ctx);
        let place = Place {
            facts: &facts,
            ancestors,
            scope: Some(&scope),
        };
        let returns = returns_scoped(f.body);
        let Some(kinds) = returned_kinds(&place, f.body, &returns) else {
            return;
        };
        let Some((name, is_static, is_this)) = one_class(&kinds) else {
            return;
        };
        let same_class = scope
            .name
            .as_deref()
            .is_some_and(|n| n.eq_ignore_ascii_case(name));
        let self_or_static = if scope.is_final || ctx.php_version.id() < STATIC_RETURN_TYPE {
            "self"
        } else {
            "static"
        };
        let r#type = if is_this || (is_static && same_class) {
            self_or_static
        } else if same_class {
            "self"
        } else {
            return;
        };
        out.push(add_return_type(f.span, f.at, r#type));
    }
}

fn one_class(kinds: &[Kind]) -> Option<(&str, bool, bool)> {
    let mut objects = kinds.iter().map(|k| match k {
        Kind::Object {
            name,
            is_static,
            is_this,
        } => Some((name.as_str(), *is_static, *is_this)),
        _ => None,
    });
    let first = objects.next()??;
    objects.try_fold(first, |(name, is_static, is_this), next| {
        let (other, other_static, other_this) = next?;
        name.eq_ignore_ascii_case(other).then_some((
            name,
            is_static && other_static,
            is_this && other_this,
        ))
    })
}
