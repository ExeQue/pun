use crate::problem::{Edit, Fix, Safety, Span};
use mago_syntax::cst::{AssignmentOperator, Expression, Node, NodeKind};

use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoConcatAssignNewItem;

impl Rule for NoConcatAssignNewItem {
    fn id(&self) -> &'static str {
        "lint/complexity/no-concat-assign-new-item"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Assignment])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Assignment(a) = node else { return };
        if !matches!(a.operator, AssignmentOperator::Concat(_))
            || !matches!(a.lhs, Expression::ArrayAppend(_))
        {
            return;
        }
        let edit = Edit::new(
            Span::new(node_span(a.lhs).end, node_span(a.rhs).start),
            " = ",
        );
        out.push(Report {
            span: node_span(a),
            message: "Use `=` instead of `.=` on a new array item.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![edit],
            }),
        });
    }
}
