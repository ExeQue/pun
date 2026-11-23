//! Rector 2.8.4's `GetCalledClassToStaticClassRector`.

use mago_syntax::cst::{Node, NodeKind};

use super::scope::{ClassLike, class_like};
use crate::problem::{Edit, Safety};
use crate::rules::support::expressions::named;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferStaticOverGetCalledClass;

impl Rule for PreferStaticOverGetCalledClass {
    fn id(&self) -> &'static str {
        "lint/php-5.5/prefer-static-over-get-called-class"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(50500)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        if !named(call, "get_called_class") {
            return;
        }
        // In a class that is not anonymous and not `final`.
        if !matches!(class_like(ancestors), Some(ClassLike::Open)) {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Use `static::class` instead of `get_called_class()`.",
            Safety::Safe,
            vec![Edit::new(node_span(&node), "static::class")],
        ));
    }
}
