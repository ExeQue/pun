use mago_syntax::cst::{Node, NodeKind, Variable};

use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoDynamicName;

impl Rule for NoDynamicName {
    fn id(&self) -> &'static str {
        "lint/style/no-variable-variable"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Variable])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Variable(Variable::Nested(nested)) = node else {
            return;
        };
        out.push(Report {
            span: node_span(nested),
            message: "Usage of variable variables (e.g., `$$var`) is discouraged.".to_string(),
            help: Some("Refactor the code to avoid using variable variables.".to_string()),
            fix: None,
        });
    }
}
