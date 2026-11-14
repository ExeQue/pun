use mago_syntax::cst::{BinaryOperator, Expression, Node, NodeKind};
use pun_problem::{Edit, Safety};

use super::rector_code_quality::{prec, text, wrap};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoNegatedInstanceofTernary;

impl Rule for NoNegatedInstanceofTernary {
    fn id(&self) -> &'static str {
        "lint/complexity/no-negated-instanceof-ternary"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Conditional])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Conditional(t) = node else { return };
        let Some(then) = t.then else {
            return;
        };
        let Expression::UnaryPrefix(not) = unparenthesize(t.condition) else {
            return;
        };
        if !not.operator.is_not() {
            return;
        }
        let instance_of = unparenthesize(not.operand);
        if !matches!(instance_of, Expression::Binary(b) if matches!(b.operator, BinaryOperator::Instanceof(_)))
        {
            return;
        }
        let (then_inner, else_inner) = (unparenthesize(then), unparenthesize(t.r#else));
        let new_else = if matches!(t.r#else, Expression::Parenthesized(_)) {
            text(ctx, then_inner).to_string()
        } else {
            wrap(text(ctx, then_inner), prec(then_inner), 150, None)
        };
        out.push(Report::fixed(
            node_span(t),
            "Drop the `!` in front of `instanceof` and swap the branches of the t.",
            Safety::Safe,
            vec![
                Edit::new(node_span(not), text(ctx, instance_of)),
                Edit::new(node_span(then_inner), text(ctx, else_inner)),
                Edit::new(node_span(else_inner), new_else),
            ],
        ));
    }
}
