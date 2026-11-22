use crate::problem::{Edit, Fix, Safety};
use mago_syntax::cst::{Node, NodeKind};

use crate::rules::support::expressions::is_constant;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoNegatedBooleanLiteral;

impl Rule for NoNegatedBooleanLiteral {
    fn id(&self) -> &'static str {
        "lint/complexity/no-negated-boolean-literal"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::UnaryPrefix])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::UnaryPrefix(not) = node else { return };
        if !not.operator.is_not() {
            return;
        }
        let value = if is_constant(not.operand, "false") {
            "true"
        } else if is_constant(not.operand, "true") {
            "false"
        } else {
            return;
        };
        let span = node_span(not);
        out.push(Report {
            span,
            message: format!("Use `{value}` instead of negating a boolean literal."),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, value)],
            }),
        });
    }
}
