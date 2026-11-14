use mago_codex::ttype::atomic::TAtomic;
use mago_syntax::cst::{Expression, Node, NodeKind};
use pun_problem::{Edit, Safety};

use super::rector_code_quality::text;
use super::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferExplicitArrayComparison;

impl Rule for PreferExplicitArrayComparison {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-explicit-array-comparison"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::If,
            NodeKind::IfStatementBodyElseIfClause,
            NodeKind::IfColonDelimitedBodyElseIfClause,
            NodeKind::Conditional,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let condition = match node {
            Node::If(i) => i.condition,
            Node::IfStatementBodyElseIfClause(c) => c.condition,
            Node::IfColonDelimitedBodyElseIfClause(c) => c.condition,
            Node::Conditional(t) if t.then.is_some() => t.condition,
            _ => return,
        };
        let condition = unparenthesize(condition);
        let (variable, operator) = match condition {
            Expression::UnaryPrefix(not) if not.operator.is_not() => {
                (unparenthesize(not.operand), "===")
            }
            other => (other, "!=="),
        };
        if !matches!(variable, Expression::Variable(_)) {
            return;
        }
        if !facts(ctx)
            .native_type(node_span(variable))
            .is_some_and(|t| {
                !t.types.is_empty() && t.types.iter().all(|a| matches!(a, TAtomic::Array(_)))
            })
        {
            return;
        }
        let span = node_span(condition);
        out.push(Report::fixed(
            span,
            "Compare the array with `[]` instead of using it as a condition.",
            Safety::Safe,
            vec![Edit::new(
                span,
                format!("{} {operator} []", text(ctx, variable)),
            )],
        ));
    }
}
