//! Rector 2.8.4's `StrEndsWithRector`.

use mago_syntax::cst::{Binary, Expression, Node, NodeKind, UnaryPrefixOperator};

use super::prefer_str_starts_with::{
    arg, call_and_other, is_length_of, is_strlen_of, positive, report,
};
use crate::rules::support::values::{Value, value as literal};
use crate::syntax::unparenthesize;
use crate::{Context, Report, Rule};

pub struct PreferEndsWithFunction;

impl Rule for PreferEndsWithFunction {
    fn id(&self) -> &'static str {
        "lint/php-8.0/prefer-str-ends-with"
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
        let Some((haystack, needle)) =
            refactor_substr(ctx, binary).or_else(|| refactor_substr_compare(ctx, binary))
        else {
            return;
        };
        report(
            ctx,
            ancestors,
            binary,
            "str_ends_with",
            haystack,
            needle,
            positive,
            out,
        );
    }
}

fn refactor_substr<'a>(
    ctx: &Context<'_>,
    binary: &'a Binary<'a>,
) -> Option<(&'a Expression<'a>, &'a Expression<'a>)> {
    let (call, compared) = call_and_other(binary, "substr")?;
    let offset = arg(call, 1)?;
    if !is_unary_minus_strlen(ctx, offset, compared) && !is_hard_coded_length(offset, compared) {
        return None;
    }
    Some((arg(call, 0)?, compared))
}

fn refactor_substr_compare<'a>(
    ctx: &Context<'_>,
    binary: &'a Binary<'a>,
) -> Option<(&'a Expression<'a>, &'a Expression<'a>)> {
    let (call, other) = call_and_other(binary, "substr_compare")?;
    if literal(other) != Some(Value::Int(0)) {
        return None;
    }
    let (haystack, needle, offset) = (arg(call, 0)?, arg(call, 1)?, arg(call, 2)?);
    if arg(call, 4).is_some_and(|e| literal(e) == Some(Value::Bool(true))) {
        return None;
    }
    if !is_unary_minus_strlen(ctx, offset, needle) && !is_hard_coded_length(offset, needle) {
        return None;
    }
    Some((haystack, needle))
}

fn negated<'a>(expr: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    match unparenthesize(expr) {
        Expression::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Negation(_)) => {
            Some(u.operand)
        }
        _ => None,
    }
}

fn is_unary_minus_strlen(
    ctx: &Context<'_>,
    offset: &Expression<'_>,
    needle: &Expression<'_>,
) -> bool {
    negated(offset).is_some_and(|e| is_strlen_of(ctx, e, needle))
}

fn is_hard_coded_length(offset: &Expression<'_>, needle: &Expression<'_>) -> bool {
    negated(offset).is_some_and(|e| is_length_of(e, needle))
}
