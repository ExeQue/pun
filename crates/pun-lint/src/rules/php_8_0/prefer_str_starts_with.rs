//! Rector 2.8.4's `StrStartsWithRector`, with its `StrncmpMatchAndRefactor`, `SubstrMatchAndRefactor` and
//! `StrposMatchAndRefactor`.

use mago_syntax::cst::{Binary, BinaryOperator, Expression, FunctionCall, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{Prec, call_to, equal, in_parent, text};
use crate::rules::support::values::{Value, value as literal};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferStartsWithFunction;

impl Rule for PreferStartsWithFunction {
    fn id(&self) -> &'static str {
        "lint/php-8.0/prefer-str-starts-with"
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
        let Some((haystack, needle)) = strncmp(ctx, binary)
            .or_else(|| substr(ctx, binary))
            .or_else(|| strpos(binary))
        else {
            return;
        };
        report(
            ctx,
            ancestors,
            binary,
            "str_starts_with",
            haystack,
            needle,
            positive,
            out,
        );
    }
}

/// `true` for `===` and `==`, `false` for `!==` and `!=`.
pub(crate) fn positive(binary: &Binary<'_>) -> Option<bool> {
    match binary.operator {
        BinaryOperator::Identical(_) | BinaryOperator::Equal(_) => Some(true),
        BinaryOperator::NotIdentical(_)
        | BinaryOperator::NotEqual(_)
        | BinaryOperator::AngledNotEqual(_) => Some(false),
        _ => None,
    }
}

/// The call to `name` on either side, the left one first, and the other side.
pub(crate) fn call_and_other<'a>(
    binary: &'a Binary<'a>,
    name: &str,
) -> Option<(&'a FunctionCall<'a>, &'a Expression<'a>)> {
    if let Some(call) = call_to(binary.lhs, name) {
        return Some((call, binary.rhs));
    }
    call_to(binary.rhs, name).map(|call| (call, binary.lhs))
}

pub(crate) fn arg<'a>(call: &'a FunctionCall<'a>, index: usize) -> Option<&'a Expression<'a>> {
    call.argument_list.arguments.get(index).map(value)
}

pub(crate) fn is_strlen_of(ctx: &Context<'_>, expr: &Expression<'_>, of: &Expression<'_>) -> bool {
    call_to(expr, "strlen")
        .and_then(|c| arg(c, 0))
        .is_some_and(|e| equal(ctx, e, of))
}

/// `$lNumber->value === strlen($string->value)`.
pub(crate) fn is_length_of(length: &Expression<'_>, string: &Expression<'_>) -> bool {
    let (Expression::Literal(mago_syntax::cst::Literal::Integer(_)), Some(Value::Int(n))) =
        (crate::syntax::unparenthesize(length), literal(length))
    else {
        return false;
    };
    let Expression::Literal(mago_syntax::cst::Literal::String(_)) =
        crate::syntax::unparenthesize(string)
    else {
        return false;
    };
    matches!(literal(string), Some(Value::Str(s)) if usize::try_from(n) == Ok(s.len()))
}

fn args_len(call: &FunctionCall<'_>) -> usize {
    call.argument_list.arguments.len()
}

fn strncmp<'a>(
    ctx: &Context<'_>,
    binary: &'a Binary<'a>,
) -> Option<(&'a Expression<'a>, &'a Expression<'a>)> {
    // Rector does not read the compared value.
    let (call, _) = call_and_other(binary, "strncmp")?;
    if args_len(call) < 3 {
        return None;
    }
    let (haystack, needle, length) = (arg(call, 0)?, arg(call, 1)?, arg(call, 2)?);
    (is_strlen_of(ctx, length, needle) || is_length_of(length, needle))
        .then_some((haystack, needle))
}

fn substr<'a>(
    ctx: &Context<'_>,
    binary: &'a Binary<'a>,
) -> Option<(&'a Expression<'a>, &'a Expression<'a>)> {
    let (call, needle) = call_and_other(binary, "substr")?;
    if args_len(call) < 3 || literal(arg(call, 1)?) != Some(Value::Int(0)) {
        return None;
    }
    let length = arg(call, 2)?;
    (is_strlen_of(ctx, length, needle) || is_length_of(length, needle))
        .then_some((arg(call, 0)?, needle))
}

fn strpos<'a>(binary: &'a Binary<'a>) -> Option<(&'a Expression<'a>, &'a Expression<'a>)> {
    let (call, zero) = call_and_other(binary, "strpos")?;
    if literal(zero) != Some(Value::Int(0)) || args_len(call) < 2 {
        return None;
    }
    Some((arg(call, 0)?, arg(call, 1)?))
}

#[expect(clippy::too_many_arguments, reason = "the parts of the rewrite")]
pub(crate) fn report(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    binary: &Binary<'_>,
    function: &str,
    haystack: &Expression<'_>,
    needle: &Expression<'_>,
    positive: bool,
    out: &mut Vec<Report>,
) {
    let call = format!("{function}({}, {})", text(ctx, haystack), text(ctx, needle));
    let span = node_span(binary);
    let code = if positive {
        call
    } else {
        in_parent(span, ancestors, format!("!{call}"), Prec::Prefix(30))
    };
    out.push(Report::fixed(
        span,
        format!("Use `{function}()` instead of comparing a part of the string."),
        Safety::Unsafe,
        vec![Edit::new(span, code)],
    ));
}
