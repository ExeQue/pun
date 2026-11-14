use mago_syntax::cst::{Node, NodeKind};

use super::{Is, Trinary, function_matcher, type_is};
use crate::{Context, Report, Rule};

pub struct PreferToEndWith;

impl Rule for PreferToEndWith {
    fn id(&self) -> &'static str {
        "lint/pest/prefer-to-end-with"
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
            "str_ends_with",
            "toEndWith",
            |values| match values {
                [subject, other] if type_is(ctx, subject, Is::String) == Trinary::Yes => {
                    Some((*subject, vec![*other]))
                }
                _ => None,
            },
            out,
        );
    }
}
