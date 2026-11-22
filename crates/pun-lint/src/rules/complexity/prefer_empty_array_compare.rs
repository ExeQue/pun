use crate::problem::{Edit, Fix, Safety};
use mago_syntax::cst::{BinaryOperator, Construct, Expression, Node, NodeKind};

use crate::rules::support::expressions::{Prec, args, call_to, equal, in_parent, text};
use crate::rules::support::options::part_on;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct PreferEmptyArrayCompare {
    /// `is-array` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for PreferEmptyArrayCompare {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-empty-array-compare"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !checks(options, "is-array")?;
        Ok(())
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
        if self.off {
            return;
        }
        let Node::Binary(and) = node else { return };
        if !matches!(and.operator, BinaryOperator::And(_)) {
            return;
        }
        let Some((array, empty)) = is_array_arg(and.lhs)
            .zip(empty_arg(and.rhs))
            .or_else(|| is_array_arg(and.rhs).zip(empty_arg(and.lhs)))
        else {
            return;
        };
        if !equal(ctx, empty, array) {
            return;
        }
        let span = node_span(and);
        let identical = Prec::Infix(80, 80, 80);
        let code = format!("{} === []", text(ctx, unparenthesize(empty)));
        out.push(Report {
            span,
            message: "Compare with `[]` instead of checking `is_array()` and `empty()`."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, in_parent(span, ancestors, code, identical))],
            }),
        });
    }
}

fn is_array_arg<'a>(expr: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    let value = args(call_to(expr, "is_array")?).first()?.value();
    matches!(unparenthesize(value), Expression::Variable(_)).then_some(value)
}

fn empty_arg<'a>(expr: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    let Expression::Construct(Construct::Empty(empty)) = unparenthesize(expr) else {
        return None;
    };
    matches!(unparenthesize(empty.value), Expression::Variable(_)).then_some(empty.value)
}

const CHECKS: [&str; 2] = ["empty", "is-array"];

/// Pun's `checks`: the ones that run, one for each of Rector's 2 rules (`empty` is
/// `SimplifyEmptyCheckOnEmptyArrayRector`, `is-array` is `SimplifyEmptyArrayCheckRector`).
pub(super) fn checks(options: &serde_json::Value, check: &str) -> Result<bool, OptionsError> {
    part_on(options, "checks", &CHECKS, check)
}
