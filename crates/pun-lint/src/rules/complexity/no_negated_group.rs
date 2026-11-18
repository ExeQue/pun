use mago_syntax::cst::{BinaryOperator, Expression, Node, NodeKind};
use pun_problem::{Edit, Fix, Safety};

use crate::rules::support::expressions::{Prec, de_morgan, in_parent};
use crate::rules::support::options::part_on;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct NoNegatedGroup {
    /// `de-morgan` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for NoNegatedGroup {
    fn id(&self) -> &'static str {
        "lint/complexity/no-negated-group"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !checks(options, "de-morgan")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::UnaryPrefix])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if self.off {
            return;
        }
        let Node::UnaryPrefix(not) = node else { return };
        if !not.operator.is_not() {
            return;
        }
        let Expression::Binary(or) = unparenthesize(not.operand) else {
            return;
        };
        let is_or = |e: &Expression<'_>| matches!(unparenthesize(e), Expression::Binary(b) if matches!(b.operator, BinaryOperator::Or(_)));
        if !matches!(or.operator, BinaryOperator::Or(_)) || is_or(or.lhs) || is_or(or.rhs) {
            return;
        }
        let span = node_span(not);
        let and = Prec::Infix(120, 121, 120);
        let code = in_parent(
            span,
            ancestors,
            de_morgan(ctx, or.lhs, or.rhs, "&&", (121, 120)),
            and,
        );
        out.push(Report {
            span,
            message: "Negate each condition and join them with `&&` instead of negating the `||`."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, code)],
            }),
        });
    }
}

const CHECKS: [&str; 2] = ["ands", "de-morgan"];

/// Pun's `checks`: the ones that run, one for each of Rector's 2 rules (`ands` is `NegatedAndsToPositiveOrsRector`,
/// `de-morgan` is `SimplifyDeMorganBinaryRector`).
pub(super) fn checks(options: &serde_json::Value, check: &str) -> Result<bool, OptionsError> {
    part_on(options, "checks", &CHECKS, check)
}
