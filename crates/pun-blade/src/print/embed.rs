//! `print/embed.ts`, `embed/attribute.ts` and `blade-syntax.ts`: the nodes Prettier formats
//! with another language before the main print.

use super::Printer;
use super::oxc::{self, JsKind};
use super::utils::RawContext;
use crate::ast::Id;
use crate::doc::{Doc, concat, group, if_break, indent, join, line, map_doc, softline};
use crate::frontend::is_frontend_event_style_at_name;
use crate::tree::{NodeKind, js_trim};

/// `max(160, printWidth * 2)` and `max(200, printWidth * 8)`.
const INLINE_INTENT_MIN_LENGTH: usize = 240;
const INLINE_INTENT_SUBFORMAT_WIDTH: usize = 960;

#[derive(Clone, Copy)]
enum AlpineKind {
    Expression,
    Statement,
}

const ALPINE_DIRECTIVES: &[(&str, AlpineKind)] = &[
    ("x-data", AlpineKind::Expression),
    ("x-model", AlpineKind::Expression),
    ("x-modelable", AlpineKind::Expression),
    ("x-show", AlpineKind::Expression),
    ("x-if", AlpineKind::Expression),
    ("x-for", AlpineKind::Expression),
    ("x-text", AlpineKind::Expression),
    ("x-html", AlpineKind::Expression),
    ("x-effect", AlpineKind::Expression),
    ("x-id", AlpineKind::Expression),
    ("x-teleport", AlpineKind::Expression),
    ("x-init", AlpineKind::Statement),
    ("x-intersect", AlpineKind::Expression),
    ("x-trap", AlpineKind::Expression),
    ("x-mask", AlpineKind::Expression),
    ("x-sort", AlpineKind::Expression),
    ("x-anchor", AlpineKind::Expression),
    ("x-persist", AlpineKind::Expression),
    ("x-resize", AlpineKind::Statement),
];

const CSS_AT_RULE_NAMES: &[&str] = &[
    "charset", "import", "namespace", "media", "supports", "layer", "container", "scope", "font-face",
    "font-feature-values", "font-palette-values", "property", "counter-style", "keyframes", "-webkit-keyframes",
    "-moz-keyframes", "-o-keyframes", "page", "starting-style", "view-transition", "document", "custom-media",
    "tailwind", "apply", "screen", "responsive", "variants", "utility", "theme", "plugin", "config", "use", "forward",
    "mixin", "include", "function", "return", "if", "else", "for", "each", "while", "at-root", "extend", "debug",
    "warn", "error",
];

fn extract_style_at_rule_name_at(source: &str, pos: usize) -> Option<String> {
    let b = source.as_bytes();
    if pos >= b.len() || b[pos] != b'@' {
        return None;
    }
    let start = pos + 1;
    let mut i = start;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'-' || b[i] == b'_') {
        i += 1;
    }
    (i > start).then(|| source[start..i].to_lowercase())
}

fn extract_directive_name(text: &str) -> Option<String> {
    let t = crate::tree::js_trim_start(text);
    let b = t.as_bytes();
    if b.first() != Some(&b'@') || b.len() < 2 || !(b[1].is_ascii_alphabetic() || b[1] == b'_') {
        return None;
    }
    let mut i = 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    Some(t[1..i].to_lowercase())
}

/// `@name\s*(` anywhere.
fn has_directive_call_args(text: &str) -> bool {
    directive_calls(text).any(|rest| rest.starts_with('('))
}

/// For each `@[A-Za-z_][A-Za-z0-9_]*` in `text`, what follows it after optional whitespace.
fn directive_calls(text: &str) -> impl Iterator<Item = &str> {
    let b = text.as_bytes();
    (0..b.len()).filter_map(move |i| {
        if b[i] != b'@' || i + 1 >= b.len() || !(b[i + 1].is_ascii_alphabetic() || b[i + 1] == b'_') {
            return None;
        }
        let mut j = i + 1;
        while j < b.len() && (b[j].is_ascii_alphanumeric() || b[j] == b'_') {
            j += 1;
        }
        Some(crate::tree::js_trim_start(&text[j..]))
    })
}

