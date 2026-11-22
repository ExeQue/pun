use crate::problem::{Edit, Fix, Safety};
use mago_syntax::cst::{Expression, Node, NodeKind};

use crate::rules::support::expressions::{args, call_to, equal, is_constant, text};
use crate::rules::support::options::part_on;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct PreferNullCoalesce {
    /// `array-key-exists` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for PreferNullCoalesce {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-null-coalesce"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !checks(options, "array-key-exists")?;
        Ok(())
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
        if self.off {
            return;
        }
        let Node::Conditional(t) = node else { return };
        let Some(call) = call_to(t.condition, "array_key_exists") else {
            return;
        };
        let Some(Expression::ArrayAccess(fetch)) = t.then.map(unparenthesize) else {
            return;
        };
        let args = args(call);
        if args.len() < 2
            || !equal(ctx, fetch.array, args[1].value())
            || !equal(ctx, fetch.index, args[0].value())
        {
            return;
        }
        if !is_constant(t.r#else, "null") {
            return;
        }
        let code = format!(
            "{} ?? {}",
            text(ctx, fetch),
            text(ctx, unparenthesize(t.r#else))
        );
        out.push(Report {
            span: node_span(t),
            message: "Use `??` instead of an `array_key_exists()` ternary.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(node_span(t), code)],
            }),
        });
    }
}

const CHECKS: [&str; 3] = ["array-key-exists", "consecutive-returns", "foreach"];

/// Pun's `checks`: the ones that run, one for each of Rector's 3 rules (`array-key-exists` is
/// `ArrayKeyExistsTernaryThenValueToCoalescingRector`, `consecutive-returns` is
/// `ConsecutiveNullCompareReturnsToNullCoalesceQueueRector`, `foreach` is `SimplifyForeachToCoalescingRector`).
pub(super) fn checks(options: &serde_json::Value, check: &str) -> Result<bool, OptionsError> {
    part_on(options, "checks", &CHECKS, check)
}
