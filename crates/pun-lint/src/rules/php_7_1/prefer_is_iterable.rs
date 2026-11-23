//! Rector 2.8.4's `IsIterableRector`.

use mago_syntax::cst::{Node, NodeKind};

use crate::rules::php_7_3::prefer_is_countable::dual_check;
use crate::{Context, Report, Rule};

pub struct PreferIsIterable;

impl Rule for PreferIsIterable {
    fn id(&self) -> &'static str {
        "lint/php-7.1/prefer-is-iterable"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70100)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Binary])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Binary(binary) = node else {
            return;
        };
        if let Some(report) = dual_check(ancestors, ctx, binary, "Traversable", "is_iterable") {
            out.push(report);
        }
    }
}
