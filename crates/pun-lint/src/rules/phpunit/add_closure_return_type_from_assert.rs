use mago_syntax::cst::{Node, NodeKind};

use super::{AnyCall, contains, is_in_test_class};
use crate::rules::types::closure_return;
use crate::{Context, Report, Rule};

const SCALAR_TYPES: u32 = 70000;

pub struct AddClosureReturnTypeFromAssert;

impl Rule for AddClosureReturnTypeFromAssert {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-closure-return-type-from-assert"
    }

    fn min_php_version(&self) -> u32 {
        SCALAR_TYPES
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
        if closure.return_type_hint.is_some() || !is_in_test_class(ctx, ancestors, None) {
            return;
        }
        if ancestors.iter().rev().find_map(|a| match a {
            Node::Trait(_) => Some(true),
            Node::Class(_) | Node::AnonymousClass(_) | Node::Enum(_) | Node::Interface(_) => {
                Some(false)
            }
            _ => None,
        }) == Some(true)
        {
            return;
        }
        let asserts = closure.body.statements.iter().any(|s| {
            contains(Node::Statement(s), &|n| {
                AnyCall::of(n).is_some_and(|c| c.is_name(&["assertInstanceOf"]))
            })
        });
        if asserts {
            closure_return(node, ancestors, ctx, out, true);
        }
    }
}
