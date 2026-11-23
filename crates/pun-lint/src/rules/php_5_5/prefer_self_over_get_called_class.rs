//! Rector 2.8.4's `GetCalledClassToSelfClassRector`.

use mago_syntax::cst::{Node, NodeKind};

use super::scope::{ClassLike, class_like, in_anonymous_function};
use crate::problem::{Edit, Safety};
use crate::rules::support::expressions::named;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferSelfOverGetCalledClass;

impl Rule for PreferSelfOverGetCalledClass {
    fn id(&self) -> &'static str {
        "lint/php-5.5/prefer-self-over-get-called-class"
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
        let Some(class) = class_like(ancestors) else {
            return;
        };
        // `isInsideFinalClass($node)`, or `$scope->isInAnonymousFunction()`.
        if !matches!(class, ClassLike::Final) && !in_anonymous_function(ancestors) {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Use `self::class` instead of `get_called_class()`.",
            Safety::Unsafe,
            vec![Edit::new(node_span(&node), "self::class")],
        ));
    }
}
