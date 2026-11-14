use mago_syntax::cst::{Node, NodeKind};

use crate::rules::support::names::function_call_matches;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct RequireAssertDescription;

impl Rule for RequireAssertDescription {
    fn id(&self) -> &'static str {
        "lint/functions/require-assert-description"
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
        if function_call_matches(ancestors, call, "assert")
            && call.argument_list.arguments.get(1).is_none()
        {
            out.push(Report {
                span: node_span(call),
                message: "Missing description in assert function.".into(),
                help: Some(
                    "Add a description to the assert function to clarify its purpose.".into(),
                ),
                fix: None,
            });
        }
    }
}
