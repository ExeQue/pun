//! Rector 2.8.4's `MbStrContainsRector`, with `StrFalseComparisonResolver`.

use mago_syntax::cst::{Binary, Expression, FunctionCall, Node, NodeKind};
use pun_phpstan::types::Type;

use super::prefer_str_starts_with::positive;
use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{Prec, call_to, in_parent, is_constant, named, text};
use crate::rules::support::values::{Value, value as literal};
use crate::rules::types::compare::mapper::expr_type;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const OLD_STR_NAMES: [&str; 2] = ["mb_strpos", "mb_strstr"];

pub struct PreferStrContainsOverMbStrpos;

impl Rule for PreferStrContainsOverMbStrpos {
    fn id(&self) -> &'static str {
        "lint/php-8.0/prefer-str-contains-over-mb-strpos"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80000)
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
        let Node::Binary(binary) = node else {
            return;
        };
        let Some(positive) = positive(binary) else {
            return;
        };
        let Some(call) = resolve(binary) else {
            return;
        };
        let args: Vec<_> = call.argument_list.arguments.iter().collect();
        if let Some(offset) = args.get(2) {
            // str_contains() has no offset, and mb_substr() would copy the haystack on every call
            if named(call, "mb_strpos") && !is_zero(ctx, ancestors, value(offset)) {
                return;
            }
        }
        let kept: Vec<&str> = (args.iter().enumerate())
            .filter(|(i, _)| *i != 2)
            .map(|(_, a)| text(ctx, *a))
            .collect();
        let call = format!("str_contains({})", kept.join(", "));
        let span = node_span(binary);
        // `=== false` and `== false` ask whether it is missing.
        let code = if positive {
            in_parent(span, ancestors, format!("!{call}"), Prec::Prefix(30))
        } else {
            call
        };
        out.push(Report::fixed(
            span,
            "Use `str_contains()` instead of comparing `mb_strpos()` or `mb_strstr()` with `false`.",
            Safety::Unsafe,
            vec![Edit::new(span, code)],
        ));
    }
}

/// `StrFalseComparisonResolver::resolve`.
fn resolve<'a>(binary: &'a Binary<'a>) -> Option<&'a FunctionCall<'a>> {
    let is_old = |e: &'a Expression<'a>| OLD_STR_NAMES.iter().find_map(|name| call_to(e, name));
    if is_constant(binary.lhs, "false") {
        return is_old(binary.rhs);
    }
    if is_constant(binary.rhs, "false") {
        return is_old(binary.lhs);
    }
    None
}

/// `ValueResolver::isValue($expr, 0)`: the literal `0`, or an expression whose type is the constant `0`.
fn is_zero(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    literal(expr) == Some(Value::Int(0))
        || matches!(
            expr_type(&facts(ctx), ancestors, expr, false),
            Type::ConstInt(0)
        )
}
