use mago_syntax::cst::{BinaryOperator, Expression, Node, NodeKind};
use pun_problem::{Edit, Fix, Safety};

use crate::rules::support::expressions::{equal, moved};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct NoTautologyTernary {
    /// `tautology` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for NoTautologyTernary {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-useless-ternary"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !super::no_useless_ternary::checks(options, "tautology")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Conditional])
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
        let Node::Conditional(t) = node else { return };
        let Expression::Binary(compare) = unparenthesize(t.condition) else {
            return;
        };
        let not_identical = match compare.operator {
            BinaryOperator::NotIdentical(_) => true,
            BinaryOperator::Identical(_) => false,
            _ => return,
        };
        let Some(then) = t.then else { return };
        let (l, r, e) = (compare.lhs, compare.rhs, t.r#else);
        let matched =
            (equal(ctx, l, then) && equal(ctx, r, e)) || (equal(ctx, r, then) && equal(ctx, l, e));
        if !matched {
            return;
        }
        let value = if not_identical { then } else { e };
        let span = node_span(t);
        let code = moved(ctx, span, ancestors, value);
        out.push(Report {
            span,
            message:
                "The ternary compares the two values it chooses between; use the value it always gives."
                    .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, code)],
            }),
        });
    }
}
