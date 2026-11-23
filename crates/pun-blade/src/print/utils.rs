//! `print/utils.ts`, `blade-options.ts` (with Pint's fixed options), `doc-utils.ts` and the
//! small helpers around them.

use std::collections::HashSet;

use super::Printer;
use crate::ast::{Id, Node, is_echo_like};
use crate::doc::{Doc, concat, literalline};
use crate::html_data::{HTML_ELEMENT_ATTRIBUTES, HTML_TAGS};
use crate::ignore::{IgnoreKind, comment_kind};
use crate::lexer::{Token, TokenType};
use crate::tree::{NodeKind, js_trim};

// ---------------------------------------------------------------------------------------------
// Options (Pint's `prettierrc.json` on top of the plugin defaults)

pub(crate) const PRINT_WIDTH: usize = 120;
pub(crate) const TAB_WIDTH: usize = 4;

const BLADE_COMPONENT_PREFIXES: &[&str] = &[
    "x-", "x:", "s-", "s:", "statamic-", "statamic:", "flux-", "flux:", "livewire-", "livewire:", "native-", "native:",
];

const DIRECTIVE_ARG_SPACING_SPACE: &[&str] = &["if", "elseif", "unless", "while", "for", "foreach", "forelse", "switch", "case"];

pub(crate) fn is_blade_component_tag_name(full_name: &str) -> bool {
    let n = js_trim(full_name).to_lowercase();
    !n.is_empty() && BLADE_COMPONENT_PREFIXES.iter().any(|p| n.starts_with(p))
}

/// `resolveDirectiveArgSpacingRule` + `getDirectiveArgSpacingText` for Pint's options: the
/// spacing between a directive name and its arguments.
pub(crate) fn directive_arg_spacing(name: &str, in_component_attribute: bool) -> &'static str {
    if in_component_attribute {
        return "";
    }
    let trimmed = js_trim(name);
    let lookup = trimmed.strip_prefix('@').unwrap_or(trimmed).to_lowercase();
    if DIRECTIVE_ARG_SPACING_SPACE.contains(&lookup.as_str()) { " " } else { "" }
}

/// Inline intent elements are `["svg", "svg:*"]`.
pub(crate) fn preserve_inline_intent_element(tag_name: &str, _namespace: &str) -> bool {
    tag_name.to_lowercase() == "svg"
}

pub(crate) fn preserve_inline_intent_namespace(namespace: &str) -> bool {
    namespace.to_lowercase() == "svg"
}

pub(crate) fn preserve_inline_intent_attributes(tag_name: &str, namespace: &str) -> bool {
    if preserve_inline_intent_element(tag_name, namespace) {
        return true;
    }
    let tag = tag_name.to_lowercase();
    let ns = namespace.to_lowercase();
    if ns.is_empty() || (ns == "svg" && tag == "foreignobject") {
        return false;
    }
    preserve_inline_intent_namespace(&ns)
}

// ---------------------------------------------------------------------------------------------
// String helpers

pub(crate) fn is_html_ws(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{c}' | '\r' | ' ')
}

/// JavaScript `\s`.
pub(crate) fn is_js_ws(c: char) -> bool {
    crate::tree::is_js_whitespace(c)
}

/// `s.replace(/\s+$/u, "")`
pub(crate) fn trim_end_js(s: &str) -> &str {
    crate::tree::js_trim_end(s)
}

/// `s.split(/[\t\n\f\r ]+/)`
pub(crate) fn html_whitespace_split(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_ws = false;
    for c in s.chars() {
        if is_html_ws(c) {
            if !in_ws {
                out.push(std::mem::take(&mut cur));
            }
            in_ws = true;
        } else {
            cur.push(c);
            in_ws = false;
        }
    }
    out.push(cur);
    out
}

/// Slice off the first `n` UTF-16 units of an ASCII-indented line.
fn slice_chars(line: &str, n: usize) -> &str {
    match line.char_indices().nth(n) {
        Some((i, _)) => &line[i..],
        None => "",
    }
}

fn leading_indent(line: &str) -> usize {
    line.bytes().take_while(|b| matches!(b, b' ' | b'\t')).count()
}

