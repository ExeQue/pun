//! `print/embed.ts`, `embed/attribute.ts` and `blade-syntax.ts`: the nodes Prettier formats
//! with another language before the main print.

use super::Printer;
use super::utils::RawContext;
use crate::ast::Id;
use crate::doc::{Doc, concat, group, if_break, indent, join, line, map_doc, softline};
use crate::frontend::is_frontend_event_style_at_name;
use crate::tree::{NodeKind, js_trim};

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
            _ => None,
        }
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

        let printed: Doc = if name.as_deref() == Some("srcset") && static_value && (parent_full == "img" || parent_full == "source") {
            // TODO: srcset printing.
            return None;
        } else if static_value && name.as_deref() == Some("style") && !self.has_blade_value_syntax(id) {
            // TODO: embedded CSS.
            return None;
        } else if name.as_deref().is_some_and(crate::frontend::is_html_event_attribute)
            && static_value
            && !self.has_blade_value_syntax(id)
        {
            // TODO: embedded JS (event handlers).
            return None;
        } else if self.is_colon_bound_attribute(id) {
            // TODO: embedded JS expressions.
            return None;
        } else if static_value && matches!(name.as_deref(), Some("class" | "classname")) && !self.has_blade_value_syntax(id) {
            Doc::from(js_trim(&value).split(crate::tree::is_js_whitespace).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" "))
        } else if static_value && name.as_deref() == Some("allow") && parent_full == "iframe" && !self.has_blade_value_syntax(id) {
            print_permissions_policy(&value)
        } else {
            // TODO: Alpine attributes.
            return None;
        };
        Some(self.wrap_attribute_value(id, printed, false))
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
