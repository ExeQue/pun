//! Rector 2.8.4's datetime-to-carbon rules: `DateTimeInstanceToCarbonRector` and `DateTimeMethodCallToCarbonRector`
//! (one rule: the second rewrites the `new` that the first rewrites everywhere), with `CarbonCallFactory`,
//! `TimeFuncCallToCarbonRector` and `DateFuncCallToCarbonRector`.

use std::fmt::Write as _;
use std::sync::LazyLock;

use crate::problem::{Edit, Safety};
use mago_syntax::cst::{
    Argument, BinaryOperator, Call, Expression, FunctionCall, Literal, Node, NodeKind,
};
use pcre2::bytes::Regex;

use crate::rules::refactor::configured_codemods::single_quoted;
use crate::rules::refactor::{called, function_name};
use crate::rules::support::expressions::text;
use crate::rules::support::names::resolve_class_name;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

static PLUS_MINUS_COUNT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?<operator>\+|-)(\s+)?(?<count>\d+)(\s+)?(?<unit>seconds|second|sec|minutes|minute|min|hours|hour|days|day|weeks|week|months|month|years|year)",
    )
    .unwrap()
});
static STATIC_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"now|yesterday|today|tomorrow").unwrap());

/// Nette's `Strings::trim()` characters.
const TRIM: &[char] = &[' ', '\t', '\n', '\r', '\0', '\x0B', '\u{A0}'];

/// `CarbonCallFactory::createFromDateTimeString()`, printed.
fn carbon_call(class: &str, value: &str) -> String {
    let bytes = value.as_bytes();
    let start = match STATIC_DATE.find(bytes).unwrap() {
        Some(m) => &value[m.start()..m.end()],
        None => "now",
    };
    let mut rest = Vec::new();
    let mut at = 0;
    for m in STATIC_DATE.find_iter(bytes) {
        let m = m.unwrap();
        rest.extend_from_slice(&bytes[at..m.start()]);
        at = m.end();
    }
    rest.extend_from_slice(&bytes[at..]);
    let mut chain = String::new();
    while let Some(caps) = PLUS_MINUS_COUNT.captures(&rest).unwrap() {
        let text = |name: &str| String::from_utf8_lossy(&caps[name]).into_owned();
        let count = text("count").parse::<i64>().unwrap_or(i64::MAX);
        let unit = match text("unit").as_str() {
            "sec" | "second" | "seconds" => "Seconds",
            "min" | "minute" | "minutes" => "Minutes",
            "hour" | "hours" => "Hours",
            "day" | "days" => "Days",
            "week" | "weeks" => "Weeks",
            "month" | "months" => "Months",
            _ => "Years",
        };
        let operator = if &caps["operator"] == b"+" {
            "add"
        } else {
            "sub"
        };
        let _ = write!(chain, "->{operator}{unit}({count})");
        let whole = caps.get(0).unwrap();
        rest.drain(whole.start()..whole.end());
    }
    let rest = String::from_utf8_lossy(&rest);
    let rest = rest.trim_matches(TRIM);
    let root = if rest.is_empty() {
        format!("{start}()")
    } else if start == "now" {
        format!("parse({})", single_quoted(rest))
    } else {
        format!("parse({})", single_quoted(&format!("{rest} {start}")))
    };
    format!("\\{class}::{root}{chain}")
}

/// `DateTimeInstanceToCarbonRector`, and `DateTimeMethodCallToCarbonRector` (the same rewrite of a `new` that is
/// the object of a method call).
pub struct PreferCarbon;

impl Rule for PreferCarbon {
    fn id(&self) -> &'static str {
        "lint/style/prefer-carbon"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Instantiation])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Instantiation(new) = node else {
            return;
        };
        let Expression::Identifier(class) = new.class else {
            return;
        };
        let name = resolve_class_name(ancestors, class);
        let carbon = match name.trim_start_matches('\\') {
            n if n.eq_ignore_ascii_case("DateTime") => "Carbon\\Carbon",
            n if n.eq_ignore_ascii_case("DateTimeImmutable") => "Carbon\\CarbonImmutable",
            _ => return,
        };
        let args: Vec<_> = new
            .argument_list
            .iter()
            .flat_map(|l| l.arguments.iter())
            .collect();
        let code = match args.as_slice() {
            [] => format!("\\{carbon}::now()"),
            [arg] => {
                let Expression::Literal(Literal::String(string)) = arg.value() else {
                    return;
                };
                let Some(value) = string.value.and_then(|v| std::str::from_utf8(v).ok()) else {
                    return;
                };
                carbon_call(carbon, value)
            }
            _ => return,
        };
        out.push(Report::fixed(
            node_span(new),
            format!("Use `{carbon}` instead of `new {name}`."),
            Safety::Unsafe,
            vec![Edit::new(node_span(new), code)],
        ));
    }
}

/// `TimeFuncCallToCarbonRector`.
pub struct PreferCarbonTimestamp;

impl Rule for PreferCarbonTimestamp {
    fn id(&self) -> &'static str {
        "lint/style/prefer-carbon-timestamp"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall, NodeKind::FunctionPartialApplication])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(Expression::Identifier(name)) = called(node) else {
            return;
        };
        if !function_name(ctx, ancestors, name).eq_ignore_ascii_case("time") {
            return;
        }
        let call = "\\Carbon\\Carbon::now()->getTimestamp()";
        let code = match node {
            Node::FunctionCall(c) if c.argument_list.arguments.is_empty() => call.to_string(),
            // php-parser's printer wraps the arrow function that is the value of an assignment
            Node::FunctionPartialApplication(_) => {
                let span = node_span(&node);
                let assigned = ancestors
                    .iter()
                    .rev()
                    .find(|a| node_span(*a) != span && !matches!(a, Node::Expression(_)));
                match assigned {
                    Some(Node::Assignment(_)) => format!("(static fn() => {call})"),
                    _ => format!("static fn() => {call}"),
                }
            }
            _ => return,
        };
        out.push(Report::fixed(
            node_span(&node),
            "Use `Carbon::now()->getTimestamp()` instead of `time()`.",
            Safety::Unsafe,
            vec![Edit::new(node_span(&node), code)],
        ));
    }
}