fn has_php_like_markers(text: &str) -> bool {
    // `/@[A-Za-z_][A-Za-z0-9_]*\s*\(\s*['"`[]/u`
    let call_with_literal = directive_calls(text).any(|rest| {
        rest.strip_prefix('(').is_some_and(|r| crate::tree::js_trim_start(r).starts_with(['\'', '"', '`', '[']))
    });
    let mut named_args = false;
    if let (Some(open), Some(close)) = (text.find('('), text.rfind(')'))
        && close > open
    {
        // `/(^|,)\s*[A-Za-z_][A-Za-z0-9_]*\s*:/u`
        let args = &text[open + 1..close];
        named_args = std::iter::once(0).chain(args.match_indices(',').map(|(i, _)| i + 1)).any(|start| {
            let s = crate::tree::js_trim_start(&args[start..]);
            let b = s.as_bytes();
            if b.is_empty() || !(b[0].is_ascii_alphabetic() || b[0] == b'_') {
                return false;
            }
            let len = b.iter().take_while(|c| c.is_ascii_alphanumeric() || **c == b'_').count();
            crate::tree::js_trim_start(&s[len..]).starts_with(':')
        });
    }
    text.contains('$')
        || text.contains("->")
        || text.contains("::")
        || text.contains("=>")
        || call_with_literal
        || named_args
}

