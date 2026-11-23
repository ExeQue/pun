use mago_syntax::cst::{BinaryOperator, Call, Expression, Literal, Node, NodeKind};

use super::object_type::is_object_type;
use super::{is_name, member_name, replace};
use crate::rules::support::expressions::text;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

#[derive(Clone, Copy)]
enum Op {
    Equal,
    Greater,
    GreaterOrEqual,
    Identical,
    NotEqual,
    NotIdentical,
    Smaller,
    SmallerOrEqual,
}

impl Op {
    fn of(operator: &BinaryOperator<'_>) -> Option<Op> {
        Some(match operator {
            BinaryOperator::Equal(_) => Op::Equal,
            BinaryOperator::GreaterThan(_) => Op::Greater,
            BinaryOperator::GreaterThanOrEqual(_) => Op::GreaterOrEqual,
            BinaryOperator::Identical(_) => Op::Identical,
            BinaryOperator::NotEqual(_) | BinaryOperator::AngledNotEqual(_) => Op::NotEqual,
            BinaryOperator::NotIdentical(_) => Op::NotIdentical,
            BinaryOperator::LessThan(_) => Op::Smaller,
            BinaryOperator::LessThanOrEqual(_) => Op::SmallerOrEqual,
            _ => return None,
        })
    }

    fn mirrored(self) -> Op {
        match self {
            Op::Greater => Op::Smaller,
            Op::GreaterOrEqual => Op::SmallerOrEqual,
            Op::Smaller => Op::Greater,
            Op::SmallerOrEqual => Op::GreaterOrEqual,
            other => other,
        }
    }

    fn method(self, number: u64) -> Option<&'static str> {
        Some(match (self, number) {
            (Op::Equal | Op::Identical | Op::SmallerOrEqual, 0) | (Op::Smaller, 1) => "isEmpty",
            (Op::Greater | Op::NotEqual | Op::NotIdentical, 0) | (Op::GreaterOrEqual, 1) => {
                "isNotEmpty"
            }
            _ => return None,
        })
    }
}

pub struct PreferCollectionIsEmpty;

impl Rule for PreferCollectionIsEmpty {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-collection-is-empty"
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
        let Some(op) = Op::of(&binary.operator) else {
            return;
        };
        let int = |e: &Expression<'_>| match unparenthesize(e) {
            Expression::Literal(Literal::Integer(i)) => Some(i.value),
            _ => None,
        };
        let (op, count_call, number) = if let Some(n) = int(binary.rhs) {
            (op, binary.lhs, n)
        } else if let Some(n) = int(binary.lhs) {
            (op.mirrored(), binary.rhs, n)
        } else {
            return;
        };
        let Some(method) = number.and_then(|n| op.method(n)) else {
            return;
        };
        let Expression::Call(Call::Method(call)) = unparenthesize(count_call) else {
            return;
        };
        if !member_name(&call.method).is_some_and(|n| is_name(n, "count"))
            || !call.argument_list.arguments.is_empty()
            || !is_object_type(
                ctx,
                ancestors,
                call.object,
                "Illuminate\\Support\\Enumerable",
            )
        {
            return;
        }
        out.push(replace(
            node_span(binary),
            format!("Use `{method}()` instead of comparing `count()`."),
            format!("{}->{method}()", text(ctx, call.object)),
        ));
    }
}
