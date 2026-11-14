use mago_syntax::cst::{Node, NodeKind};

use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoShortTernary;

impl Rule for NoShortTernary {
    fn id(&self) -> &'static str {
        "lint/style/no-short-ternary"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Conditional])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Conditional(c) = node else { return };
        if c.then.is_some() {
            return;
        }
        out.push(Report {
            span: node_span(c),
            message: "Short ternary operator is not allowed. Use null coalesce operator if applicable or consider using long ternary.".to_string(),
            help: None,
            fix: None,
        });
    }
}
