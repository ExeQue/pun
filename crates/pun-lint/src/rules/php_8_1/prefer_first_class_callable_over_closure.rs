//! Rector 2.8.4's `ClosureDelegatingCallToFirstClassCallableRector`.

use mago_syntax::cst::{Node, NodeKind, Statement};

use super::delegating_callable::refactor;
use crate::{Context, Report, Rule};

pub struct PreferFirstClassCallableOverClosure;

impl Rule for PreferFirstClassCallableOverClosure {
    fn id(&self) -> &'static str {
        "lint/php-8.1/prefer-first-class-callable-over-closure"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80100)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Closure])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Closure(closure) = node else {
            return;
        };
        // must have exactly 1 stmt with Return_
        let [Statement::Return(r#return)] = closure.body.statements.as_slice() else {
            return;
        };
        let Some(call) = r#return.value else {
            return;
        };
        refactor(ctx, node, ancestors, &closure.parameter_list, call, out);
    }
}
