//! Rector 2.8.4's `RegexDashEscapeRector`.

use std::sync::LazyLock;

use mago_syntax::cst::{LiteralStringKind, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::rector_names::phpdoc_parser::replace_all;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

fn regex(pattern: &str, caseless: bool) -> pcre2::bytes::Regex {
    pcre2::bytes::RegexBuilder::new()
        .caseless(caseless)
        .build(pattern)
        .expect("RegexDashEscapeRector pattern")
}

/// `THREE_BACKSLASH_FOR_ESCAPE_NEXT_REGEX`, `#(?<=[^\\\\])\\\\{2}(?=[^\\\\])#`.
static THREE_BACKSLASH: LazyLock<pcre2::bytes::Regex> =
    LazyLock::new(|| regex(r"(?<=[^\\])\\{2}(?=[^\\])", false));
/// `LEFT_HAND_UNESCAPED_DASH_REGEX`, `#(\[.*?\\\\(w|s|d))-(?!\])#i`.
static LEFT_HAND: LazyLock<pcre2::bytes::Regex> =
    LazyLock::new(|| regex(r"(\[.*?\\(w|s|d))-(?!\])", true));
/// `RIGHT_HAND_UNESCAPED_DASH_REGEX`, `#(?<!\[)(?<!\\\\)-(\\\\(w|s|d)[.*]?)\]#i`.
static RIGHT_HAND: LazyLock<pcre2::bytes::Regex> =
    LazyLock::new(|| regex(r"(?<!\[)(?<!\\)-(\\(w|s|d)[.*]?)\]", true));

pub struct NoUnescapedRegexDash;

impl Rule for NoUnescapedRegexDash {
    fn id(&self) -> &'static str {
        "lint/php-7.3/no-unescaped-regex-dash"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70300)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::LiteralString])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::LiteralString(string) = node else {
            return;
        };
        let Some(value) = string.value else {
            return;
        };
        if THREE_BACKSLASH.is_match(value).unwrap_or(false) {
            return;
        }
        let span = node_span(string);
        let raw = &ctx.source[span.start..span.end];
        let inner = &raw[1..raw.len() - 1];
        let replaced = if LEFT_HAND.is_match(inner.as_bytes()).unwrap_or(false) {
            replace_all(&LEFT_HAND, inner, |g| format!("{}\\-", g[1]))
        } else if RIGHT_HAND.is_match(inner.as_bytes()).unwrap_or(false) {
            replace_all(&RIGHT_HAND, inner, |g| format!("\\-{}]", g[1]))
        } else {
            return;
        };
        // `setRawValue`.
        let quote = match string.kind {
            LiteralStringKind::DoubleQuoted => '"',
            LiteralStringKind::SingleQuoted => '\'',
        };
        out.push(Report::fixed(
            span,
            "Escape the `-` next to a character class in this pattern: PHP 7.3's PCRE2 rejects it.",
            Safety::Safe,
            vec![Edit::new(span, format!("{quote}{replaced}{quote}"))],
        ));
    }
}