/// `dedentString`
pub(crate) fn dedent_string(s: &str) -> String {
    let lines: Vec<&str> = s.split('\n').collect();
    let mut min = usize::MAX;
    for line in &lines {
        if js_trim(line).is_empty() {
            continue;
        }
        min = min.min(leading_indent(line));
    }
    if min == 0 || min == usize::MAX {
        return s.to_owned();
    }
    lines.iter().map(|l| slice_chars(l, min)).collect::<Vec<_>>().join("\n")
}

/// `htmlTrimPreserveIndentation`
pub(crate) fn html_trim_preserve_indentation(s: &str) -> String {
    let s = s.trim_end_matches(is_html_ws);
    // `/^[\t\f\r ]*\n/g` only ever matches once, at the start.
    let skip = s.bytes().take_while(|b| matches!(b, b'\t' | 0x0c | b'\r' | b' ')).count();
    if s.as_bytes().get(skip) == Some(&b'\n') { s[skip + 1..].to_owned() } else { s.to_owned() }
}

/// `trimTrailingHorizontalWhitespace`: `/[ \t]+(?=\r?\n|$)/gu` → "".
pub(crate) fn trim_trailing_horizontal_whitespace(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut pending = String::new();
    let b = value.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b' ' || c == b'\t' {
            pending.push(c as char);
            i += 1;
            continue;
        }
        let at_line_end = c == b'\n' || (c == b'\r' && b.get(i + 1) == Some(&b'\n'));
        if !at_line_end {
            out.push_str(&pending);
        }
        pending.clear();
        let ch_len = value[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&value[i..i + ch_len]);
        i += ch_len;
    }
    out
}

pub(crate) fn normalize_line_endings_to_lf(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// The plugin's `replaceEndOfLine`: splits on `\n`, skipping empty lines.
pub(crate) fn replace_eol(text: &str, replacement: &Doc) -> Vec<Doc> {
    let mut parts = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            parts.push(replacement.clone());
        }
        if !line.is_empty() {
            parts.push(Doc::from(line));
        }
    }
    parts
}

pub(crate) fn replace_eol_literal(text: &str) -> Doc {
    concat(replace_eol(text, &literalline()))
}

pub(crate) fn unescape_quote_entities(text: &str) -> String {
    text.replace("&apos;", "'").replace("&quot;", "\"")
}

pub(crate) fn is_html_tag(name: &str) -> bool {
    HTML_TAGS.binary_search(&name).is_ok()
}

fn element_attributes(tag: &str) -> Option<&'static [&'static str]> {
    HTML_ELEMENT_ATTRIBUTES.binary_search_by(|(k, _)| (*k).cmp(tag)).ok().map(|i| HTML_ELEMENT_ATTRIBUTES[i].1)
}

pub(crate) fn has_blank_line_between(between: &str) -> bool {
    // `/\r?\n\s*\r?\n/`
    let mut seen_newline = false;
    for c in between.chars() {
        if c == '\n' {
            if seen_newline {
                return true;
            }
            seen_newline = true;
        } else if !is_js_ws(c) {
            seen_newline = false;
        }
    }
    false
}

/// `/(?:\r\n|\r|\n)[^\S\r\n]*(?:\r\n|\r|\n)/u`
pub(crate) fn has_blank_line_strict(between: &str) -> bool {
    let mut seen_newline = false;
    let mut chars = between.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            if seen_newline {
                return true;
            }
            seen_newline = true;
        } else if !is_js_ws(c) {
            seen_newline = false;
        }
    }
    false
}

pub(crate) fn has_line_break(s: &str) -> bool {
    s.contains(['\r', '\n'])
}

// ---------------------------------------------------------------------------------------------
// Node helpers

