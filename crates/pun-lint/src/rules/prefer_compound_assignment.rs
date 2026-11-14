use mago_span::HasSpan;
use mago_syntax::cst::{AssignmentOperator, BinaryOperator, Expression, Node, NodeKind};
use pun_problem::{Edit, Fix, Safety, Span};

use crate::syntax::{node_span, same_code, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferCompoundAssignment;

impl Rule for PreferCompoundAssignment {
    fn id(&self) -> &'static str {
        "lint/style/prefer-compound-assignment"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Assignment])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Assignment(a) = node else { return };
        if !matches!(a.operator, AssignmentOperator::Assign(_)) {
            return;
        }
        let Expression::Binary(binary) = unparenthesize(a.rhs) else {
            return;
        };
        let Some(op) = compound(&binary.operator) else {
            return;
        };
        let left = unparenthesize(binary.lhs);
        if matches!(left, Expression::ArrayAccess(_))
            && matches!(binary.operator, BinaryOperator::BitwiseXor(_))
        {
            return;
        }
        let var = node_span(a.lhs);
        if !same_code(ctx.source, var, node_span(left)) {
            return;
        }
        let right = match unparenthesize(binary.rhs) {
            Expression::Binary(b)
                if matches!(
                    b.operator,
                    BinaryOperator::LowAnd(_)
                        | BinaryOperator::LowOr(_)
                        | BinaryOperator::LowXor(_)
                ) =>
            {
                node_span(binary.rhs)
            }
            inner => node_span(inner),
        };
        let end = a.span().end.offset as usize;
        let mut edits = vec![Edit::new(
            Span::new(var.end, right.start),
            format!(" {op}= "),
        )];
        if right.end < end {
            edits.push(Edit::new(Span::new(right.end, end), ""));
        }
        out.push(Report {
            span: node_span(a),
            message: format!("Use `{op}=` instead of repeating the assigned variable."),
            help: None,
            fix: Some(Fix {
                safety: Safety::Unsafe,
                edits,
            }),
        });
    }
}

fn compound(op: &BinaryOperator<'_>) -> Option<&'static str> {
    Some(match op {
        BinaryOperator::Addition(_) => "+",
        BinaryOperator::Subtraction(_) => "-",
        BinaryOperator::Multiplication(_) => "*",
        BinaryOperator::Division(_) => "/",
        BinaryOperator::Modulo(_) => "%",
        BinaryOperator::Exponentiation(_) => "**",
        BinaryOperator::BitwiseAnd(_) => "&",
        BinaryOperator::BitwiseOr(_) => "|",
        BinaryOperator::BitwiseXor(_) => "^",
        BinaryOperator::LeftShift(_) => "<<",
        BinaryOperator::RightShift(_) => ">>",
        BinaryOperator::StringConcat(_) => ".",
        _ => return None,
    })
}
