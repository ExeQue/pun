//! Rector 2.8.4's `Php81\NodeFactory\EnumFactory`, which `MyCLabsClassToEnumRector` and `SpatieEnumClassToEnumRector`
//! share, with the printing of the new `Enum_` node by php-parser's pretty printer: the class's own comments go, each
//! member sits on its own line with no blank line between members, and the members kept from the class keep their
//! text.

use std::sync::LazyLock;

use mago_syntax::cst::{
    ArrayElement, BinaryOperator, Class, ClassLikeMember, Expression, Literal, LiteralStringKind,
    MethodBody, Node,
};

use crate::Context;
use crate::problem::{Edit, Span};
use crate::rules::complexity::prefer_absolute_include::print_string;
use crate::rules::phpunit::annotations::reformatted;
use crate::rules::support::docblocks::Doc;
use crate::rules::support::statement_edits::{line_indent, previous_token_end, tokens};
use crate::rules::types::returns_scoped;
use crate::syntax::{node_span, unparenthesize};

/// One statement of the enum: its comments (as php-parser reformats them) and its code.
struct Member {
    comments: Vec<String>,
    code: String,
}

/// `createFromClass()`: the trait uses, a case for the first constant of each declaration, then the methods.
pub(super) fn from_class(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], class: &Class<'_>) -> Edit {
    let mut members: Vec<Member> = (class.members.iter())
        .filter(|m| matches!(m, ClassLikeMember::TraitUse(_)))
        .map(|m| kept(ctx, class, m))
        .collect();
    let constants: Vec<_> = (class.members.iter())
        .filter_map(|m| match m {
            ClassLikeMember::Constant(c) => Some((m, c)),
            _ => None,
        })
        .collect();
    let mut scalar = None;
    if let Some((_, first)) = constants.first() {
        scalar = Some(if is_string(first.first_item().value) {
            "string"
        } else {
            "int"
        });
        for (member, constant) in &constants {
            let item = constant.first_item();
            members.push(Member {
                comments: comments(ctx, class, node_span(*member).start),
                code: format!(
                    "case {} = {};",
                    String::from_utf8_lossy(item.name.value),
                    &ctx.source[node_span(item.value).start..node_span(item.value).end]
                ),
            });
        }
    }
    members.extend(
        (class.members.iter())
            .filter(|m| matches!(m, ClassLikeMember::Method(_)))
            .map(|m| kept(ctx, class, m)),
    );
    print(ctx, ancestors, class, scalar, &members)
}

/// `createFromSpatieClass()`: a case for each `@method` tag of the class's docblock, valued from what `values()`
/// returns or else the method name.
pub(super) fn from_spatie_class(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    class: &Class<'_>,
    snake_case: bool,
) -> Edit {
    let names: Vec<String> =
        Doc::of(ctx, ancestors, node_span(class).start).map_or_else(Vec::new, |doc| {
            (0..doc.len())
                .filter_map(|i| method_tag_name(doc.text(i)))
                .collect()
        });
    let mut scalar = None;
    let mut members = Vec::new();
    if !names.is_empty() {
        let mapping = mapping_from_class(class);
        // `getIdentifierTypeFromMappings()`: `int` only when every value is an integer.
        let ints = !mapping.is_empty() && mapping.iter().all(|(_, v)| matches!(v, Value::Int(_)));
        scalar = Some(if ints { "int" } else { "string" });
        for name in names {
            let value = mapping
                .iter()
                .rev()
                .find(|(k, _)| k.as_bytes() == name.as_bytes())
                .map_or_else(
                    || Value::String(name.clone().into_bytes()),
                    |(_, v)| v.clone(),
                );
            let case = if snake_case {
                upper_snake_case(&name)
            } else {
                name.to_ascii_uppercase()
            };
            let value = match value {
                Value::Int(i) => i.to_string(),
                Value::String(s) => print_string(LiteralStringKind::SingleQuoted, &s)
                    .unwrap_or_else(|| String::from_utf8_lossy(&s).into_owned()),
            };
            members.push(Member {
                comments: Vec::new(),
                code: format!("case {case} = {value};"),
            });
        }
    }
    print(ctx, ancestors, class, scalar, &members)
}

/// What `ValueResolver::getValue()` gives for the first constant is a string: a string literal or a concatenation.
fn is_string(value: &Expression<'_>) -> bool {
    match unparenthesize(value) {
        Expression::Literal(Literal::String(_)) | Expression::CompositeString(_) => true,
        Expression::Binary(b) => matches!(b.operator, BinaryOperator::StringConcat(_)),
        _ => false,
    }
}

/// A member the enum keeps as it is: its comments and its text.
fn kept(ctx: &Context<'_>, class: &Class<'_>, member: &ClassLikeMember<'_>) -> Member {
    let span = node_span(member);
    Member {
        comments: comments(ctx, class, span.start),
        code: ctx.source[span.start..span.end].to_string(),
    }
}