impl Printer<'_> {
    pub(crate) fn n(&self, id: Id) -> &Node {
        &self.ast.nodes[id]
    }

    pub(crate) fn kind(&self, id: Id) -> NodeKind {
        self.ast.nodes[id].kind
    }

    pub(crate) fn source(&self) -> &str {
        &self.ast.source
    }

    pub(crate) fn full_text(&self, id: Id) -> &str {
        let n = self.n(id);
        if n.kind == NodeKind::FrontMatter {
            return self.ast.front_matter.as_ref().map_or("", |f| f.raw.as_str());
        }
        &self.ast.source[n.start..n.end]
    }

    pub(crate) fn tokens_of(&self, id: Id) -> &[Token] {
        let f = &self.ast.flat[self.n(id).flat];
        if self.kind(id) == NodeKind::FrontMatter {
            return &[];
        }
        &self.ast.tokens[f.token_start..f.token_start + f.token_count]
    }

    pub(crate) fn tok_text(&self, t: &Token) -> &str {
        &self.ast.source[t.start..t.end]
    }

    pub(crate) fn flat_data(&self, id: Id) -> usize {
        self.ast.flat[self.n(id).flat].data
    }

    pub(crate) fn last_child(&self, id: Id) -> Option<Id> {
        self.n(id).children.last().copied()
    }

    pub(crate) fn first_child(&self, id: Id) -> Option<Id> {
        self.n(id).children.first().copied()
    }

    pub(crate) fn is_text_like(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind == NodeKind::Comment {
            return !n.is_ie_conditional_start_comment;
        }
        matches!(n.kind, NodeKind::Text | NodeKind::Cdata | NodeKind::BogusComment | NodeKind::UnpairedClosingTag)
            || is_echo_like(n.kind)
    }

    pub(crate) fn is_script_like(&self, id: Id) -> bool {
        self.ast.is_script_like_tag(id)
    }

    pub(crate) fn is_pre_like(&self, id: Id) -> bool {
        self.n(id).is_indentation_sensitive
    }

    pub(crate) fn last_descendant(&self, id: Id) -> Id {
        let mut cur = id;
        while let Some(c) = self.last_child(cur) {
            cur = c;
        }
        cur
    }

    pub(crate) fn has_leading_line_break(&self, id: Id) -> bool {
        let n = self.n(id);
        if !n.has_leading_spaces {
            return false;
        }
        if let Some(p) = n.prev {
            return self.n(p).end_line < n.start_line;
        }
        let Some(parent) = n.parent else { return false };
        self.kind(parent) == NodeKind::Root || self.n(parent).start_line < n.start_line
    }

    pub(crate) fn has_trailing_line_break(&self, id: Id) -> bool {
        let n = self.n(id);
        if !n.has_trailing_spaces {
            return false;
        }
        if let Some(x) = n.next {
            return self.n(x).start_line > n.end_line;
        }
        let Some(parent) = n.parent else { return false };
        self.kind(parent) == NodeKind::Root || self.n(parent).end_line > n.end_line
    }

    pub(crate) fn has_surrounding_line_break(&self, id: Id) -> bool {
        self.has_leading_line_break(id) && self.has_trailing_line_break(id)
    }

    fn has_non_text_child(&self, id: Id) -> bool {
        self.n(id).children.iter().any(|&c| self.kind(c) != NodeKind::Text && !is_echo_like(self.kind(c)))
    }

    pub(crate) fn force_break_children(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind != NodeKind::Element || n.children.is_empty() {
            return false;
        }
        matches!(n.tag_name.as_str(), "html" | "head" | "ul" | "ol" | "select")
            || (n.css_display.starts_with("table") && n.css_display != "table-cell")
    }

    pub(crate) fn force_break_content(&self, id: Id) -> bool {
        if self.force_break_children(id) {
            return true;
        }
        let n = self.n(id);
        if n.kind != NodeKind::Element || n.children.is_empty() {
            return false;
        }
        if is_blade_slot_element(n) {
            return true;
        }
        let first = n.children[0];
        let last = n.children[n.children.len() - 1];
        matches!(n.tag_name.as_str(), "body" | "script" | "style")
            || n.children.iter().any(|&c| self.has_non_text_child(c))
            || (first == last
                && self.kind(first) != NodeKind::Text
                && !is_echo_like(self.kind(first))
                && self.has_leading_line_break(first)
                && (!self.n(last).is_trailing_space_sensitive || self.has_trailing_line_break(last)))
    }

    pub(crate) fn prefer_hardline_as_surrounding_spaces(&self, id: Id) -> bool {
        let n = self.n(id);
        match n.kind {
            NodeKind::Comment
            | NodeKind::BogusComment
            | NodeKind::BladeComment
            | NodeKind::ConditionalComment
            | NodeKind::Directive
            | NodeKind::DirectiveBlock => true,
            NodeKind::Element => n.tag_name == "script" || n.tag_name == "select",
            _ => false,
        }
    }

    pub(crate) fn prefer_hardline_as_leading_spaces(&self, id: Id) -> bool {
        self.prefer_hardline_as_surrounding_spaces(id)
            || self.n(id).prev.is_some_and(|p| self.prefer_hardline_as_trailing_spaces(p))
            || self.has_surrounding_line_break(id)
    }

    pub(crate) fn prefer_hardline_as_trailing_spaces(&self, id: Id) -> bool {
        self.prefer_hardline_as_surrounding_spaces(id)
            || (self.kind(id) == NodeKind::Element && self.n(id).tag_name == "br")
            || self.has_surrounding_line_break(id)
    }

    pub(crate) fn should_preserve_content(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind == NodeKind::ConditionalComment {
            if let Some(last) = self.last_child(id) {
                let l = self.n(last);
                if !l.is_self_closing && !l.has_closing_tag {
                    return true;
                }
            }
            if !n.complete {
                return true;
            }
            if n.children.iter().any(|&c| self.kind(c) == NodeKind::UnpairedClosingTag) {
                return true;
            }
        }
        if self.is_pre_like(id)
            && n.children.iter().any(|&c| self.kind(c) != NodeKind::Text && !is_echo_like(self.kind(c)))
        {
            return true;
        }
        if n.kind == NodeKind::Element && self.is_script_like(id) && self.should_preserve_raw_content_element_with_blade(id) {
            return true;
        }
        false
    }

    fn should_preserve_raw_content_element_with_blade(&self, id: Id) -> bool {
        let n = self.n(id);
        let context = match n.tag_name.as_str() {
            "script" => RawContext::Script,
            "style" => RawContext::Style,
            _ => RawContext::Generic,
        };
        if context == RawContext::Script {
            let has = n
                .children
                .iter()
                .any(|&c| self.kind(c) != NodeKind::IgnoreRange && self.is_blade_construct_child(c, RawContext::Script));
            if !has {
                return false;
            }
            return self.infer_element_parser(id).is_none();
        }
        self.parent_contains_blade_syntax(id, context)
    }

    pub(crate) fn force_next_empty_line(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind == NodeKind::FrontMatter {
            return true;
        }
        n.next.is_some_and(|x| n.end_line + 1 < self.n(x).start_line)
    }

    // -- prettier-ignore ----------------------------------------------------------------------

    pub(crate) fn ignore_comment_kind(&self, id: Id) -> Option<IgnoreKind> {
        match self.kind(id) {
            NodeKind::Comment => comment_kind(self.full_text(id), false),
            NodeKind::BladeComment => comment_kind(self.full_text(id), true),
            _ => None,
        }
    }

    fn ignored_children(&self, parent: Id) -> HashSet<Id> {
        if let Some(set) = self.ignored.borrow().get(&parent) {
            return set.clone();
        }
        let mut ignored = HashSet::new();
        let mut count = 0;
        for &c in &self.n(parent).children {
            let k = self.ignore_comment_kind(c);
            if count > 0 && k != Some(IgnoreKind::Ignore) {
                ignored.insert(c);
                count -= 1;
            }
            if k == Some(IgnoreKind::Ignore) {
                count = count.max(1);
            }
        }
        self.ignored.borrow_mut().insert(parent, ignored.clone());
        ignored
    }

    pub(crate) fn has_prettier_ignore(&self, id: Id) -> bool {
        if self.kind(id) == NodeKind::Attribute {
            return false;
        }
        let Some(parent) = self.n(id).parent else { return false };
        self.ignored_children(parent).contains(&id)
    }

    pub(crate) fn printable_subtree_end(&self, id: Id) -> usize {
        let n = self.n(id);
        let mut end = n.end;
        for &c in n.attrs.iter().chain(n.children.iter()) {
            end = end.max(self.printable_subtree_end(c));
        }
        end
    }

    // -- attributes --------------------------------------------------------------------------

    pub(crate) fn attribute_name_raw(&self, id: Id) -> String {
        let mut start = None;
        let mut end = None;
        for t in self.tokens_of(id) {
            if t.kind == TokenType::Equals {
                break;
            }
            if start.is_none() {
                start = Some(t.start);
            }
            if t.kind != TokenType::Whitespace {
                end = Some(t.end);
            }
        }
        match (start, end) {
            (Some(s), Some(e)) if e >= s => self.ast.source[s..e].to_owned(),
            _ => String::new(),
        }
    }

    pub(crate) fn is_static_attribute_name(&self, id: Id) -> bool {
        for t in self.tokens_of(id) {
            match t.kind {
                TokenType::Equals => break,
                TokenType::EchoStart
                | TokenType::RawEchoStart
                | TokenType::TripleEchoStart
                | TokenType::Directive
                | TokenType::PhpTagStart => return false,
                _ => {}
            }
        }
        true
    }

    pub(crate) fn static_attribute_name_lower(&self, id: Id) -> Option<String> {
        if !self.is_static_attribute_name(id) {
            return None;
        }
        let raw = self.attribute_name_raw(id);
        (!raw.is_empty()).then(|| raw.to_lowercase())
    }

    pub(crate) fn attribute_name_parts(&self, id: Id) -> Vec<(NamePart, String)> {
        let tokens = self.tokens_of(id);
        let mut parts = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let t = tokens[i];
            let construct = match t.kind {
                TokenType::EchoStart => Some((NamePart::Echo, TokenType::EchoEnd)),
                TokenType::RawEchoStart => Some((NamePart::RawEcho, TokenType::RawEchoEnd)),
                TokenType::TripleEchoStart => Some((NamePart::TripleEcho, TokenType::TripleEchoEnd)),
                TokenType::PhpTagStart => Some((NamePart::PhpTag, TokenType::PhpTagEnd)),
                TokenType::PhpBlockStart => Some((NamePart::PhpBlock, TokenType::PhpBlockEnd)),
                _ => None,
            };
            if let Some((kind, end_type)) = construct {
                let start = t.start;
                let mut end = t.end;
                i += 1;
                while i < tokens.len() {
                    end = tokens[i].end;
                    if tokens[i].kind == end_type {
                        break;
                    }
                    i += 1;
                }
                parts.push((kind, self.ast.source[start..end].to_owned()));
                i += 1;
                continue;
            }
            match t.kind {
                TokenType::Equals => break,
                TokenType::Whitespace => {}
                TokenType::Directive => {
                    let start = t.start;
                    let mut end = t.end;
                    if i + 1 < tokens.len() && tokens[i + 1].kind == TokenType::DirectiveArgs {
                        i += 1;
                        end = tokens[i].end;
                    }
                    parts.push((NamePart::Directive, self.ast.source[start..end].to_owned()));
                }
                _ => parts.push((NamePart::Text, self.tok_text(&t).to_owned())),
            }
            i += 1;
        }
        parts
    }

    /// Token index range (relative to the attribute's tokens) of its value.
    fn attribute_value_range(&self, id: Id) -> Option<ValueRange> {
        let tokens = self.tokens_of(id);
        let eq = tokens.iter().position(|t| t.kind == TokenType::Equals)?;
        let mut start = eq + 1;
        while start < tokens.len() && tokens[start].kind == TokenType::Whitespace {
            start += 1;
        }
        if start >= tokens.len() {
            return Some(ValueRange { start, end: start, quote: None });
        }
        if tokens[start].kind != TokenType::Quote {
            return Some(ValueRange { start, end: tokens.len(), quote: None });
        }
        let quote = self.tok_text(&tokens[start]).to_owned();
        let mut end = tokens.len();
        if end > start + 1 && tokens[end - 1].kind == TokenType::Quote {
            end -= 1;
        }
        Some(ValueRange { start: start + 1, end, quote: Some(quote) })
    }

    pub(crate) fn attribute_raw_value(&self, id: Id) -> Option<String> {
        let range = self.attribute_value_range(id)?;
        if range.start >= range.end {
            return Some(String::new());
        }
        let tokens = self.tokens_of(id);
        let mut value = String::new();
        for t in &tokens[range.start..range.end] {
            if range.quote.is_some() && t.kind == TokenType::Quote {
                continue;
            }
            value.push_str(self.tok_text(t));
        }
        Some(value)
    }

    pub(crate) fn attribute_value_quote(&self, id: Id) -> Option<String> {
        self.attribute_value_range(id)?.quote
    }

    pub(crate) fn attribute_value_kind(&self, id: Id) -> ValueKind {
        let Some(range) = self.attribute_value_range(id) else { return ValueKind::None };
        if range.start >= range.end {
            return ValueKind::StaticText;
        }
        let tokens = self.tokens_of(id);
        let mut dynamic = false;
        let mut text = false;
        let mut i = range.start;
        while i < range.end {
            let t = tokens[i];
            if t.kind == TokenType::JsxAttributeValue {
                dynamic = true;
                i += 1;
                continue;
            }
            let end_type = match t.kind {
                TokenType::EchoStart => Some(TokenType::EchoEnd),
                TokenType::RawEchoStart => Some(TokenType::RawEchoEnd),
                TokenType::TripleEchoStart => Some(TokenType::TripleEchoEnd),
                TokenType::PhpTagStart => Some(TokenType::PhpTagEnd),
                TokenType::PhpBlockStart => Some(TokenType::PhpBlockEnd),
                _ => None,
            };
            if let Some(end_type) = end_type {
                dynamic = true;
                i += 1;
                while i < range.end && tokens[i].kind != end_type {
                    i += 1;
                }
                i += 1;
                continue;
            }
            if t.kind == TokenType::Directive {
                dynamic = true;
                if i + 1 < range.end && tokens[i + 1].kind == TokenType::DirectiveArgs {
                    i += 1;
                }
                i += 1;
                continue;
            }
            if self.tok_text(&t).contains("<?") {
                dynamic = true;
            } else {
                text = true;
            }
            i += 1;
        }
        if !dynamic {
            ValueKind::StaticText
        } else if text {
            ValueKind::Mixed
        } else {
            ValueKind::PureConstruct
        }
    }

    pub(crate) fn is_static_attribute_value(&self, id: Id) -> bool {
        self.attribute_value_kind(id) == ValueKind::StaticText
    }

    pub(crate) fn unescaped_attribute_value(&self, id: Id) -> String {
        match self.attribute_raw_value(id) {
            Some(raw) if !raw.is_empty() => unescape_quote_entities(&raw),
            _ => String::new(),
        }
    }

    /// `getAttributeValueTokenRange` (absolute token indexes relative to the attribute).
    pub(crate) fn attribute_value_token_range(&self, id: Id) -> Option<(usize, usize)> {
        self.attribute_value_range(id).map(|r| (r.start, r.end))
    }

    pub(crate) fn attr_map_get(&self, id: Id, key: &str) -> Option<String> {
        let mut found = None;
        for &a in &self.n(id).attrs {
            if !self.is_static_attribute_name(a) {
                continue;
            }
            let name = self.attribute_name_raw(a);
            if !name.is_empty() && name.to_lowercase() == key {
                found = Some(self.attribute_raw_value(a).unwrap_or_default());
            }
        }
        found
    }

    pub(crate) fn has_attr(&self, id: Id, key: &str) -> bool {
        self.attr_map_get(id, key).is_some()
    }

    /// `inferElementParser`
    pub(crate) fn infer_element_parser(&self, id: Id) -> Option<&'static str> {
        let full = self.n(id).full_name.to_lowercase();
        if full == "script" {
            if self.has_attr(id, "src") {
                return None;
            }
            let ty = self.attr_map_get(id, "type").filter(|s| !s.is_empty());
            let lang = self.attr_map_get(id, "lang").filter(|s| !s.is_empty());
            if ty.is_none() && lang.is_none() {
                return Some("babel");
            }
            return infer_parser_by_language(lang.as_deref()).or_else(|| infer_parser_by_type(ty.as_deref()));
        }
        if full == "style" {
            if let Some(lang) = self.attr_map_get(id, "lang").filter(|s| !s.is_empty()) {
                return infer_parser_by_language(Some(&lang));
            }
            return Some("css");
        }
        None
    }

    pub(crate) fn normalize_attribute_name(&self, name: &str, parent: Option<Id>) -> String {
        let Some(p) = parent.filter(|&p| self.kind(p) == NodeKind::Element) else { return name.to_owned() };
        if name.contains(':') {
            return name.to_owned();
        }
        let lower = name.to_lowercase();
        let tag = &self.n(p).tag_name;
        if let Some(attrs) = element_attributes(tag) {
            let global = element_attributes("*").unwrap_or(&[]);
            if global.binary_search(&lower.as_str()).is_ok() || attrs.binary_search(&lower.as_str()).is_ok() {
                return lower;
            }
        }
        name.to_owned()
    }

    pub(crate) fn is_static_element_name(&self, id: Id) -> bool {
        let n = self.n(id);
        n.kind != NodeKind::Element
            || !(n.raw_tag_name.contains("{{")
                || n.raw_tag_name.contains("{!!")
                || n.raw_tag_name.contains("<?")
                || n.raw_tag_name.contains('@'))
    }
}

