use mago_syntax::cst::{Node, NodeKind};
use pun_problem::Safety;

use super::annotations::Annotated;
use super::{class_is, is_in_test_class};
use crate::rules::support::names::resolve_class_name;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub(super) const ALLOW_MOCK_OBJECTS: &str =
    "PHPUnit\\Framework\\Attributes\\AllowMockObjectsWithoutExpectations";
pub(super) const ATTRIBUTES: u32 = 80000;

pub struct AddAllowMockObjectsAttribute;

impl Rule for AddAllowMockObjectsAttribute {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-allow-mock-objects-attribute"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(ATTRIBUTES)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let mut scope = ancestors.to_vec();
        scope.push(node);
        if !should_add(ctx, &scope, node)
            || !class_is(
                ctx,
                &scope,
                &["Symfony\\Component\\Form\\Test\\TypeTestCase"],
            )
        {
            return;
        }
        add_attribute(ctx, ancestors, node, out);
    }
}

pub(super) fn should_add(ctx: &Context<'_>, scope: &[Node<'_, '_>], node: Node<'_, '_>) -> bool {
    let filled = node.children().into_iter().any(|child| {
        matches!(child, Node::AttributeList(list) if list.attributes.iter().any(|a| {
            resolve_class_name(scope, &a.name).eq_ignore_ascii_case(ALLOW_MOCK_OBJECTS)
        }))
    });
    is_in_test_class(ctx, scope, None)
        && facts(ctx)
            .codebase
            .get_class_like(ALLOW_MOCK_OBJECTS.as_bytes())
            .is_some()
        && !filled
}

pub(super) fn add_attribute(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    node: Node<'_, '_>,
    out: &mut Vec<Report>,
) {
    let Some(edits) = Annotated::of(ctx, node, ancestors)
        .and_then(|a| a.edits(ctx.source, None, &[format!("#[\\{ALLOW_MOCK_OBJECTS}]")]))
    else {
        return;
    };
    out.push(Report::fixed(
        node_span(&node),
        "Allow mock objects without expectations in this test class.",
        Safety::Safe,
        edits,
    ));
}
