use mago_syntax::cst::*;
use pun_problem::{Edit, Fix, Safety};

use crate::rules::support::expressions::equal;
use crate::rules::support::statement_edits::{
    compared_values, has_binary, is_in_constant_expression, is_safe_to_evaluate_eagerly,
};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferInArray;

impl Rule for PreferInArray {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-in-array"
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
        if !matches!(binary.operator, BinaryOperator::Or(_)) {
            return;
        }
        let or = |op: &BinaryOperator<'_>| matches!(op, BinaryOperator::Or(_));
        let compare = |op: &BinaryOperator<'_>| {
            matches!(op, BinaryOperator::Equal(_) | BinaryOperator::Identical(_))
        };
        if is_in_constant_expression(ancestors) {
            return;
        }
        let Some(pairs) = compared_values(binary, &or, &compare) else {
            return;
        };
        if pairs.len() < 3 || !pairs.iter().all(|(_, v)| is_safe_to_evaluate_eagerly(v)) {
            return;
        }
        let (compared, _) = pairs[pairs.len() - 1];
        if !pairs.iter().all(|(c, _)| equal(ctx, c, compared)) {
            return;
        }
        let identicals = has_binary(node, &|op| matches!(op, BinaryOperator::Identical(_)));
        let equals = has_binary(node, &|op| matches!(op, BinaryOperator::Equal(_)));
        if identicals && equals {
            return;
        }
        let text = |e: &Expression<'_>| {
            let s = node_span(unparenthesize(e));
            &ctx.source[s.start..s.end]
        };
        let values: Vec<&str> = pairs.iter().map(|(_, v)| text(v)).collect();
        let strict = if identicals { ", true" } else { "" };
        let code = format!(
            "in_array({}, [{}]{strict})",
            text(compared),
            values.join(", ")
        );
        out.push(Report {
            span: node_span(binary),
            message: "Use `in_array()` instead of repeated `||` comparisons.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(node_span(binary), code)],
            }),
        });
    }
}
