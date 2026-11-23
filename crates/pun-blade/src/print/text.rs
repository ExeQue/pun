//! `print/text.ts`, `echo.ts`, `echo-normalization.ts` and `comment.ts`.

use super::Printer;
use super::utils::{
    dedent_string, html_trim_preserve_indentation, html_whitespace_split,
    normalize_line_endings_to_lf, replace_eol,
};
use crate::ast::{Id, is_echo_like};
use crate::doc::{Doc, concat, fill, hardline, join, line, literalline};
use crate::lexer::TokenType;
use crate::tree::{NodeKind, js_trim};

impl Printer<'_> {
    pub(crate) fn print_text(&self, id: Id) -> Doc {
        let n = self.n(id);
        let text = &n.raw_text;
        if let Some(p) = n.parent
            && is_echo_like(self.kind(p))
        {
            // `/\n[^\S\n]*$/`
            let trimmed =
                text.trim_end_matches(|c: char| c != '\n' && crate::tree::is_js_whitespace(c));
            let has_trailing_newline = trimmed.ends_with('\n');
            let value = if has_trailing_newline {
                &trimmed[..trimmed.len() - 1]
            } else {
                text.as_str()
            };
            return concat(vec![
                concat(replace_eol(value, &literalline())),
                if has_trailing_newline {
                    hardline()
                } else {
                    Doc::default()
                },
            ]);
        }
        let prefix = self.opening_tag_prefix(id);
        let suffix = self.closing_tag_suffix(id);
        let mut parts = self.text_value_parts(id);
        if parts.is_empty() {
            return concat(vec![prefix, concat(parts), suffix]);
        }
        parts[0] = concat(vec![prefix, parts[0].clone()]);
        let last = parts.pop().unwrap_or_default();
        parts.push(concat(vec![last, suffix]));
        fill(parts)
    }

    fn text_value_parts(&self, id: Id) -> Vec<Doc> {
        let n = self.n(id);
        let value = &n.raw_text;
        if let Some(p) = n.parent
            && self.n(p).is_whitespace_sensitive
        {
            if self.n(p).is_indentation_sensitive {
                return replace_eol(value, &literalline());
            }
            return replace_eol(
                &dedent_string(&html_trim_preserve_indentation(value)),
                &hardline(),
            );
        }
        let words: Vec<Doc> = html_whitespace_split(value)
            .into_iter()
            .map(Doc::from)
            .collect();
        match join(&line(), words) {
            Doc::Array(v) => (*v).clone(),
            other => vec![other],
        }
    }

    pub(crate) fn print_comment(&self, id: Id) -> Doc {
        if self.n(id).is_ie_conditional_start_comment && self.kind(id) == NodeKind::Comment {
            return concat(vec![self.opening_tag_start(id), self.closing_tag_end(id)]);
        }
        concat(vec![
            self.opening_tag_prefix(id),
            super::utils::replace_eol_literal(self.full_text(id)),
            self.closing_tag_suffix(id),
        ])
    }

    fn echo_delimiters(&self, id: Id) -> (&'static str, &'static str) {
        match self.kind(id) {
            NodeKind::RawEcho => ("{!!", "!!}"),
            NodeKind::TripleEcho => ("{{{", "}}}"),
            _ => ("{{", "}}"),
        }
    }

    /// `printEcho` with `bladeEchoSpacing: "space"`.
    pub(crate) fn print_echo(&self, id: Id) -> Doc {
        let parts: Vec<&str> = self
            .tokens_of(id)
            .iter()
            .filter(|t| t.kind == TokenType::EchoContent)
            .map(|t| self.tok_text(t))
            .collect();
        if parts.is_empty() {
            return Doc::from(self.full_text(id));
        }
        let content = parts.concat();
        let trimmed = js_trim(&content);
        let (open, close) = self.echo_delimiters(id);
        if trimmed.is_empty() {
            return Doc::from(format!("{open} {close}"));
        }
        if trimmed.contains('\n') {
            let raw = format!("{open}\n{trimmed}\n{close}");
            if self.should_normalize_multiline_echo_indent(id) {
                return concat(replace_eol(
                    &self.normalize_multiline_echo_indent_text(id, &raw),
                    &hardline(),
                ));
            }
            return Doc::from(raw);
        }
        Doc::from(format!("{open} {trimmed} {close}"))
    }

    fn should_normalize_multiline_echo_indent(&self, id: Id) -> bool {
        let raw = self.full_text(id);
        self.n(id).parent.is_some_and(|p| self.is_script_like(p))
            && (raw.contains('\n') || raw.contains('\r'))
    }

    pub(crate) fn normalize_multiline_echo_indent_text(&self, id: Id, value: &str) -> String {
        if !value.contains('\n') && !value.contains('\r') {
            return value.to_owned();
        }
        let (open, close) = self.echo_delimiters(id);
        let normalized = normalize_line_endings_to_lf(value);
        let normalized = js_trim(&normalized);
        if !normalized.starts_with(open)
            || !normalized.ends_with(close)
            || normalized.len() < open.len() + close.len()
        {
            return value.to_owned();
        }
        let mut inner = &normalized[open.len()..normalized.len() - close.len()];
        inner = inner.strip_prefix('\n').unwrap_or(inner);
        inner = inner.strip_suffix('\n').unwrap_or(inner);
        // `.replace(/\n[^\S\r\n]*$/u, "")`
        let t = inner
            .trim_end_matches(|c: char| c != '\n' && c != '\r' && crate::tree::is_js_whitespace(c));
        let inner = t.strip_suffix('\n').unwrap_or(inner);
        if js_trim(inner).is_empty() {
            return format!("{open}\n{close}");
        }
        let unit = " ".repeat(super::utils::TAB_WIDTH);
        let body: Vec<String> = dedent_string(inner)
            .split('\n')
            .map(|l| {
                if js_trim(l).is_empty() {
                    String::new()
                } else {
                    format!("{unit}{l}")
                }
            })
            .collect();
        format!("{open}\n{}\n{close}", body.join("\n"))
    }
}
