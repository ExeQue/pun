//! Rector 2.8.4's `ArrowFunctionDelegatingCallToFirstClassCallableRector`.

use mago_syntax::cst::{Node, NodeKind};

use super::delegating_callable::refactor;
use crate::{Context, Report, Rule};

pub struct PreferFirstClassCallableOverArrowFunction;

impl Rule for PreferFirstClassCallableOverArrowFunction {
    fn id(&self) -> &'static str {
        "lint/php-8.1/prefer-first-class-callable-over-arrow-function"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80100)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ArrowFunction])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::ArrowFunction(arrow) = node else {
            return;
        };
        refactor(
            ctx,
            node,
            ancestors,
            &arrow.parameter_list,
            arrow.expression,
            out,
        );
    }
}