impl Printer<'_> {
    fn is_directive_node_blade_like(&self, id: Id, context: RawContext) -> bool {
        let text = self.full_text(id);
        let Some(name) = extract_directive_name(text) else { return false };
        if context == RawContext::Style {
            if let Some(rule) = extract_style_at_rule_name_at(self.source(), self.n(id).start)
                && CSS_AT_RULE_NAMES.contains(&rule.as_str())
            {
                if !self.ast.directives.is_directive(&name) {
                    return false;
                }
                return rule == name && (has_php_like_markers(text) || has_directive_call_args(text));
            }
            if is_frontend_event_style_at_name(&name) {
                return has_php_like_markers(text);
            }
        } else if is_frontend_event_style_at_name(&name) {
            return has_php_like_markers(text);
        }
        if self.ast.directives.is_directive(&name) {
            return true;
        }
        has_php_like_markers(text)
    }

    pub(crate) fn is_blade_construct_child(&self, id: Id, context: RawContext) -> bool {
        match self.kind(id) {
            NodeKind::Echo
            | NodeKind::RawEcho
            | NodeKind::TripleEcho
            | NodeKind::PhpTag
            | NodeKind::PhpBlock
            | NodeKind::DirectiveBlock
            | NodeKind::IgnoreRange => true,
            NodeKind::Directive => self.is_directive_node_blade_like(id, context),
            _ => false,
        }
    }

    fn element_content(&self, id: Id) -> &str {
        let n = self.n(id);
        if n.open_tag_end_offset > 0 && n.closing_tag_start_offset > n.open_tag_end_offset {
            return &self.source()[n.open_tag_end_offset..n.closing_tag_start_offset];
        }
        let (Some(f), Some(l)) = (self.first_child(id), self.last_child(id)) else { return "" };
        let (s, e) = (self.n(f).start, self.n(l).end);
        if e <= s { "" } else { &self.source()[s..e] }
    }

    pub(crate) fn parent_contains_blade_syntax(&self, id: Id, context: RawContext) -> bool {
        if self.kind(id) != NodeKind::Element {
            return false;
        }
        if self.n(id).children.iter().any(|&c| self.is_blade_construct_child(c, context)) {
            return true;
        }
        let content = self.element_content(id);
        content.contains("{{") || content.contains("{!!") || content.contains("<?php")
    }

    /// `printEmbeddedLanguages`: a post-order walk (children, then attributes, then the node)
    /// that records the embedded docs used in place of the normal print.
    pub(crate) fn compute_embeds(&self, id: Id) {
        let n = self.n(id);
        for &c in &n.children.clone() {
            self.compute_embeds(c);
        }
        for &a in &n.attrs.clone() {
            self.compute_embeds(a);
        }
        if let Some(d) = self.embed(id) {
            self.embeds.borrow_mut().insert(id, d);
        }
    }

    fn embed(&self, id: Id) -> Option<Doc> {
        match self.kind(id) {
            NodeKind::Attribute | NodeKind::JsxAttribute => self.embed_attribute(id),
            NodeKind::Element => self.embed_element(id),
            NodeKind::Text => self.embed_script_text(id),
            NodeKind::FrontMatter => self.embed_front_matter(),
            _ => None,
        }
    }

    /// Prettier's front matter `print`: YAML is formatted, other languages print as written.
    fn embed_front_matter(&self) -> Option<Doc> {
        let fm = self.ast.front_matter.as_ref()?;
        if fm.language != "yaml" {
            return None;
        }
        let value = js_trim(&fm.value);
        let doc = if value.is_empty() { Doc::default() } else { oxc::yaml_to_doc(&self.ids, value)? };
        let has_doc = !value.is_empty();
        Some(crate::doc::mark_as_root(concat(vec![
            Doc::from(fm.start_delimiter.as_str()),
            Doc::from(fm.explicit_language.clone().unwrap_or_default()),
            crate::doc::hardline(),
            doc,
            if has_doc { crate::doc::hardline() } else { Doc::default() },
            Doc::from(fm.end_delimiter.as_str()),
        ])))
    }

    /// `handleElementEmbed`
    fn embed_element(&self, id: Id) -> Option<Doc> {
        if self.should_use_inline_intent_element_embed(id) {
            let delegated = super::format_with(self.full_text(id), INLINE_INTENT_SUBFORMAT_WIDTH, true);
            let trimmed = delegated.strip_suffix('\n').unwrap_or(&delegated);
            return Some(concat(super::utils::replace_eol(trimmed, &crate::doc::hardline())));
        }
        if !self.is_script_like(id) {
            return None;
        }
        if self.should_use_unparsed_raw_content_embedding(id) {
            return Some(self.embed_unparsed_raw_content_element(id));
        }
        if self.should_use_mixed_raw_content_embedding(id) {
            return Some(self.embed_mixed_raw_content_element(id).unwrap_or_else(|| Doc::from(self.full_text(id))));
        }
        let n = self.n(id);
        if self.infer_element_parser(id).is_some()
            && n.tag_name == "style"
            && n.children.iter().any(|&c| self.kind(c) != NodeKind::Text)
        {
            if n.children.iter().any(|&c| self.is_blade_construct_child(c, RawContext::Style)) {
                return None;
            }
            return self.embed_style_element_children(id);
        }
        None
    }

    /// `shouldUseInlineIntentElementEmbed`
    fn should_use_inline_intent_element_embed(&self, id: Id) -> bool {
        let n = self.n(id);
        if !n.has_closing_tag || self.inline_intent_delegated || !matches!(n.tag_name.as_str(), "p" | "svg") {
            return false;
        }
        if !super::utils::preserve_inline_intent_element(&n.tag_name, &n.namespace) {
            return false;
        }
        let source = crate::tree::js_trim_end(self.full_text(id));
        source.encode_utf16().count() >= INLINE_INTENT_MIN_LENGTH && !source.contains('\n') && !source.contains('\r')
    }

    fn has_blade_value_syntax(&self, id: Id) -> bool {
        let text = self.full_text(id);
        text.contains("{{") || text.contains("{!!")
    }

    fn embed_attribute(&self, id: Id) -> Option<Doc> {
        let value = self.unescaped_attribute_value(id);
        if value.is_empty() {
            return None;
        }
        let name = self.static_attribute_name_lower(id);
        let static_value = self.is_static_attribute_value(id);
        let parent_full = self.n(id).parent.map(|p| self.n(p).full_name.to_lowercase()).unwrap_or_default();

        let mut single_quote_wrapper = false;
        let printed: Doc = if name.as_deref() == Some("srcset") && static_value && (parent_full == "img" || parent_full == "source") {
            print_srcset(&value)?
        } else if static_value && name.as_deref() == Some("style") && !self.has_blade_value_syntax(id) {
            if self.should_preserve_inline_svg_style_attribute(id) {
                Doc::from(value.as_str())
            } else {
                print_expand(oxc::css_to_doc(&self.ids, &value, true)?, true)
            }
        } else if name.as_deref().is_some_and(crate::frontend::is_html_event_attribute)
            && static_value
            && !self.has_blade_value_syntax(id)
        {
            oxc::js_to_doc(&self.ids, &value, JsKind::Handler)?
        } else if self.is_colon_bound_attribute(id) {
            if self.n(id).parent.is_some_and(|p| {
                self.kind(p) == NodeKind::Element && super::utils::is_blade_component_tag_name(&self.n(p).full_name)
            }) {
                return None;
            }
            single_quote_wrapper = true;
            let doc = oxc::js_to_doc(&self.ids, &value, JsKind::GroupExpression)
                .or_else(|| oxc::js_to_doc(&self.ids, &value, JsKind::HandlerGrouped))?;
            group(doc)
        } else if static_value && matches!(name.as_deref(), Some("class" | "classname")) && !self.has_blade_value_syntax(id) {
            Doc::from(js_trim(&value).split(crate::tree::is_js_whitespace).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "))
        } else if static_value && name.as_deref() == Some("allow") && parent_full == "iframe" && !self.has_blade_value_syntax(id) {
            print_permissions_policy(&value)
        } else if let Some(kind) = self.alpine_directive_kind(id, &value) {
            let (first, second) = match kind {
                AlpineKind::Statement => (JsKind::Handler, JsKind::HugExpression),
                AlpineKind::Expression => (JsKind::HugExpression, JsKind::Handler),
            };
            oxc::js_to_doc(&self.ids, &value, first)
                .or_else(|| oxc::js_to_doc(&self.ids, &value, second))
                .unwrap_or_else(|| Doc::from(normalize_multiline_fallback_value_indentation(&value)))
        } else {
            return None;
        };
        Some(self.wrap_attribute_value(id, printed, single_quote_wrapper))
    }

    /// `isAlpineAttribute` and `getAlpineDirectiveInfo`.
    fn alpine_directive_kind(&self, id: Id, value: &str) -> Option<AlpineKind> {
        if value.contains("{{") || value.contains("{!!") || !self.is_static_attribute_value(id) || !self.is_static_attribute_name(id) {
            return None;
        }
        let name = self.attribute_name_raw(id);
        if name.is_empty() {
            return None;
        }
        let lower = name.to_lowercase();
        if lower.starts_with('@') || lower.starts_with("x-on:") {
            return Some(AlpineKind::Statement);
        }
        if lower.starts_with(':') || lower.starts_with("x-bind:") {
            return Some(AlpineKind::Expression);
        }
        ALPINE_DIRECTIVES.iter().find_map(|&(directive, kind)| {
            let rest = lower.strip_prefix(directive)?;
            (rest.is_empty() || rest.starts_with('.') || rest.starts_with(':')).then_some(kind)
        })
    }

    /// `shouldPreserveInlineSvgStyleAttribute`
    fn should_preserve_inline_svg_style_attribute(&self, id: Id) -> bool {
        let Some(parent) = self.n(id).parent else { return false };
        let p = self.n(parent);
        if self.kind(parent) != NodeKind::Element || p.namespace != "svg" {
            return false;
        }
        if !super::utils::preserve_inline_intent_attributes(&p.tag_name, &p.namespace) {
            return false;
        }
        let multiline = |t: &str| t.contains('\n') || t.contains('\r');
        !multiline(self.full_text(parent)) && !multiline(self.full_text(id))
    }

    fn is_colon_bound_attribute(&self, id: Id) -> bool {
        if !self.is_static_attribute_name(id) || !self.is_static_attribute_value(id) || self.has_blade_value_syntax(id) {
            return false;
        }
        let name = self.attribute_name_raw(id);
        let t = crate::tree::js_trim_start(&name);
        !name.is_empty() && (t.starts_with(':') || t.to_lowercase().starts_with("v-bind:"))
    }

    /// `createAttributePrinter`: `name="value"` with `"` escaped inside the value.
    fn wrap_attribute_value(&self, id: Id, value: Doc, allow_single_quote_wrapper: bool) -> Doc {
        let raw_name = self.format_attribute_name_for_print(id);
        let name = if self.is_static_attribute_name(id) {
            self.normalize_attribute_name(&raw_name, self.n(id).parent)
        } else {
            raw_name
        };
        if allow_single_quote_wrapper {
            let content = collect_doc_strings(&value);
            if content.contains('"') && !content.contains('\'') {
                return concat(vec![Doc::from(name), Doc::from("='"), group(value), Doc::from("'")]);
            }
        }
        let escaped = map_doc(&value, &mut |d| match &d {
            Doc::Str(s) if s.contains('"') => Doc::from(s.replace('"', "&quot;")),
            _ => d,
        });
        concat(vec![Doc::from(name), Doc::from("=\""), group(escaped), Doc::from("\"")])
    }
}

