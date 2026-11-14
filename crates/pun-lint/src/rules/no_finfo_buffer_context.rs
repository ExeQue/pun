use mago_syntax::cst::{Argument, ArgumentList, ClassLikeMemberSelector, Node, NodeKind};
use pun_problem::{Fix, Safety};

use super::lint_b_helpers::{is_object_of, remove_argument};
use super::rector_code_quality::named;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoFinfoBufferContext;

impl Rule for NoFinfoBufferContext {
    fn id(&self) -> &'static str {
        "lint/php-8.5/no-finfo-buffer-context"
    }

    fn min_php_version(&self) -> u32 {
        80500
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall, NodeKind::MethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (list, position) = match node {
            Node::FunctionCall(call) if named(call, "finfo_buffer") => (&call.argument_list, 3),
            Node::MethodCall(call) => {
                let ClassLikeMemberSelector::Identifier(name) = &call.method else {
                    return;
                };
                if !name.value.eq_ignore_ascii_case(b"buffer")
                    || !is_object_of(ancestors, call.object, "finfo", node_span(call).start)
                {
                    return;
                }
                (&call.argument_list, 2)
            }
            _ => return,
        };
        let Some(index) = context_arg(list, position) else {
            return;
        };
        out.push(Report {
            span: node_span(&list.arguments.as_slice()[index]),
            message: "Remove the `$context` argument, which PHP 8.5 deprecates.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![remove_argument(list, index)],
            }),
        });
    }
}

fn context_arg(list: &ArgumentList<'_>, position: usize) -> Option<usize> {
    let args = list.arguments.as_slice();
    if args.len() < position {
        return None;
    }
    if !args.iter().any(|a| matches!(a, Argument::Named(_))) {
        return (args.len() > position).then_some(position);
    }
    args.iter().position(
        |a| matches!(a, Argument::Named(n) if n.name.value.eq_ignore_ascii_case(b"context")),
    )
}
