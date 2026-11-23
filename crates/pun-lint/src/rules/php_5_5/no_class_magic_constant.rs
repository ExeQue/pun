//! Rector 2.8.4's `ClassConstantToSelfClassRector`.

use mago_syntax::cst::{MagicConstant, Node, NodeKind};

use super::scope::class_like;
use crate::problem::{Edit, Safety};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoClassMagicConstant;

impl Rule for NoClassMagicConstant {
    fn id(&self) -> &'static str {
        "lint/php-5.5/no-class-magic-constant"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(50500)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MagicConstant])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::MagicConstant(MagicConstant::Class(_)) = node else {
            return;
        };
        // Rector also rewrites a `__CLASS__` outside any class, where `self::class` is a compile error.
        if class_like(ancestors).is_none() {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Use `self::class` instead of `__CLASS__`.",
            Safety::Safe,
            vec![Edit::new(node_span(&node), "self::class")],
        ));
    }
}