fn collect_doc_strings(doc: &Doc) -> String {
    let mut out = String::new();
    map_doc(doc, &mut |d| {
        if let Doc::Str(s) = &d {
            out.push_str(s);
        }
        d
    });
    out
}

/// `printExpand`
pub(crate) fn print_expand(content: Doc, trailing: bool) -> Doc {
    concat(vec![indent(concat(vec![softline(), content])), if trailing { softline() } else { Doc::default() }])
}

fn print_permissions_policy(value: &str) -> Doc {
    let directives: Vec<Vec<&str>> = value
        .split(';')
        .map(js_trim)
        .filter(|s| !s.is_empty())
        .map(|d| d.split(crate::tree::is_js_whitespace).filter(|s| !s.is_empty()).collect())
        .collect();
    if directives.is_empty() {
        return concat(vec![Doc::default()]);
    }
    let count = directives.len();
    let parts: Vec<Doc> = directives
        .into_iter()
        .enumerate()
        .map(|(i, words)| {
            concat(vec![
                Doc::from(words.join(" ")),
                if i == count - 1 { if_break(";", "") } else { concat(vec![Doc::from(";"), line()]) },
            ])
        })
        .collect();
    let _ = join;
    print_expand(concat(parts), true)
}

fn line_indent(line: &str) -> usize {
    line.bytes().take_while(|&b| b == b' ' || b == b'\t').count()
}

