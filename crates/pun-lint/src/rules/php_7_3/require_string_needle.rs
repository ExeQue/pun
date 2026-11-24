//! Rector 2.8.4's `StringifyStrNeedlesRector`.

use mago_syntax::cst::{CompositeString, Expression, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::{unpacked, value};
use crate::rules::support::expressions::{named, operand};
use crate::rules::types::compare::mapper::expr_type;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

/// `NEEDLE_STRING_SENSITIVE_FUNCTIONS`.
const NEEDLE_STRING_SENSITIVE_FUNCTIONS: &[&str] = &[
    "strpos", "strrpos", "stripos", "strstr", "stripos", "strripos", "strstr", "strchr", "strrchr",
    "stristr",
];

pub struct RequireStringNeedle;

impl Rule for RequireStringNeedle {
    fn id(&self) -> &'static str {
        "lint/php-7.3/require-string-needle"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70300)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        if !NEEDLE_STRING_SENSITIVE_FUNCTIONS
            .iter()
            .any(|name| named(call, name))
        {
            return;
        }
        let Some(arg) = call.argument_list.arguments.get(1) else {
            return;
        };
        if unpacked(arg) {
            return;
        }
        // is argument string?
        let needle = value(arg);
        if matches!(
            needle,
            Expression::CompositeString(CompositeString::Interpolated(_))
        ) {
            return;
        }
        if expr_type(&facts(ctx), ancestors, needle, false)
            .is_string()
            .yes()
        {
            return;
        }
        out.push(Report::fixed(
            node_span(needle),
            "A needle that is not a string is read as a string since PHP 8.0; cast it with `(string)`.",
            Safety::Unsafe,
            vec![Edit::new(
                node_span(needle),
                format!("(string) {}", operand(ctx, needle, 1, Some(100))),
            )],
        ));
    }
}
