//! Rector 2.8.4's `MyCLabsClassToEnumRector`.

use mago_syntax::cst::{Node, NodeKind};

use super::enum_factory::from_class;
use crate::problem::Safety;
use crate::rules::refactor::is_class;
use crate::rules::support::class_members::qualified;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferNativeEnumOverMyCLabs;

impl Rule for PreferNativeEnumOverMyCLabs {
    fn id(&self) -> &'static str {
        "lint/php-8.1/prefer-native-enum-over-myclabs"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80100)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Class(class) = node else { return };
        if !is_class(
            ctx,
            &qualified(ancestors, class.name.value),
            "MyCLabs\\Enum\\Enum",
        ) {
            return;
        }
        out.push(Report::fixed(
            node_span(&class.name),
            "Turn the MyCLabs enum class into a native enum.",
            Safety::Unsafe,
            vec![from_class(ctx, ancestors, class)],
        ));
    }
}
