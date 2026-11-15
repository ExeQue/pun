use mago_syntax::cst::{Binary, BinaryOperator, Expression, Node, NodeKind};
use pun_problem::{Edit, Fix, Safety};

use crate::rules::support::expressions::is_constant;
use crate::rules::support::statements::{bare_text, parent_of};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoAndTrue;

impl Rule for NoAndTrue {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-and-true"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Binary])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Binary(binary) = node else { return };
        if !matches!(binary.operator, BinaryOperator::And(_)) {
            return;
        }
        let kept = if is_true_or_and_of_trues(binary.lhs) {
            binary.rhs
        } else if is_true_or_and_of_trues(binary.rhs) {
            binary.lhs
        } else {
            return;
        };
        let span = node_span(binary);
        out.push(Report {
            span,
            message: "`&& true` adds nothing; remove it.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(
                    span,
                    bare_text(ctx.source, kept, parent_of(ancestors)),
                )],
            }),
        });
    }
}

fn is_true_or_and_of_trues(expr: &Expression<'_>) -> bool {
    if is_constant(expr, "true") {
        return true;
    }
    let Expression::Binary(Binary {
        lhs,
        operator: BinaryOperator::And(_),
        rhs,
    }) = unparenthesize(expr)
    else {
        return false;
    };
    is_true_or_and_of_trues(lhs) && is_true_or_and_of_trues(rhs)
}
