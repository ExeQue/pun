use mago_syntax::cst::{Node, NodeKind};
use pun_problem::{Edit, Fix, Safety};

use super::rector_code_quality::{args, call_to, named, text};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoUselessArrayValues;

impl Rule for NoUselessArrayValues {
    fn id(&self) -> &'static str {
        "lint/complexity/no-useless-array-values"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(c) = node else { return };
        if !named(c, "in_array") {
            return;
        }
        let Some(haystack) = args(c).get(1).copied() else {
            return;
        };
        let Some(values) = call_to(haystack.value(), "array_values") else {
            return;
        };
        let Some(inner) = args(values).first().copied() else {
            return;
        };
        if haystack.is_unpacked() || inner.is_unpacked() {
            return;
        }
        let span = node_span(unparenthesize(haystack.value()));
        out.push(Report {
            span,
            message: "`in_array()` needs no `array_values()`.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, text(ctx, unparenthesize(inner.value())))],
            }),
        });
    }
}
