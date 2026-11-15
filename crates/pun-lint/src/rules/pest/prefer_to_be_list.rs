use mago_syntax::cst::{Node, NodeKind};

use super::{Is, Trinary, function_matcher, type_is};
use crate::{Context, Report, Rule};

pub struct PreferToBeList;

impl Rule for PreferToBeList {
    fn id(&self) -> &'static str {
        "lint/pest/prefer-to-be-list"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::MethodCall(call) = node else {
            return;
        };
        function_matcher(
            ctx,
            call,
            "array_is_list",
            "toBeList",
            |values| match values {
                [value] if type_is(ctx, value, Is::Array) == Trinary::Yes => Some((*value, vec![])),
                _ => None,
            },
            out,
        );
    }
}