/// `normalizeMultilineFallbackValueIndentation`
fn normalize_multiline_fallback_value_indentation(value: &str) -> String {
    if !value.contains('\n') {
        return value.to_owned();
    }
    let normalized = value.replace("\r\n", "\n");
    let mut lines: Vec<&str> = normalized.split('\n').collect();
    let non_empty: Vec<usize> = (0..lines.len()).filter(|&i| !js_trim(lines[i]).is_empty()).collect();
    let (Some(&first), Some(&last)) = (non_empty.first(), non_empty.last()) else { return value.to_owned() };
    if first >= last {
        return value.to_owned();
    }
    let open = js_trim(lines[first]).as_bytes()[0];
    let close = js_trim(lines[last]).as_bytes()[0];
    if !matches!((open, close), (b'[', b']') | (b'{', b'}') | (b'(', b')')) {
        return value.to_owned();
    }
    let close_indent = line_indent(lines[last]);
    let Some(min_middle) = (first + 1..last).filter(|&i| !js_trim(lines[i]).is_empty()).map(|i| line_indent(lines[i])).min()
    else {
        return value.to_owned();
    };
    let target = close_indent + 2;
    if min_middle <= target {
        return value.to_owned();
    }
    let shift = min_middle - target;
    for line in lines.iter_mut().take(last).skip(first + 1) {
        if !js_trim(line).is_empty() {
            *line = &line[shift.min(line.len())..];
        }
    }
    lines.join("\n")
}

/// `printSrcset`, with `@prettier/parse-srcset`.
fn print_srcset(value: &str) -> Option<Doc> {
    let candidates = super::srcset::parse_srcset(value)?;
    if candidates.is_empty() {
        return Some(Doc::from(value));
    }
    let units = [('w', 0usize), ('h', 1), ('x', 2)];
    let used: Vec<char> = units.iter().filter(|(_, i)| candidates.iter().any(|c| c.descriptors[*i].is_some())).map(|(u, _)| *u).collect();
    if used.len() > 1 {
        return None;
    }
    let key = used.first().copied();
    let key_index = key.and_then(|k| units.iter().find(|(u, _)| *u == k).map(|(_, i)| *i));
    let max_url = candidates.iter().map(|c| c.url.chars().count()).max().unwrap_or(0);
    let descriptors: Vec<String> =
        candidates.iter().map(|c| key_index.and_then(|i| c.descriptors[i].clone()).unwrap_or_default()).collect();
    let left: Vec<usize> = descriptors.iter().map(|d| d.find('.').unwrap_or(d.len())).collect();
    let max_left = left.iter().copied().max().unwrap_or(0);
    let parts: Vec<Doc> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut parts = vec![Doc::from(c.url.as_str())];
            if !descriptors[i].is_empty() {
                let pad = max_url - c.url.chars().count() + 1 + max_left - left[i];
                parts.push(if_break(" ".repeat(pad).as_str(), " "));
                parts.push(Doc::from(format!("{}{}", descriptors[i], key.map(String::from).unwrap_or_default())));
            }
            concat(parts)
        })
        .collect();
    Some(print_expand(join(&concat(vec![Doc::from(","), line()]), parts), true))
}