/// `DateFuncCallToCarbonRector`'s `TIME_UNITS`.
const TIME_UNITS: [(&str, u64); 5] = [
    ("Weeks", 604_800),
    ("Days", 86_400),
    ("Hours", 3_600),
    ("Minutes", 60),
    ("Seconds", 1),
];

/// `DateFuncCallToCarbonRector`.
///
/// Where Rector's rewrite drops part of the code, Pun leaves it alone: an unpacked argument (Rector writes the array as
/// the one argument), a named argument out of its parameter's place (Rector reads arguments by position), an operand of
/// `time() - ...` that is not an integer, and the base timestamp of `date($format, strtotime($date, $base))` (Rector
/// drops it; Pun takes the timestamp path).
pub struct PreferCarbonDate;

impl Rule for PreferCarbonDate {
    fn id(&self) -> &'static str {
        "lint/style/prefer-carbon-date"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Binary, NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let code = match node {
            Node::Binary(binary) => {
                if !matches!(binary.operator, BinaryOperator::Subtraction(_)) {
                    return;
                }
                let Expression::Call(Call::Function(left)) = unparenthesize(binary.lhs) else {
                    return;
                };
                if !is_function(ctx, ancestors, left, "time") {
                    return;
                }
                let Some(product) = product(binary.rhs) else {
                    return;
                };
                let Some((unit, seconds)) = TIME_UNITS.iter().find(|(_, s)| product % s == 0)
                else {
                    return;
                };
                format!(
                    "\\Carbon\\Carbon::now()->sub{unit}({})->getTimestamp()",
                    product / seconds
                )
            }
            Node::FunctionCall(call) => {
                let args: Vec<&Argument<'_>> = call.argument_list.arguments.iter().collect();
                if is_function(ctx, ancestors, call, "date") {
                    if !readable(&args, &["format", "timestamp"]) {
                        return;
                    }
                    match args.as_slice() {
                        [format, timestamp, ..] => {
                            let format = text(ctx, format.value());
                            match strtotime_date(ctx, ancestors, timestamp.value()) {
                                Some(date) => {
                                    format!("\\Carbon\\Carbon::parse({date})->format({format})")
                                }
                                None => format!(
                                    "\\Carbon\\Carbon::createFromTimestamp({})->format({format})",
                                    text(ctx, timestamp.value())
                                ),
                            }
                        }
                        [format] => {
                            if !matches!(format.value(), Expression::Literal(Literal::String(_))) {
                                return;
                            }
                            format!(
                                "\\Carbon\\Carbon::now()->format({})",
                                text(ctx, format.value())
                            )
                        }
                        [] => return,
                    }
                } else if is_function(ctx, ancestors, call, "strtotime") {
                    if !readable(&args, &["datetime", "baseTimestamp"]) {
                        return;
                    }
                    let [date] = args.as_slice() else {
                        return;
                    };
                    format!(
                        "\\Carbon\\Carbon::parse({})->getTimestamp()",
                        text(ctx, date.value())
                    )
                } else {
                    return;
                }
            }
            _ => return,
        };
        out.push(Report::fixed(
            node_span(&node),
            "Use Carbon instead of `date()`, `strtotime()` or `time()` arithmetic.",
            Safety::Unsafe,
            vec![Edit::new(node_span(&node), code)],
        ));
    }
}

fn is_function(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    call: &FunctionCall<'_>,
    name: &str,
) -> bool {
    let Expression::Identifier(id) = call.function else {
        return false;
    };
    function_name(ctx, ancestors, id).eq_ignore_ascii_case(name)
}

/// No unpacked argument, and every named argument in its parameter's place.
fn readable(args: &[&Argument<'_>], parameters: &[&str]) -> bool {
    args.iter().enumerate().all(|(i, arg)| match arg {
        Argument::Positional(a) => a.ellipsis.is_none(),
        Argument::Named(a) => parameters
            .get(i)
            .is_some_and(|p| p.as_bytes() == a.name.value),
    })
}

/// The `$date` of a `strtotime($date)` timestamp.
fn strtotime_date<'s>(
    ctx: &Context<'s>,
    ancestors: &[Node<'_, '_>],
    timestamp: &Expression<'_>,
) -> Option<&'s str> {
    let Expression::Call(Call::Function(call)) = unparenthesize(timestamp) else {
        return None;
    };
    if !is_function(ctx, ancestors, call, "strtotime") {
        return None;
    }
    let args: Vec<&Argument<'_>> = call.argument_list.arguments.iter().collect();
    match args.as_slice() {
        [date] if readable(&args, &["datetime"]) => Some(text(ctx, date.value())),
        _ => None,
    }
}

/// `calculateProduct()`: an integer, or the product of a multiplication of integers. Rector skips an operand of the
/// multiplication that is not an integer (`60 * $x` is 60); Pun leaves such a subtraction alone.
fn product(expr: &Expression<'_>) -> Option<u64> {
    match unparenthesize(expr) {
        Expression::Literal(Literal::Integer(int)) => int.value,
        Expression::Binary(b) if matches!(b.operator, BinaryOperator::Multiplication(_)) => {
            product(b.lhs)?.checked_mul(product(b.rhs)?)
        }
        _ => None,
    }
}