/// The comments php-parser attaches to the member starting at `start`: those after the token before it.
fn comments(ctx: &Context<'_>, class: &Class<'_>, start: usize) -> Vec<String> {
    let from = previous_token_end(ctx, class.left_brace.start.offset as usize, start);
    tokens(ctx.source, from, start)
        .into_iter()
        .filter(|t| t.kind.is_comment())
        .map(|t| reformatted(ctx.source[t.start..t.end].trim_end_matches(['\n', '\r'])))
        .collect()
}

/// `pStmt_Enum()` in place of the class and the comments before it.
fn print(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    class: &Class<'_>,
    scalar: Option<&str>,
    members: &[Member],
) -> Edit {
    let span = node_span(class);
    let outer = (ancestors.iter().rev())
        .map(|a| node_span(a).start)
        .find(|&s| s < span.start)
        .unwrap_or(0);
    let from = previous_token_end(ctx, outer, span.start);
    let start = tokens(ctx.source, from, span.start)
        .into_iter()
        .find(|t| t.kind.is_comment())
        .map_or(span.start, |t| t.start);
    let nl = format!("\n{}", line_indent(ctx.source, span.start));
    let inner = format!("{nl}    ");
    let mut out = format!("enum {}", String::from_utf8_lossy(class.name.value));
    if let Some(scalar) = scalar {
        out.push_str(" : ");
        out.push_str(scalar);
    }
    out.push_str(&nl);
    out.push('{');
    for member in members {
        if !member.comments.is_empty() {
            out.push_str(&inner);
            out.push_str(&member.comments.join(&inner).replace('\n', &inner));
        }
        out.push_str(&inner);
        out.push_str(&member.code);
    }
    out.push_str(&nl);
    out.push('}');
    Edit::new(Span::new(start, span.end), out)
}

#[derive(Clone)]
enum Value {
    Int(u64),
    String(Vec<u8>),
}

/// `generateMappingFromClass()`: the literal `key => value` items of the arrays `values()` returns.
fn mapping_from_class(class: &Class<'_>) -> Vec<(String, Value)> {
    let Some(body) = class.members.iter().find_map(|m| match m {
        ClassLikeMember::Method(m) if m.name.value.eq_ignore_ascii_case(b"values") => {
            match &m.body {
                MethodBody::Concrete(body) => Some(body),
                MethodBody::Abstract(_) => None,
            }
        }
        _ => None,
    }) else {
        return Vec::new();
    };
    let mut mapping = Vec::new();
    for ret in returns_scoped(body) {
        let elements = match ret.value.map(unparenthesize) {
            Some(Expression::Array(a)) => &a.elements,
            Some(Expression::LegacyArray(a)) => &a.elements,
            _ => continue,
        };
        for element in elements {
            let ArrayElement::KeyValue(item) = element else {
                continue;
            };
            let (Some(key), Some(value)) = (literal(item.key), literal(item.value)) else {
                continue;
            };
            let key = match key {
                Value::Int(i) => i.to_string(),
                Value::String(s) => String::from_utf8_lossy(&s).into_owned(),
            };
            mapping.push((key, value));
        }
    }
    mapping
}

/// An `Int_` or a `String_` node and its value.
fn literal(expr: &Expression<'_>) -> Option<Value> {
    match expr {
        Expression::Literal(Literal::Integer(i)) => i.value.map(Value::Int),
        Expression::Literal(Literal::String(s)) => s.value.map(|v| Value::String(v.to_vec())),
        _ => None,
    }
}

/// The method name of an `@method` tag: the name before its parameter list.
fn method_tag_name(tag: &str) -> Option<String> {
    let rest = tag.strip_prefix("@method")?;
    if !rest.starts_with([' ', '\t']) {
        return None;
    }
    let before = rest[..rest.find('(')?].trim_end();
    let start = before
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || !c.is_ascii()))
        .map_or(0, |i| i + 1);
    let name = &before[start..];
    (!name.is_empty() && !name.as_bytes()[0].is_ascii_digit()).then(|| name.to_string())
}

static PASCAL_CASE_TO_UNDERSCORE: LazyLock<pcre2::bytes::Regex> = LazyLock::new(|| {
    pcre2::bytes::Regex::new(
        "(?<=[A-Z])(?=[A-Z][a-z])|(?<=[^A-Z])(?=[A-Z])|(?<=[A-Za-z])(?=[^A-Za-z])",
    )
    .expect("PASCAL_CASE_TO_UNDERSCORE_REGEX")
});

/// `strtoupper(Strings::replace($name, PASCAL_CASE_TO_UNDERSCORE_REGEX, '_$0'))` with runs of `_` made one.
fn upper_snake_case(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() * 2);
    let mut at = 0;
    for found in PASCAL_CASE_TO_UNDERSCORE.find_iter(bytes) {
        let found = found.expect("the regex matches");
        out.extend_from_slice(&bytes[at..found.start()]);
        out.push(b'_');
        out.extend_from_slice(found.as_bytes());
        at = found.end();
    }
    out.extend_from_slice(&bytes[at..]);
    let mut squeezed = Vec::with_capacity(out.len());
    for b in out {
        if !(b == b'_' && squeezed.last() == Some(&b'_')) {
            squeezed.push(b.to_ascii_uppercase());
        }
    }
    String::from_utf8_lossy(&squeezed).into_owned()
}
