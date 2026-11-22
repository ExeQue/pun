use crate::problem::{Edit, Fix, Safety};
use mago_syntax::cst::*;

use crate::rules::support::expressions::{Prec, equal, in_parent};
use crate::rules::support::statement_edits::{
    compared_values, has_binary, is_in_constant_expression, is_safe_to_evaluate_eagerly,
};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferNotInArray;

impl Rule for PreferNotInArray {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-not-in-array"
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
        if !matches!(binary.operator, BinaryOperator::And(_)) {
            return;
        }
        let and = |op: &BinaryOperator<'_>| matches!(op, BinaryOperator::And(_));
        let compare = |op: &BinaryOperator<'_>| op.is_negated_equality();
        if is_in_constant_expression(ancestors) {
            return;
        }
        let Some(pairs) = compared_values(binary, &and, &compare) else {
            return;
        };
        if pairs.len() < 3 || !pairs.iter().all(|(_, v)| is_safe_to_evaluate_eagerly(v)) {
            return;
        }
        let (compared, _) = pairs[pairs.len() - 1];
        if !pairs.iter().all(|(c, _)| equal(ctx, c, compared)) {
            return;
        }
        let not_identicals = has_binary(node, &|op| matches!(op, BinaryOperator::NotIdentical(_)));
        let not_equals = has_binary(node, &|op| {
            matches!(
                op,
                BinaryOperator::NotEqual(_) | BinaryOperator::AngledNotEqual(_)
            )
        });
        let text = |e: &Expression<'_>| {
            let s = node_span(unparenthesize(e));
            &ctx.source[s.start..s.end]
        };
        let values: Vec<&str> = pairs.iter().map(|(_, v)| text(v)).collect();
        let strict = if not_identicals && !not_equals {
            ", true"
        } else {
            ""
        };
        let span = node_span(binary);
        let code = format!(
            "!in_array({}, [{}]{strict})",
            text(compared),
            values.join(", ")
        );
        out.push(Report {
            span,
            message: "Use `!in_array()` instead of repeated `&&` comparisons.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(
                    span,
                    in_parent(span, ancestors, code, Prec::Prefix(30)),
                )],
            }),
        });
    }
}
