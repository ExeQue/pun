use mago_syntax::cst::{
    Access, AssignmentOperator, BinaryOperator, Expression, Literal, Node, NodeKind,
};
use pun_problem::{Edit, Fix, Safety, Span};

use crate::rules::support::statements::{keeps_parentheses, operand_precedence, parent_of, tokens};
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoUselessMath;

impl Rule for NoUselessMath {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-useless-math"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Binary, NodeKind::Assignment])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let source = ctx.source;
        let parent = parent_of(ancestors);
        let text = match node {
            Node::Assignment(assignment) => {
                let facts = facts(ctx);
                let rhs = unparenthesize(assignment.rhs);
                let kept = match assignment.operator {
                    AssignmentOperator::Addition(_) | AssignmentOperator::Subtraction(_) => {
                        is_literal(rhs, 0)
                            && (matches!(assignment.operator, AssignmentOperator::Subtraction(_))
                                || is_integer(&facts, assignment.lhs))
                    }
                    AssignmentOperator::Multiplication(_) | AssignmentOperator::Division(_) => {
                        is_literal(rhs, 1)
                    }
                    _ => return,
                };
                if !kept || !is_number(&facts, assignment.lhs) {
                    return;
                }
                operand_text(source, assignment.lhs, parent)
            }
            _ => {
                let Node::Binary(binary) = node else { return };
                let (left, right) = (unparenthesize(binary.lhs), unparenthesize(binary.rhs));
                let plus = matches!(binary.operator, BinaryOperator::Addition(_));
                match binary.operator {
                    BinaryOperator::Addition(_) | BinaryOperator::Subtraction(_) => {
                        let facts = facts(ctx);
                        if !is_number(&facts, binary.lhs) || !is_number(&facts, binary.rhs) {
                            return;
                        }
                        if is_literal(left, 0) {
                            if !is_integer(&facts, binary.rhs) {
                                return;
                            }
                            if plus {
                                operand_text(source, binary.rhs, parent)
                            } else {
                                negated(source, right)
                            }
                        } else if is_literal(right, 0) && (!plus || is_integer(&facts, binary.lhs))
                        {
                            operand_text(source, binary.lhs, parent)
                        } else {
                            return;
                        }
                    }
                    BinaryOperator::Multiplication(_) | BinaryOperator::Division(_) => {
                        let class_constant = |e: &Expression<'_>| {
                            matches!(e, Expression::Access(Access::ClassConstant(_)))
                        };
                        if class_constant(left) || class_constant(right) {
                            return;
                        }
                        let facts = facts(ctx);
                        if !is_number(&facts, binary.lhs) || !is_number(&facts, binary.rhs) {
                            return;
                        }
                        let times = matches!(binary.operator, BinaryOperator::Multiplication(_));
                        if times && is_literal(left, 1) {
                            if is_parenthesized_operation(ctx, node_span(binary), binary.rhs) {
                                format!("({})", text_of(source, right))
                            } else {
                                operand_text(source, binary.rhs, parent)
                            }
                        } else if is_literal(right, 1) {
                            operand_text(source, binary.lhs, parent)
                        } else {
                            return;
                        }
                    }
                    _ => return,
                }
            }
        };
        let span = node_span(&node);
        out.push(Report {
            span,
            message: "This operation does not change the number; keep the number.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, text)],
            }),
        });
    }
}

fn is_literal(expr: &Expression<'_>, value: u64) -> bool {
    matches!(expr, Expression::Literal(Literal::Integer(i)) if i.value == Some(value))
}

fn is_integer(facts: &Facts, expr: &Expression<'_>) -> bool {
    facts
        .native_type(node_span(unparenthesize(expr)))
        .is_some_and(|t| !t.types.is_empty() && t.is_int())
}

fn is_number(facts: &Facts, expr: &Expression<'_>) -> bool {
    is_integer(facts, expr)
        || facts
            .native_type(node_span(unparenthesize(expr)))
            .is_some_and(mago_codex::ttype::union::TUnion::is_float)
}

fn text_of<'s>(source: &'s str, expr: &Expression<'_>) -> &'s str {
    let span = node_span(expr);
    &source[span.start..span.end]
}

fn operand_text(source: &str, expr: &Expression<'_>, parent: Option<Node<'_, '_>>) -> String {
    let inner = unparenthesize(expr);
    if keeps_parentheses(inner, parent) {
        format!("({})", text_of(source, inner))
    } else {
        text_of(source, inner).to_string()
    }
}

fn negated(source: &str, operand: &Expression<'_>) -> String {
    let text = text_of(source, operand);
    if operand_precedence(operand) < 20 || text.starts_with('-') {
        format!("-({text})")
    } else {
        format!("-{text}")
    }
}

fn is_parenthesized_operation(ctx: &crate::Context<'_>, mul: Span, right: &Expression<'_>) -> bool {
    let source = ctx.source;
    let inner = unparenthesize(right);
    if !matches!(inner, Expression::Binary(_)) || !source[..mul.end].ends_with(')') {
        return false;
    }
    let start = node_span(inner).start;
    tokens(ctx)
        .iter()
        .rev()
        .find(|t| t.end <= start && !source[t.start..t.end].trim().is_empty())
        .is_some_and(|t| &source[t.start..t.end] == "(")
}