pub(crate) fn is_blade_slot_element(n: &Node) -> bool {
    if n.kind != NodeKind::Element {
        return false;
    }
    let f = n.full_name.to_lowercase();
    f == "x-slot" || f.starts_with("x-slot:") || f.starts_with("x-slot[")
}

fn infer_parser_by_language(lang: Option<&str>) -> Option<&'static str> {
    // Prettier's built-in languages (name, aliases, extensions), searched in that order.
    let lang = lang?;
    let by_name: &[(&str, &str)] = &[
        ("javascript", "babel"),
        ("flow", "flow"),
        ("jsx", "babel"),
        ("typescript", "typescript"),
        ("tsx", "typescript"),
        ("json", "json"),
        ("css", "css"),
        ("postcss", "css"),
        ("less", "less"),
        ("scss", "scss"),
        ("markdown", "markdown"),
        ("mdx", "mdx"),
        ("html", "html"),
        ("vue", "vue"),
        ("yaml", "yaml"),
        ("graphql", "graphql"),
        ("handlebars", "glimmer"),
    ];
    if let Some((_, p)) = by_name.iter().find(|(n, _)| *n == lang) {
        return Some(p);
    }
    let by_alias_or_ext: &[(&str, &str)] = &[
        ("js", "babel"),
        ("mjs", "babel"),
        ("cjs", "babel"),
        ("ts", "typescript"),
        ("mts", "typescript"),
        ("cts", "typescript"),
        ("md", "markdown"),
        ("pcss", "css"),
        ("yml", "yaml"),
        ("gql", "graphql"),
        ("hbs", "glimmer"),
    ];
    by_alias_or_ext.iter().find(|(n, _)| *n == lang).map(|(_, p)| *p)
}

fn infer_parser_by_type(ty: Option<&str>) -> Option<&'static str> {
    let ty = ty?;
    let explicit = match ty {
        "module" | "text/javascript" | "text/babel" | "text/jsx" | "application/javascript" => Some("babel"),
        "application/x-typescript" => Some("typescript"),
        "text/markdown" => Some("markdown"),
        "text/html" => Some("html"),
        "text/x-handlebars-template" => Some("glimmer"),
        _ => None,
    };
    if explicit.is_some() {
        return explicit;
    }
    if ty.ends_with("json") || ty.ends_with("importmap") || ty == "speculationrules" {
        return Some("json");
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NamePart {
    Text,
    Echo,
    RawEcho,
    TripleEcho,
    Directive,
    PhpTag,
    PhpBlock,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueKind {
    None,
    StaticText,
    PureConstruct,
    Mixed,
}

struct ValueRange {
    start: usize,
    end: usize,
    quote: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RawContext {
    Style,
    Script,
    Generic,
}
