use mago_syntax::cst::{ClassLikeMember, LiteralStringKind, Node, NodeKind};
use pun_problem::{Edit, Safety, Span};
use serde_json::Value;

use super::annotations::{Annotated, TagValue, named_tags};
use super::prefer_test_with_attribute::php_trim;
use super::{class_body, is_in_test_class};
use crate::rules::complexity::prefer_absolute_include::print_string;
use crate::rules::support::rewrites::merge;
use crate::rules::support::statement_edits::{line_indent, previous_token_end};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferDataProvider;

impl Rule for PreferDataProvider {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-data-provider"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (members, left_brace, _) = class_body(node).unwrap();
        if !ctx.source.contains("@testWith") && !ctx.source.contains("@testwith") {
            return;
        }
        let mut scope = ancestors.to_vec();
        scope.push(node);
        if !is_in_test_class(ctx, &scope, None) {
            return;
        }
        let mut edits = Vec::new();
        let mut providers = Vec::new();
        let mut span = None;
        for member in members {
            let ClassLikeMember::Method(method) = member else {
                continue;
            };
            let annotated = Annotated::of(ctx, Node::Method(method), &scope).unwrap();
            let Some(doc) = annotated.doc.as_ref() else {
                continue;
            };
            let (Some(upper), Some(lower)) = (
                named_tags(ctx, &scope, doc, "testWith"),
                named_tags(ctx, &scope, doc, "testwith"),
            ) else {
                return;
            };
            if upper.is_empty() && lower.is_empty() {
                continue;
            }
            let mut removed = vec![false; doc.len()];
            let mut single_line = Vec::new();
            let mut multi_line = None;
            for (i, value) in upper.into_iter().chain(lower) {
                let TagValue::Generic(value) = value else {
                    return;
                };
                let mut values = Vec::new();
                for item in php_trim(&value).split('\n') {
                    let Some(value) = decode(php_trim(item)) else {
                        return;
                    };
                    values.push(value);
                }
                if values.len() > 1 {
                    multi_line = Some(create_array(&Value::Array(values.clone())));
                }
                if let [only] = &values[..] {
                    if !matches!(only, Value::Array(_) | Value::Object(_)) {
                        return;
                    }
                    single_line.push(create_array(only));
                }
                removed[i] = true;
            }
            let name = data_provider_name(&String::from_utf8_lossy(method.name.value));
            let new_doc = doc.print_appending(&removed, &[&format!("@dataProvider {name}")]);
            let Some(doc_edits) = annotated.edits(ctx.source, Some(&new_doc), &[]) else {
                return;
            };
            span.get_or_insert(doc.span);
            edits.extend(doc_edits);
            let value = if single_line.is_empty() {
                multi_line.unwrap_or_default()
            } else {
                format!("[{}]", single_line.join(", "))
            };
            providers.push((name, value));
        }
        let Some(span) = span else {
            return;
        };
        let mut at = None;
        for (index, member) in members.iter().enumerate() {
            match member {
                ClassLikeMember::Method(_) => {
                    let start = node_span(member).start;
                    let from = if index == 0 {
                        left_brace.end.offset as usize
                    } else {
                        previous_token_end(ctx, node_span(&node).start, start)
                    };
                    at = Some((from, line_indent(ctx.source, start)));
                    break;
                }
                ClassLikeMember::Property(_)
                    if !matches!(members.get(index + 1), Some(ClassLikeMember::Property(_))) =>
                {
                    let end = node_span(member).end;
                    at = Some((end, line_indent(ctx.source, node_span(member).start)));
                    break;
                }
                _ => {}
            }
        }
        let (at, indent) = at.unwrap();
        if indent.contains('\t') {
            return;
        }
        let text: String = providers
            .iter()
            .rev()
            .map(|(name, value)| {
                format!(
                    "\n{indent}public function {name}()\n{indent}{{\n{indent}    return {value};\n{indent}}}"
                )
            })
            .collect();
        edits.push(Edit::new(Span::new(at, at), text));
        out.push(Report::fixed(
            span,
            "Use a data provider method instead of the `@testWith` annotation.",
            Safety::Safe,
            merge(edits),
        ));
    }
}

fn data_provider_name(method: &str) -> String {
    let name = method.strip_prefix("test").unwrap_or(method);
    let mut chars = name.chars();
    match chars.next() {
        Some(first) => format!(
            "dataProvider{}{}",
            first.to_ascii_uppercase(),
            chars.as_str()
        ),
        None => "dataProvider".to_string(),
    }
}

fn decode(json: &str) -> Option<Value> {
    let mut digits = 0;
    for c in json.bytes() {
        digits = if c.is_ascii_digit() { digits + 1 } else { 0 };
        if digits >= 19 {
            return None;
        }
    }
    serde_json::from_str(json).ok()
}

fn create_array(value: &Value) -> String {
    let items: Vec<String> = match value.as_object() {
        Some(map) => map
            .iter()
            .map(|(key, value)| {
                let value = parse_array_item_value(value);
                if is_int_key(key) {
                    value
                } else {
                    let key =
                        print_string(LiteralStringKind::SingleQuoted, key.as_bytes()).unwrap();
                    format!("{key} => {value}")
                }
            })
            .collect(),
        None => value
            .as_array()
            .unwrap()
            .iter()
            .map(parse_array_item_value)
            .collect(),
    };
    format!("[{}]", items.join(", "))
}

fn is_int_key(key: &str) -> bool {
    let digits = key.strip_prefix('-').unwrap_or(key);
    let canonical = digits == "0" && key == "0"
        || (digits.starts_with(|c: char| ('1'..='9').contains(&c))
            && digits.bytes().all(|c| c.is_ascii_digit()));
    canonical && key.parse::<i64>().is_ok()
}

fn parse_array_item_value(value: &Value) -> String {
    match value {
        Value::Array(_) | Value::Object(_) => create_array(value),
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.to_string(),
            None => json_float(n.as_f64().unwrap()),
        },
        Value::String(s) => json_string(s),
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0'..='\u{1f}' | '\u{2028}' | '\u{2029}' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

fn json_float(value: f64) -> String {
    let shortest = format!("{value:e}");
    let digits = shortest
        .split('e')
        .next()
        .map_or(1, |m| m.bytes().filter(u8::is_ascii_digit).count());
    let exact = format!("{value:.*e}", digits - 1);
    let (m, e) = exact.split_once('e').unwrap_or((&exact, "0"));
    let scientific = format!("{}e{e}", m.trim_end_matches('0').trim_end_matches('.'));
    let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let (sign, mantissa) = match mantissa.strip_prefix('-') {
        Some(m) => ("-", m),
        None => ("", mantissa),
    };
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    if value == 0.0 {
        return format!("{sign}0.0");
    }
    if !(-4..17).contains(&exponent) {
        let (first, rest) = digits.split_at(1);
        let rest = if rest.is_empty() { "0" } else { rest };
        let exponent_sign = if exponent < 0 { '-' } else { '+' };
        return format!("{sign}{first}.{rest}e{exponent_sign}{}", exponent.abs());
    }
    if exponent < 0 {
        return format!("{sign}0.{}{digits}", "0".repeat((-exponent - 1) as usize));
    }
    let point = exponent as usize + 1;
    let whole = format!("{digits:0<point$}");
    let (int, fraction) = whole.split_at(point);
    let fraction = if fraction.is_empty() { "0" } else { fraction };
    format!("{sign}{int}.{fraction}")
}
