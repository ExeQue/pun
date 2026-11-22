use crate::problem::{Fix, Safety};
use mago_syntax::cst::{Argument, Node, NodeKind};

use crate::rules::support::arguments::append_argument;
use crate::rules::support::expressions::{args, named};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct RequireStrictArraySearch;

impl Rule for RequireStrictArraySearch {
    fn id(&self) -> &'static str {
        "lint/functions/require-strict-array-search"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(c) = node else { return };
        let call = c;
        let args = args(call);
        if !named(call, "array_search")
            || args.len() != 2
            || args.iter().any(|a| matches!(a, Argument::Named(_)))
        {
            return;
        }
        out.push(Report {
            span: node_span(call),
            message: "Pass `true` to `array_search()` for a strict comparison.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Unsafe,
                edits: vec![append_argument(ctx.source, &call.argument_list, "true")],
            }),
        });
    }
}
