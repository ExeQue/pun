//! Rector 2.8.4's `HebrevcToNl2brHebrevRector`.

use mago_syntax::cst::{Node, NodeKind};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::expressions::named;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoHebrevc;

impl Rule for NoHebrevc {
    fn id(&self) -> &'static str {
        "lint/php-7.4/no-hebrevc"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        if !named(call, "hebrevc") {
            return;
        }
        let end = node_span(&node).end;
        out.push(Report::fixed(
            node_span(&node),
            "Replace `hebrevc()`, which PHP 8.0 removes, with `nl2br(hebrev())`.",
            Safety::Safe,
            vec![
                Edit::new(node_span(call.function), "nl2br(hebrev"),
                Edit::new(Span::new(end, end), ")"),
            ],
        ));
    }
}
