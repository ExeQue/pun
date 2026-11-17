use mago_syntax::cst::*;
use pun_problem::{Edit, Fix, Safety};

use crate::rules::support::options::part_on;
use crate::rules::support::syntax_types::{is_bool, parameter_hint};
use crate::syntax::{node_span, same_expression, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct NoUselessTernary {
    /// `useless` is left out of the rule's `checks`.
    off: bool,
}

#[derive(PartialEq)]
enum Native {
    Bool,
    Array,
    Int,
    Other,
}

impl Rule for NoUselessTernary {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-useless-ternary"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !checks(options, "useless")?;
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
        let Node::Conditional(ternary) = node else {
            return;
        };
        let source = ctx.source;
        let condition = unparenthesize(ternary.condition);
        if matches!(condition, Expression::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Not(_)))
        {
            return;
        }
        let native = native(ancestors, condition);
        let is = |e: &Expression<'_>, value: bool| {
            matches!(
                (unparenthesize(e), value),
                (Expression::Literal(Literal::True(_)), true)
                    | (Expression::Literal(Literal::False(_)), false)
            )
        };
        let useless = if native == Native::Bool
            && ternary.then.is_some_and(|t| is(t, true))
            && is(ternary.r#else, false)
        {
            true
        } else if ternary
            .then
            .is_some_and(|t| !same_expression(source, t, ternary.condition))
        {
            false
        } else {
            match (native, unparenthesize(ternary.r#else)) {
                (Native::Bool, e) => is(e, false),
                (Native::Array, Expression::Array(a)) => a.elements.is_empty(),
                (Native::Array, Expression::LegacyArray(a)) => a.elements.is_empty(),
                (Native::Int, Expression::Literal(Literal::Integer(i))) => i.value == Some(0),
                _ => false,
            }
        };
        if !useless {
            return;
        }
        let span = node_span(condition);
        out.push(Report {
            span: node_span(ternary),
            message: "This ternary gives back its condition either way; use the condition."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(node_span(ternary), &source[span.start..span.end])],
            }),
        });
    }
}

fn native(ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> Native {
    if is_bool(ancestors, expr) {
        return Native::Bool;
    }
    let Expression::Variable(Variable::Direct(v)) = expr else {
        return Native::Other;
    };
    match parameter_hint(ancestors, v.name) {
        Some(Hint::Array(_)) => Native::Array,
        Some(Hint::Integer(_)) => Native::Int,
        _ => Native::Other,
    }
}

const CHECKS: [&str; 3] = ["bool", "tautology", "useless"];

/// Pun's `checks`: the ones that run, one for each of Rector's three rules (`useless` is `RemoveUselessTernaryRector`,
/// in its dead-code set; `tautology` and `bool` are in its code-quality set).
pub(super) fn checks(options: &serde_json::Value, check: &str) -> Result<bool, OptionsError> {
    part_on(options, "checks", &CHECKS, check)
}
