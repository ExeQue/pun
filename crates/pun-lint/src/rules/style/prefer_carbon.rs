//! Rector 2.8.4's datetime-to-carbon rules: `DateTimeInstanceToCarbonRector` and `DateTimeMethodCallToCarbonRector`
//! (one rule: the second rewrites the `new` that the first rewrites everywhere), with `CarbonCallFactory`, and
//! `TimeFuncCallToCarbonRector`.

use std::fmt::Write as _;
use std::sync::LazyLock;

use crate::problem::{Edit, Safety};
use mago_syntax::cst::{Expression, Literal, Node, NodeKind};
use pcre2::bytes::Regex;

use crate::rules::refactor::configured_codemods::single_quoted;
use crate::rules::refactor::{called, function_name};
use crate::rules::support::names::resolve_class_name;
use crate::syntax::node_span;
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
