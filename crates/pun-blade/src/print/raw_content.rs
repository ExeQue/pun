//! `print/embed/raw-content.ts` and `rawtext-script-scanner.ts`: `<script>` and `<style>` bodies
//! that hold Blade. The Blade parts are masked with placeholders, the body is formatted in its
//! language, the placeholders are put back, and the result is normalized as text.

use super::Printer;
use super::oxc::{self, Lang};
use super::utils::{
    RawContext, TAB_WIDTH, dedent_string, normalize_line_endings_to_lf, replace_eol,
    trim_trailing_horizontal_whitespace,
};
use crate::ast::Id;
use crate::directives::Role;
use crate::doc::{Doc, concat, group, hardline, indent};
use crate::lexer::TokenType;
use crate::tree::{NodeKind, is_js_whitespace, js_trim, js_trim_end, js_trim_start};

const EXPRESSION_DIRECTIVES: &[&str] = &["json", "js", "entangle", "this"];
const STYLE_OPENERS: &[&str] = &[
    "if", "unless", "isset", "empty", "for", "foreach", "forelse", "while", "switch", "php",
    "verbatim",
];
const STYLE_CLOSERS: &[&str] = &[
    "endif",
    "endunless",
    "endisset",
    "endempty",
    "endfor",
    "endforeach",
    "endforelse",
    "endwhile",
    "endswitch",
    "endphp",
    "endverbatim",
];
const STYLE_BRANCHES: &[&str] = &["else", "elseif", "empty", "case", "default"];
const STYLE_RAW_FALLBACK_NAMES: &[&str] = &[
    "elseif",
    "endif",
    "endunless",
    "endisset",
    "endempty",
    "endfor",
    "endforeach",
    "endforelse",
    "endwhile",
    "endswitch",
    "endphp",
    "endverbatim",
    "foreach",
    "forelse",
    "case",
    "default",
];
const LARGE_STYLE_EMBED_CHAR_THRESHOLD: usize = 20_000;
const LARGE_STYLE_EMBED_LINE_THRESHOLD: usize = 300;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Expr,
    Stmt,
}

struct PlaceholderSlot {
    node: Id,
    marker: String,
    replacement: String,
}

struct Candidate {
    node: Id,
    start: usize,
    end: usize,
    kind: Slot,
    in_value: bool,
    replacement: String,
}

fn is_ws_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_'
}

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn is_structural(name: &str) -> bool {
    STYLE_OPENERS.contains(&name) || STYLE_BRANCHES.contains(&name) || STYLE_CLOSERS.contains(&name)
}

/// `^\s*`
fn leading_ws(line: &str) -> &str {
    &line[..line.len() - js_trim_start(line).len()]
}

/// `^[ \t]*` length
fn indent_len(line: &str) -> usize {
    line.bytes()
        .take_while(|&b| b == b' ' || b == b'\t')
        .count()
}

/// `split(/\r?\n/u)`
fn split_lines(value: &str) -> Vec<String> {
    value
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_owned())
        .collect()
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

fn to_base36(mut n: usize) -> String {
    if n == 0 {
        return "0".into();
    }
    let mut out = Vec::new();
    while n > 0 {
        let d = (n % 36) as u8;
        out.push(if d < 10 { b'0' + d } else { b'a' + d - 10 });
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// The end of `@[A-Za-z_][A-Za-z0-9_]*(?:\s*\([^)]*\))?` at `i`, or `None`.
fn directive_match_end(b: &[u8], i: usize) -> Option<(usize, usize)> {
    if b.get(i) != Some(&b'@') || !b.get(i + 1).is_some_and(|&c| is_ident_start(c)) {
        return None;
    }
    let mut j = i + 1;
    while j < b.len() && is_ident(b[j]) {
        j += 1;
    }
    let name_end = j;
    let mut k = j;
    while k < b.len() && b[k].is_ascii_whitespace() {
        k += 1;
    }
    if b.get(k) == Some(&b'(')
        && let Some(close) = b[k + 1..].iter().position(|&c| c == b')')
    {
        return Some((name_end, k + 1 + close + 1));
    }
    Some((name_end, name_end))
}

/// `trimmedLine.match(/^@([A-Za-z_][A-Za-z0-9_]*)/u)?.[1].toLowerCase()`
fn directive_name_from_line(trimmed: &str) -> Option<String> {
    let b = trimmed.as_bytes();
    let (name_end, _) = directive_match_end(b, 0)?;
    Some(trimmed[1..name_end].to_lowercase())
}

/// `trimmed.match(/^@[A-Za-z_][A-Za-z0-9_]*(?:\s*\([^)]*\))?/u)?.[0]`
fn directive_prefix(trimmed: &str) -> Option<&str> {
    let (_, end) = directive_match_end(trimmed.as_bytes(), 0)?;
    Some(&trimmed[..end])
}

fn is_style_comment_line(t: &str) -> bool {
    (t.starts_with("/*") && t.ends_with("*/")) || t.starts_with("//")
}

/// `/^\/\*\s*@/u` or `/^\/\/\s*@/u`
fn is_directive_comment(t: &str) -> bool {
    let rest = t.strip_prefix("/*").or_else(|| t.strip_prefix("//"));
    rest.is_some_and(|r| js_trim_start(r).starts_with('@'))
}

/// `/^([.#:[*]|[A-Za-z_]).*\{/u` (or with `\{\s*$` when `at_end`).
fn is_selector_like(t: &str, at_end: bool) -> bool {
    let Some(&first) = t.as_bytes().first() else {
        return false;
    };
    if !(matches!(first, b'.' | b'#' | b':' | b'[' | b'*') || is_ident_start(first)) {
        return false;
    }
    let first_line = t.split('\n').next().unwrap_or("");
    if at_end {
        js_trim_end(t).ends_with('{') && !t.contains('\n')
    } else {
        first_line[1..].contains('{')
    }
}

fn normalize_directive_line_arg_spacing(line: &str) -> String {
    let b = line.as_bytes();
    let ind = indent_len(line);
    let Some((name_end, _)) = directive_match_end(b, ind) else {
        return line.to_owned();
    };
    let mut k = name_end;
    while k < b.len() && is_js_whitespace(b[k] as char) && b[k].is_ascii() {
        k += 1;
    }
    if b.get(k) != Some(&b'(') {
        return line.to_owned();
    }
    let name = line[ind + 1..name_end].to_lowercase();
    if !is_structural(&name) && !EXPRESSION_DIRECTIVES.contains(&name.as_str()) {
        return line.to_owned();
    }
    format!(
        "{}@{} ({}",
        &line[..ind],
        &line[ind + 1..name_end],
        &line[k + 1..]
    )
}

fn is_blade_value_line(t: &str) -> bool {
    t.starts_with('@') || t.starts_with("{{") || t.starts_with("{!!") || t.starts_with("<?php")
}

/// `splitStyleBladeValueChunks`
fn split_style_blade_value_chunks(line: &str) -> Vec<String> {
    let b = line.as_bytes();
    let mut chunks = Vec::new();
    let mut cursor = 0;
    let mut i = 0;
    let ends_ok = |e: usize| e >= b.len() || b[e].is_ascii_whitespace();
    while i < b.len() {
        let at = if i == 0 && b[0] == b'@' {
            Some(0)
        } else if b[i].is_ascii_whitespace() && b.get(i + 1) == Some(&b'@') {
            Some(i + 1)
        } else {
            None
        };
        let matched = at.and_then(|at| {
            let (name_end, full_end) = directive_match_end(b, at)?;
            if ends_ok(full_end) {
                Some((at, full_end))
            } else if ends_ok(name_end) {
                Some((at, name_end))
            } else {
                None
            }
        });
        if let Some((start, end)) = matched {
            let before = js_trim(&line[cursor..start]);
            if !before.is_empty() {
                chunks.push(before.to_owned());
            }
            chunks.push(line[start..end].to_owned());
            cursor = end;
            i = end;
        } else {
            i += 1;
        }
    }
    let tail = js_trim(&line[cursor..]);
    if !tail.is_empty() {
        chunks.push(tail.to_owned());
    }
    if chunks.is_empty() && !line.is_empty() {
        chunks.push(line.to_owned());
    }
    chunks
}

fn normalize_style_blade_value_indentation(value: &str) -> String {
    let mut lines = split_lines(value);
    let mut i = 0;
    while i < lines.len() {
        let current = lines[i].clone();
        if !js_trim_end(&current).ends_with(':') {
            i += 1;
            continue;
        }
        let base = leading_ws(&current).to_owned();
        let first = i + 1;
        if first >= lines.len() {
            i += 1;
            continue;
        }
        let mut semicolon: Option<usize> = None;
        let mut saw_blade = false;
        let mut chunks: Vec<String> = Vec::new();
        for (j, line) in lines.iter().enumerate().skip(first) {
            let trimmed = js_trim(line);
            if trimmed.is_empty() {
                continue;
            }
            let mut body = trimmed;
            let mut trailing = false;
            if body == ";" {
                semicolon = Some(j);
                break;
            }
            if let Some(b) = body.strip_suffix(';') {
                trailing = true;
                semicolon = Some(j);
                body = js_trim_end(b);
            }
            if !body.is_empty() {
                let c = split_style_blade_value_chunks(body);
                if c.iter().any(|c| is_blade_value_line(c)) {
                    saw_blade = true;
                }
                chunks.extend(c);
            }
            if trailing {
                break;
            }
        }
        let Some(semi) = semicolon.filter(|_| saw_blade && !chunks.is_empty()) else {
            i += 1;
            continue;
        };
        let mut depth: usize = 0;
        let mut replacement = Vec::new();
        for chunk in &chunks {
            let t = js_trim(chunk);
            if t.is_empty() {
                continue;
            }
            let name = directive_name_from_line(t);
            if name
                .as_deref()
                .is_some_and(|n| STYLE_CLOSERS.contains(&n) || STYLE_BRANCHES.contains(&n))
            {
                depth = depth.saturating_sub(1);
            }
            replacement.push(format!("{base}  {}{t}", "  ".repeat(depth)));
            if name
                .as_deref()
                .is_some_and(|n| STYLE_OPENERS.contains(&n) || STYLE_BRANCHES.contains(&n))
            {
                depth += 1;
            }
        }
        replacement.push(format!("{base}  ;"));
        let len = replacement.len();
        lines.splice(first..=semi, replacement);
        i = first + len;
    }
    lines.join("\n")
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scan {
    Code,
    Single,
    Double,
    LineComment,
    BlockComment,
}

/// `shouldTreatAsStyleLineComment`
fn treat_as_style_line_comment(b: &[u8], content_start: usize, slash: usize) -> bool {
    for i in (content_start..slash).rev() {
        match b[i] {
            b' ' | b'\t' | b'\r' => {}
            c => return matches!(c, b'\n' | b';' | b'{' | b'}'),
        }
    }
    true
}

/// A code/string/comment scanner over `value` that calls `on_code` for each byte in code
/// (returning `true` when it wrote that byte itself and may have moved `i`).
fn scan_style<F>(value: &str, mut on_code: F, mut on_string_newline: impl FnMut()) -> String
where
    F: FnMut(&[u8], &mut usize, &mut Vec<u8>) -> bool,
{
    let b = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut state = Scan::Code;
    let mut i = 0;
    while i < b.len() {
        let ch = b[i];
        let next = b.get(i + 1).copied().unwrap_or(0);
        match state {
            Scan::LineComment => {
                out.push(ch);
                if ch == b'\n' {
                    state = Scan::Code;
                    on_string_newline();
                }
            }
            Scan::BlockComment => {
                out.push(ch);
                if ch == b'\n' {
                    on_string_newline();
                }
                if ch == b'*' && next == b'/' {
                    out.push(b'/');
                    i += 1;
                    state = Scan::Code;
                }
            }
            Scan::Single | Scan::Double => {
                out.push(ch);
                if ch == b'\\' {
                    if i + 1 < b.len() {
                        out.push(b[i + 1]);
                        i += 1;
                    }
                } else if (state == Scan::Single && ch == b'\'')
                    || (state == Scan::Double && ch == b'"')
                {
                    state = Scan::Code;
                } else if ch == b'\n' {
                    on_string_newline();
                }
            }
            Scan::Code => {
                if ch == b'/' && next == b'*' {
                    out.extend_from_slice(b"/*");
                    i += 1;
                    state = Scan::BlockComment;
                } else if ch == b'/' && next == b'/' && treat_as_style_line_comment(b, 0, i) {
                    out.extend_from_slice(b"//");
                    i += 1;
                    state = Scan::LineComment;
                } else if !on_code(b, &mut i, &mut out) {
                    if ch == b'\'' {
                        state = Scan::Single;
                    } else if ch == b'"' {
                        state = Scan::Double;
                    }
                    out.push(ch);
                }
            }
        }
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_owned())
}

fn normalize_style_directive_boundaries(value: &str) -> String {
    scan_style(
        value,
        |b, i, out| {
            let ch = b[*i];
            let next = b.get(*i + 1).copied().unwrap_or(0);
            if ch == b'}' && (*i == 0 || b[*i - 1] != b'{') && next != b'}' {
                let mut j = *i + 1;
                while j < b.len() && is_ws_byte(b[j]) {
                    j += 1;
                }
                if j > *i + 1 && directive_match_end(b, j).is_some() {
                    out.extend_from_slice(b"}\n");
                    *i = j - 1;
                    return true;
                }
            }
            false
        },
        || {},
    )
}

/// `lineCode.match(/@([A-Za-z_][A-Za-z0-9_]*)(?:\s*\([^)]*\))?\s*$/u)?.[1]`
fn trailing_directive_name(line_code: &str) -> Option<String> {
    let b = line_code.as_bytes();
    for (i, _) in line_code.match_indices('@') {
        let Some((name_end, full_end)) = directive_match_end(b, i) else {
            continue;
        };
        let rest_ws = |e: usize| b[e..].iter().all(u8::is_ascii_whitespace);
        if rest_ws(full_end) || rest_ws(name_end) {
            return Some(line_code[i + 1..name_end].to_lowercase());
        }
    }
    None
}

fn normalize_style_structural_directive_semicolons(value: &str) -> String {
    let line_code = std::cell::RefCell::new(String::new());
    scan_style(
        value,
        |b, i, out| {
            let ch = b[*i];
            if ch == b'\'' || ch == b'"' {
                line_code.borrow_mut().push(ch as char);
                return false;
            }
            if ch == b';' {
                let lc = line_code.borrow().clone();
                if js_trim_start(&lc).starts_with('@')
                    && let Some(name) = trailing_directive_name(&lc)
                    && is_structural(&name)
                {
                    let mut j = *i + 1;
                    while j < b.len() && matches!(b[j], b' ' | b'\t' | b'\r') {
                        j += 1;
                    }
                    let n = b.get(j).copied();
                    let nn = b.get(j + 1).copied();
                    if n.is_none()
                        || n == Some(b'\n')
                        || (n == Some(b'/') && matches!(nn, Some(b'*' | b'/')))
                    {
                        return true;
                    }
                }
            }
            out.push(ch);
            if ch == b'\n' {
                line_code.borrow_mut().clear();
            } else {
                // Bytes of a multi-byte char land one by one; only ASCII is ever matched.
                line_code.borrow_mut().push(if ch.is_ascii() {
                    ch as char
                } else {
                    '\u{fffd}'
                });
            }
            true
        },
        || line_code.borrow_mut().clear(),
    )
}

/// The tokens of `/@[A-Za-z_][A-Za-z0-9_]*(?:\s*\([^)]*\))?|\/\*[^]*?\*\//gu` and the rest.
fn directive_comment_tokens(t: &str) -> (Vec<&str>, String) {
    let b = t.as_bytes();
    let mut tokens = Vec::new();
    let mut rest = String::new();
    let mut i = 0;
    let mut last = 0;
    while i < b.len() {
        let end = if let Some((_, e)) = directive_match_end(b, i) {
            Some(e)
        } else if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            t[i + 2..].find("*/").map(|p| i + 2 + p + 2)
        } else {
            None
        };
        if let Some(e) = end {
            rest.push_str(&t[last..i]);
            tokens.push(js_trim(&t[i..e]));
            i = e;
            last = e;
        } else {
            i += 1;
        }
    }
    rest.push_str(&t[last..]);
    (tokens, rest)
}

fn normalize_style_directive_comment_chains(value: &str) -> String {
    let mut out = Vec::new();
    for line in split_lines(value) {
        let trimmed = js_trim(&line);
        if trimmed.is_empty() || !trimmed.contains('@') {
            out.push(line);
            continue;
        }
        let ind = leading_ws(&line).to_owned();
        let (tokens, remainder) = directive_comment_tokens(trimmed);
        if tokens.len() <= 1
            || !js_trim(&remainder).is_empty()
            || !tokens.iter().any(|t| t.starts_with('@'))
        {
            out.push(line);
            continue;
        }
        if !tokens
            .iter()
            .all(|t| t.starts_with('@') || t.starts_with("/*"))
        {
            out.push(line);
            continue;
        }
        for token in tokens {
            if token.starts_with('@') {
                out.push(normalize_directive_line_arg_spacing(&format!(
                    "{ind}{token}"
                )));
            } else {
                out.push(format!("{ind}{token}"));
            }
        }
    }
    out.join("\n")
}

/// Leading `/* … */` comments of `s`: the comments and the offset after them; `None` for an
/// unclosed one.
fn leading_comments(s: &str, from: usize) -> (Vec<&str>, usize, bool) {
    let mut comments = Vec::new();
    let mut cursor = from;
    loop {
        let rest = js_trim_start(&s[cursor..]);
        if !rest.starts_with("/*") {
            return (comments, cursor, false);
        }
        let Some(end) = rest.find("*/") else {
            return (comments, cursor, true);
        };
        comments.push(&rest[..end + 2]);
        cursor = s.len() - rest.len() + end + 2;
    }
}

fn normalize_style_directive_comment_selector_lines(value: &str) -> String {
    let mut out = Vec::new();
    for line in split_lines(value) {
        let ind = leading_ws(&line).to_owned();
        let trimmed = js_trim(&line);
        let Some(prefix) = directive_prefix(trimmed) else {
            out.push(line);
            continue;
        };
        if !directive_name_from_line(prefix).is_some_and(|n| is_structural(&n)) {
            out.push(line);
            continue;
        }
        let (comments, cursor, _) = leading_comments(trimmed, prefix.len());
        let tail = js_trim_start(&trimmed[cursor..]);
        if comments.is_empty() || tail.is_empty() || !is_selector_like(tail, false) {
            out.push(line);
            continue;
        }
        out.push(normalize_directive_line_arg_spacing(&format!(
            "{ind}{prefix}"
        )));
        for c in comments {
            out.push(format!("{ind}{c}"));
        }
        out.push(format!("{ind}{tail}"));
    }
    out.join("\n")
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum ChainLine {
    Structural,
    DirectiveComment,
    GenericComment,
    Other,
}

fn classify_chain_line(line: &str) -> ChainLine {
    let t = js_trim(line);
    if t.is_empty() {
        return ChainLine::Other;
    }
    if directive_name_from_line(t).is_some_and(|n| is_structural(&n)) {
        return ChainLine::Structural;
    }
    if is_style_comment_line(t) {
        return if is_directive_comment(t) {
            ChainLine::DirectiveComment
        } else {
            ChainLine::GenericComment
        };
    }
    ChainLine::Other
}

fn normalize_style_directive_chain_blank_lines(value: &str) -> String {
    use ChainLine::{DirectiveComment as D, GenericComment as G, Structural as S};
    let lines = split_lines(value);
    let mut out: Vec<String> = Vec::new();
    for (i, current) in lines.iter().enumerate() {
        if !js_trim(current).is_empty() {
            out.push(current.clone());
            continue;
        }
        let prev = out.iter().rposition(|l| !js_trim(l).is_empty());
        let next = (i + 1..lines.len()).find(|&j| !js_trim(&lines[j]).is_empty());
        let (Some(p), Some(n)) = (prev, next) else {
            out.push(current.clone());
            continue;
        };
        let drop = matches!(
            (classify_chain_line(&out[p]), classify_chain_line(&lines[n])),
            (S | G | D, S) | (S | D, G) | (S | G, D)
        );
        if !drop {
            out.push(current.clone());
        }
    }
    out.join("\n")
}

fn normalize_style_blank_line_runs(value: &str) -> String {
    let mut out = Vec::new();
    let mut run = 0;
    for line in split_lines(value) {
        if js_trim(&line).is_empty() {
            run += 1;
            if run <= 1 {
                out.push(String::new());
            }
            continue;
        }
        run = 0;
        out.push(line);
    }
    out.join("\n")
}

fn normalize_style_standalone_semicolon_lines(value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in split_lines(value) {
        if js_trim(&line) != ";" {
            out.push(line);
            continue;
        }
        let Some(prev) = out.iter().rposition(|l| !js_trim(l).is_empty()) else {
            out.push(line);
            continue;
        };
        let structural =
            directive_name_from_line(js_trim(&out[prev])).is_some_and(|n| is_structural(&n));
        if structural {
            let mut in_value = false;
            for candidate in out[..prev].iter().rev() {
                let c = js_trim(candidate);
                if c.is_empty()
                    || directive_name_from_line(c).is_some()
                    || is_style_comment_line(c)
                    || is_blade_value_line(c)
                {
                    continue;
                }
                in_value = c.ends_with(':');
                break;
            }
            if in_value {
                out.push(line);
            }
            continue;
        }
        out.push(line);
    }
    out.join("\n")
}

fn repair_style_broken_comment_closers(value: &str) -> String {
    split_lines(value)
        .into_iter()
        .map(|line| {
            if !js_trim_start(&line).starts_with("/*") {
                return line;
            }
            let end = js_trim_end(&line);
            if end.ends_with("*/") {
                line
            } else if line.ends_with("*;") {
                format!("{}*/", &line[..line.len() - 2])
            } else if end.ends_with('*') {
                format!("{line}/")
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalize_style_comment_run_lines(value: &str) -> String {
    let mut out = Vec::new();
    for line in split_lines(value) {
        let ind = leading_ws(&line).to_owned();
        let trimmed = js_trim(&line);
        if !trimmed.starts_with("/*") {
            out.push(line);
            continue;
        }
        let (comments, cursor, failed) = leading_comments(trimmed, 0);
        if failed || comments.is_empty() {
            out.push(line);
            continue;
        }
        let tail = js_trim_start(&trimmed[cursor..]);
        let split = comments.len() >= 2
            || (!tail.is_empty()
                && (is_selector_like(tail, false)
                    || tail.starts_with('@')
                    || tail.starts_with("/*")));
        if !split {
            out.push(line);
            continue;
        }
        for c in comments {
            out.push(format!("{ind}{c}"));
        }
        if !tail.is_empty() {
            out.push(format!("{ind}{tail}"));
        }
    }
    out.join("\n")
}

fn is_structural_line(t: &str) -> bool {
    directive_name_from_line(t).is_some_and(|n| is_structural(&n))
}

fn prev_non_blank(lines: &[String], i: usize) -> Option<usize> {
    (0..i).rev().find(|&j| !js_trim(&lines[j]).is_empty())
}

fn normalize_style_directive_css_context_indent(value: &str, unit: &str) -> String {
    let mut lines = split_lines(value);
    for i in 0..lines.len() {
        let trimmed = js_trim(&lines[i]).to_owned();
        if !is_structural_line(&trimmed) {
            continue;
        }
        let Some(p) = prev_non_blank(&lines, i) else {
            continue;
        };
        let prev_trimmed = js_trim(&lines[p]).to_owned();
        let current_indent = leading_ws(&lines[i]).len();
        let prev_indent = leading_ws(&lines[p]).to_owned();
        let mut next_indent: Option<String> = None;
        if prev_trimmed == "}" && prev_indent.len() > current_indent {
            next_indent = Some(prev_indent);
        } else if prev_trimmed.ends_with('{') && !is_structural_line(&prev_trimmed) {
            let nested = format!("{prev_indent}{unit}");
            if nested.len() > current_indent {
                next_indent = Some(nested);
            }
        }
        if let Some(ni) = next_indent {
            lines[i] = format!("{ni}{trimmed}");
            if let Some(n) = (i + 1..lines.len()).find(|&j| !js_trim(&lines[j]).is_empty()) {
                let nt = js_trim(&lines[n]).to_owned();
                if is_selector_like(&nt, true) && leading_ws(&lines[n]).len() < ni.len() {
                    lines[n] = format!("{ni}{nt}");
                }
            }
        }
    }
    lines.join("\n")
}

fn set_line(lines: &mut [String], i: usize, next: String, changed: &mut bool) {
    if next != lines[i] {
        lines[i] = next;
        *changed = true;
    }
}

fn normalize_style_directive_selector_alignment(value: &str, unit: &str) -> String {
    let mut lines = split_lines(value);
    for _ in 0..4 {
        let mut changed = false;
        for i in 0..lines.len() {
            let normalized = normalize_directive_line_arg_spacing(&lines[i]);
            set_line(&mut lines, i, normalized, &mut changed);
        }
        for i in 0..lines.len() {
            let current = js_trim(&lines[i]).to_owned();
            if current.is_empty() {
                continue;
            }
            let Some(p) = prev_non_blank(&lines, i) else {
                continue;
            };
            if !is_structural_line(js_trim(&lines[p])) {
                continue;
            }
            if !current.ends_with('{') || directive_name_from_line(&current).is_some() {
                continue;
            }
            let prev_indent = leading_ws(&lines[p]).to_owned();
            if leading_ws(&lines[i]).len() <= prev_indent.len() {
                continue;
            }
            set_line(
                &mut lines,
                i,
                format!("{prev_indent}{current}"),
                &mut changed,
            );
        }
        for i in 0..lines.len() {
            let current = js_trim(&lines[i]).to_owned();
            if !is_style_comment_line(&current) || !is_directive_comment(&current) {
                continue;
            }
            let Some(p) = prev_non_blank(&lines, i) else {
                continue;
            };
            let mut prev_indent = leading_ws(&lines[p]).to_owned();
            let mut structural = is_structural_line(js_trim(&lines[p]));
            if !structural {
                let mut a = p;
                while a > 0 {
                    a -= 1;
                    let at = js_trim(&lines[a]);
                    if at.is_empty() || is_style_comment_line(at) {
                        continue;
                    }
                    if is_structural_line(at) {
                        structural = true;
                        leading_ws(&lines[a]).clone_into(&mut prev_indent);
                    }
                    break;
                }
            }
            if !structural || leading_ws(&lines[i]).len() <= prev_indent.len() {
                continue;
            }
            set_line(
                &mut lines,
                i,
                format!("{prev_indent}{current}"),
                &mut changed,
            );
        }
        for i in 0..lines.len() {
            let current = js_trim(&lines[i]).to_owned();
            let Some(cur_name) = directive_name_from_line(&current).filter(|n| is_structural(n))
            else {
                continue;
            };
            let Some(p) = prev_non_blank(&lines, i) else {
                continue;
            };
            let prev_trimmed = js_trim(&lines[p]).to_owned();
            let prev_indent = leading_ws(&lines[p]).to_owned();
            let current_indent = leading_ws(&lines[i]).len();
            if is_style_comment_line(&prev_trimmed) && current_indent > prev_indent.len() {
                set_line(
                    &mut lines,
                    i,
                    format!("{prev_indent}{current}"),
                    &mut changed,
                );
                continue;
            }
            let Some(prev_name) =
                directive_name_from_line(&prev_trimmed).filter(|n| is_structural(n))
            else {
                continue;
            };
            if current_indent <= prev_indent.len() {
                continue;
            }
            let branch_or_close =
                |n: &str| STYLE_BRANCHES.contains(&n) || STYLE_CLOSERS.contains(&n);
            if branch_or_close(&cur_name) || branch_or_close(&prev_name) {
                set_line(
                    &mut lines,
                    i,
                    format!("{prev_indent}{current}"),
                    &mut changed,
                );
                continue;
            }
            if STYLE_OPENERS.contains(&cur_name.as_str())
                && STYLE_OPENERS.contains(&prev_name.as_str())
            {
                let mut leads_to_selector = false;
                for line in &lines[i + 1..] {
                    let nt = js_trim(line);
                    if nt.is_empty() || nt == current || is_style_comment_line(nt) {
                        continue;
                    }
                    if directive_name_from_line(nt).is_some_and(|n| is_structural(&n)) {
                        continue;
                    }
                    leads_to_selector = is_selector_like(nt, true);
                    break;
                }
                if leads_to_selector {
                    set_line(
                        &mut lines,
                        i,
                        format!("{prev_indent}{current}"),
                        &mut changed,
                    );
                }
            }
        }
        if !changed {
            break;
        }
    }
    let adjusted = normalize_style_directive_css_context_indent(&lines.join("\n"), unit);
    let aligned = realign_after_directive_lines(
        &adjusted,
        &[
            "else",
            "elseif",
            "case",
            "default",
            "endif",
            "endunless",
            "endisset",
            "endempty",
            "endfor",
            "endforeach",
            "endforelse",
            "endwhile",
            "endswitch",
            "endphp",
            "endverbatim",
        ],
        |second| {
            second.starts_with('@')
                && [
                    "if",
                    "unless",
                    "isset",
                    "empty",
                    "for",
                    "foreach",
                    "forelse",
                    "while",
                    "switch",
                    "php",
                    "verbatim",
                    "else",
                    "elseif",
                    "case",
                    "default",
                    "endif",
                    "endunless",
                    "endisset",
                    "endempty",
                    "endfor",
                    "endforeach",
                    "endforelse",
                    "endwhile",
                    "endswitch",
                    "endphp",
                    "endverbatim",
                ]
                .iter()
                .any(|w| second[1..].starts_with(w))
        },
    );
    realign_after_directive_lines(
        &aligned,
        &[
            "if",
            "unless",
            "isset",
            "empty",
            "for",
            "foreach",
            "forelse",
            "while",
            "switch",
            "php",
            "verbatim",
            "else",
            "elseif",
            "case",
            "default",
            "endif",
            "endunless",
            "endisset",
            "endempty",
            "endfor",
            "endforeach",
            "endforelse",
            "endwhile",
            "endswitch",
            "endphp",
            "endverbatim",
        ],
        |second| {
            let Some(&first) = second.as_bytes().first() else {
                return false;
            };
            (matches!(first, b'.' | b'#' | b':' | b'[' | b'*') || is_ident_start(first))
                && second.trim_end_matches([' ', '\t']).ends_with('{')
        },
    )
}

/// The two final `replace`s of `normalizeStyleDirectiveSelectorAlignment`: a line that starts
/// with `@` and one of `firsts`, then an indented line `second` accepts, which takes the first
/// line's indent.
fn realign_after_directive_lines(
    value: &str,
    firsts: &[&str],
    second: impl Fn(&str) -> bool,
) -> String {
    let mut lines: Vec<String> = value.split('\n').map(str::to_owned).collect();
    let mut k = 0;
    while k + 1 < lines.len() {
        let first = &lines[k];
        let ind = indent_len(first);
        let body = &first[ind..];
        let first_ok = body.starts_with('@') && firsts.iter().any(|w| body[1..].starts_with(w));
        let next = &lines[k + 1];
        let next_ind = indent_len(next);
        if first_ok && next_ind > 0 && second(&next[next_ind..]) {
            let prefix = first[..ind].to_owned();
            lines[k + 1] = format!("{prefix}{}", &next[next_ind..]);
            k += 2;
        } else {
            k += 1;
        }
    }
    lines.join("\n")
}

/// `normalizeStyleEmbeddedOutput`
fn normalize_style_embedded_output(value: &str) -> String {
    let unit = " ".repeat(TAB_WIDTH);
    let mut v = normalize_style_directive_boundaries(value);
    v = normalize_style_blade_value_indentation(&v);
    v = normalize_style_structural_directive_semicolons(&v);
    v = normalize_style_directive_comment_chains(&v);
    v = normalize_style_directive_comment_selector_lines(&v);
    v = normalize_style_directive_chain_blank_lines(&v);
    v = normalize_style_blank_line_runs(&v);
    v = normalize_style_standalone_semicolon_lines(&v);
    v = repair_style_broken_comment_closers(&v);
    v = normalize_style_comment_run_lines(&v);
    v = normalize_style_directive_selector_alignment(&v, &unit);
    trim_trailing_horizontal_whitespace(&v)
}

fn strip_boundary_line_breaks(value: &str) -> &str {
    let v = value
        .strip_prefix("\r\n")
        .or_else(|| value.strip_prefix('\n'))
        .unwrap_or(value);
    v.strip_suffix("\r\n")
        .or_else(|| v.strip_suffix('\n'))
        .unwrap_or(v)
}

/// `value.replace(/\r?\n[^\S\r\n]*$/u, "")`
fn trim_trailing_whitespace_only_line(value: &str) -> &str {
    let t = value.trim_end_matches(|c: char| c != '\n' && c != '\r' && is_js_whitespace(c));
    if let Some(s) = t.strip_suffix('\n') {
        s.strip_suffix('\r').unwrap_or(s)
    } else {
        value
    }
}

fn trim_final_line_break(value: &str) -> &str {
    value
        .strip_suffix("\r\n")
        .or_else(|| value.strip_suffix('\n'))
        .unwrap_or(value)
}

fn normalize_embedded_raw_content_fallback(value: &str) -> String {
    let v = trim_trailing_whitespace_only_line(strip_boundary_line_breaks(value));
    normalize_line_endings_to_lf(&dedent_string(v))
}

fn normalize_embedded_output(value: &str, style: bool) -> String {
    let normalized = if style {
        normalize_style_embedded_output(value)
    } else {
        value.to_owned()
    };
    let lf = normalize_line_endings_to_lf(&normalized);
    trim_final_line_break(trim_trailing_whitespace_only_line(&lf)).to_owned()
}

fn normalize_multiline_replacement_text(value: &str) -> String {
    let lf = normalize_line_endings_to_lf(value);
    let mut lines: Vec<String> = lf.split('\n').map(str::to_owned).collect();
    if lines.len() <= 1 {
        return value.to_owned();
    }
    let min = lines
        .iter()
        .skip(1)
        .filter(|l| !js_trim(l).is_empty())
        .map(|l| indent_len(l))
        .min();
    match min {
        Some(m) if m > 0 => {
            for l in lines.iter_mut().skip(1) {
                if !js_trim(l).is_empty() {
                    *l = l[m.min(l.len())..].to_owned();
                }
            }
            lines.join("\n")
        }
        _ => lines.join("\n"),
    }
}

fn normalize_multiline_directive_replacement_text(value: &str, unit: &str) -> String {
    let normalized = normalize_multiline_replacement_text(value);
    let lines: Vec<&str> = normalized.split('\n').collect();
    let last = lines.len() - 1;
    // `/\(\s*\[\s*$/u`
    let first_ok = js_trim_end(lines[0])
        .strip_suffix('[')
        .is_some_and(|r| js_trim_end(r).ends_with('('));
    if !first_ok
        || js_trim(lines[last]) != "])"
        || !lines[1..last]
            .iter()
            .any(|l| l.chars().next().is_some_and(|c| !is_js_whitespace(c)))
    {
        return normalized;
    }
    lines
        .iter()
        .enumerate()
        .map(|(i, l)| {
            if i > 0 && i < last && !js_trim(l).is_empty() {
                format!("{unit}{l}")
            } else {
                (*l).to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `/\/[^\r\n;{}]*\\\s*\/\s*\//u`
fn has_unstable_escaped_slash_literal(b: &[u8]) -> bool {
    for i in 0..b.len() {
        if b[i] != b'/' {
            continue;
        }
        let mut j = i + 1;
        while j < b.len() && !matches!(b[j], b'\r' | b'\n' | b';' | b'{' | b'}') {
            if b[j] == b'\\' {
                let mut k = j + 1;
                while k < b.len() && b[k].is_ascii_whitespace() {
                    k += 1;
                }
                if b.get(k) == Some(&b'/') {
                    k += 1;
                    while k < b.len() && b[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    if b.get(k) == Some(&b'/') {
                        return true;
                    }
                }
            }
            j += 1;
        }
    }
    false
}

/// `STYLE_RAW_BLADE_FALLBACK_RE`
fn has_style_raw_blade_fallback(s: &str) -> bool {
    if s.contains("{{") || s.contains("{!!") {
        return true;
    }
    let b = s.as_bytes();
    s.match_indices('@').any(|(i, _)| {
        STYLE_RAW_FALLBACK_NAMES.iter().any(|n| {
            s[i + 1..].starts_with(n) && !b.get(i + 1 + n.len()).is_some_and(|&c| is_ident(c))
        })
    })
}

/// `rawtext-script-scanner.ts`'s state at `offset`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScriptScan {
    Code,
    Single,
    Double,
    Template,
    LineComment,
    BlockComment,
    Regex,
    RegexClass,
}

fn is_id_part(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

fn prev_identifier(b: &[u8], content_start: usize, index: usize) -> Option<&[u8]> {
    if index < content_start || !is_id_part(b[index]) {
        return None;
    }
    let mut s = index;
    while s > content_start && is_id_part(b[s - 1]) {
        s -= 1;
    }
    Some(&b[s..=index])
}

fn is_regex_after_condition_paren(b: &[u8], content_start: usize, close: usize) -> bool {
    let mut depth = 0i32;
    let mut open = None;
    for i in (content_start..=close).rev() {
        if b[i] == b')' {
            depth += 1;
        } else if b[i] == b'(' {
            depth -= 1;
            if depth == 0 {
                open = Some(i);
                break;
            }
        }
    }
    let Some(open) = open else { return false };
    let mut i = open;
    while i > content_start {
        i -= 1;
        if is_ws_byte(b[i]) {
            continue;
        }
        return prev_identifier(b, content_start, i)
            .is_some_and(|t| [&b"if"[..], b"while", b"for", b"with", b"catch"].contains(&t));
    }
    false
}

fn is_likely_regex_start(b: &[u8], content_start: usize, slash: usize) -> bool {
    const PREFIX: &[&[u8]] = &[
        b"return",
        b"throw",
        b"do",
        b"case",
        b"delete",
        b"void",
        b"typeof",
        b"instanceof",
        b"in",
        b"of",
        b"yield",
        b"await",
        b"else",
    ];
    let mut i = slash;
    while i > content_start {
        i -= 1;
        let ch = b[i];
        if is_ws_byte(ch) {
            continue;
        }
        if matches!(ch, b'\'' | b'"' | b'`') {
            return false;
        }
        if is_id_part(ch) {
            return prev_identifier(b, content_start, i).is_some_and(|t| PREFIX.contains(&t));
        }
        if ch == b')' {
            return is_regex_after_condition_paren(b, content_start, i);
        }
        if ch == b'+' || ch == b'-' {
            let before = (content_start..i).rev().find(|&k| !is_ws_byte(b[k]));
            return before.is_none_or(|k| b[k] != ch);
        }
        return !matches!(ch, b']' | b'}');
    }
    true
}

fn script_scan_state(source: &str, content_start: usize, offset: usize) -> ScriptScan {
    let b = source.as_bytes();
    let mut state = ScriptScan::Code;
    let mut template_depths: Vec<i32> = Vec::new();
    let mut i = content_start;
    while i < offset {
        let ch = b[i];
        let next = if i + 1 < offset { b[i + 1] } else { 0 };
        match state {
            ScriptScan::LineComment => {
                if ch == b'\n' {
                    state = ScriptScan::Code;
                }
            }
            ScriptScan::BlockComment => {
                if ch == b'*' && next == b'/' {
                    state = ScriptScan::Code;
                    i += 1;
                }
            }
            ScriptScan::Single | ScriptScan::Double => {
                if ch == b'\\' {
                    i += 1;
                } else if (state == ScriptScan::Single && ch == b'\'')
                    || (state == ScriptScan::Double && ch == b'"')
                {
                    state = ScriptScan::Code;
                }
            }
            ScriptScan::Template => {
                if ch == b'\\' {
                    i += 1;
                } else if ch == b'$' && next == b'{' {
                    template_depths.push(1);
                    state = ScriptScan::Code;
                    i += 1;
                } else if ch == b'`' {
                    state = ScriptScan::Code;
                }
            }
            ScriptScan::Regex => {
                if ch == b'\\' {
                    i += 1;
                } else if ch == b'[' {
                    state = ScriptScan::RegexClass;
                } else if ch == b'/' {
                    state = ScriptScan::Code;
                }
            }
            ScriptScan::RegexClass => {
                if ch == b'\\' {
                    i += 1;
                } else if ch == b']' {
                    state = ScriptScan::Regex;
                }
            }
            ScriptScan::Code => {
                let mut handled = false;
                if let Some(depth) = template_depths.last_mut() {
                    if ch == b'{' {
                        *depth += 1;
                        handled = true;
                    } else if ch == b'}' {
                        *depth -= 1;
                        if *depth == 0 {
                            template_depths.pop();
                            state = ScriptScan::Template;
                        }
                        handled = true;
                    }
                }
                if !handled {
                    if ch == b'/' && next == b'/' {
                        state = ScriptScan::LineComment;
                        i += 1;
                    } else if ch == b'/' && next == b'*' {
                        state = ScriptScan::BlockComment;
                        i += 1;
                    } else if ch == b'/' && is_likely_regex_start(b, content_start, i) {
                        state = ScriptScan::Regex;
                    } else if ch == b'\'' {
                        state = ScriptScan::Single;
                    } else if ch == b'"' {
                        state = ScriptScan::Double;
                    } else if ch == b'`' {
                        state = ScriptScan::Template;
                    }
                }
            }
        }
        i += 1;
    }
    state
}

/// `isLikelyInsideStyleLiteralOrComment`
fn is_likely_inside_style_literal_or_comment(
    source: &str,
    content_start: usize,
    offset: usize,
) -> bool {
    let b = source.as_bytes();
    let mut state = Scan::Code;
    let mut i = content_start;
    while i < offset {
        let ch = b[i];
        let next = if i + 1 < offset { b[i + 1] } else { 0 };
        match state {
            Scan::LineComment => {
                if ch == b'\n' {
                    state = Scan::Code;
                }
            }
            Scan::BlockComment => {
                if ch == b'*' && next == b'/' {
                    state = Scan::Code;
                    i += 1;
                }
            }
            Scan::Single | Scan::Double => {
                if ch == b'\\' {
                    i += 1;
                } else if (state == Scan::Single && ch == b'\'')
                    || (state == Scan::Double && ch == b'"')
                {
                    state = Scan::Code;
                }
            }
            Scan::Code => {
                if ch == b'/' && next == b'*' {
                    state = Scan::BlockComment;
                    i += 1;
                } else if ch == b'/'
                    && next == b'/'
                    && treat_as_style_line_comment(b, content_start, i)
                {
                    state = Scan::LineComment;
                    i += 1;
                } else if ch == b'\'' {
                    state = Scan::Single;
                } else if ch == b'"' {
                    state = Scan::Double;
                }
            }
        }
        i += 1;
    }
    state != Scan::Code
}

fn is_inside_style_comment_by_line(source: &str, content_start: usize, offset: usize) -> bool {
    let line_start = content_start
        .max(
            source[..content_start.max(offset)]
                .rfind('\n')
                .map_or(0, |p| p + 1),
        )
        .min(offset);
    let prefix = &source[line_start..offset];
    let open = prefix.rfind("/*").map_or(-1, |p| p as isize);
    let close = prefix.rfind("*/").map_or(-1, |p| p as isize);
    if open > close {
        return true;
    }
    prefix.rfind("//").is_some_and(|lc| {
        treat_as_style_line_comment(source.as_bytes(), content_start, line_start + lc)
    })
}

fn is_slash_delimited_style_literal(
    b: &[u8],
    start: usize,
    end: usize,
    content_start: usize,
    content_end: usize,
) -> bool {
    let prev = (content_start..start).rev().find(|&i| !is_ws_byte(b[i]));
    if prev.is_none_or(|p| b[p] != b'/') {
        return false;
    }
    let mut i = end;
    while i < content_end {
        match b[i] {
            c if is_ws_byte(c) => {}
            b'\\' => i += 1,
            b'/' => return true,
            b';' | b'{' | b'}' => return false,
            _ => {}
        }
        i += 1;
    }
    false
}

/// The `[lineStart, lineEnd)` of the line around `[start, end)`, inside the content.
fn line_bounds(
    source: &str,
    start: usize,
    end: usize,
    content_start: usize,
    content_end: usize,
) -> (usize, usize) {
    let line_start = content_start.max(
        source[..content_start.max(start)]
            .rfind('\n')
            .map_or(0, |p| p + 1),
    );
    let line_end = source[end..]
        .find('\n')
        .map(|p| p + end)
        .filter(|&e| e <= content_end)
        .unwrap_or(content_end);
    (line_start, line_end)
}

impl Printer<'_> {
    fn directive_name_of(&self, id: Id) -> Option<String> {
        if self.kind(id) != NodeKind::Directive {
            return None;
        }
        let t = self
            .tokens_of(id)
            .iter()
            .find(|t| t.kind == TokenType::Directive)?;
        let raw = self.tok_text(t);
        raw.strip_prefix('@').map(str::to_lowercase)
    }

    fn has_directive_args_token(&self, id: Id) -> bool {
        self.kind(id) == NodeKind::Directive
            && self
                .tokens_of(id)
                .iter()
                .any(|t| t.kind == TokenType::DirectiveArgs)
    }

    fn is_expression_directive(&self, id: Id) -> bool {
        self.directive_name_of(id).is_some_and(|n| {
            EXPRESSION_DIRECTIVES.contains(&n.as_str())
                || (n == "php" && self.has_directive_args_token(id))
        })
    }

    /// `isKnownBladeDirective`: `getDirective(name) !== undefined`, and `getDirective` answers
    /// `null` for an unknown name, so any directive name counts.
    fn is_known_blade_directive(&self, id: Id) -> bool {
        self.directive_name_of(id).is_some()
    }

    fn placeholder_kind(&self, id: Id) -> Slot {
        match self.kind(id) {
            NodeKind::Echo
            | NodeKind::RawEcho
            | NodeKind::TripleEcho
            | NodeKind::PhpTag
            | NodeKind::PhpBlock => Slot::Expr,
            NodeKind::Directive if self.is_expression_directive(id) => Slot::Expr,
            _ => Slot::Stmt,
        }
    }

    fn is_echo_construct(&self, id: Id) -> bool {
        matches!(
            self.kind(id),
            NodeKind::Echo | NodeKind::RawEcho | NodeKind::TripleEcho
        )
    }

    fn is_multiline_echo_construct(&self, id: Id) -> bool {
        self.is_echo_construct(id) && {
            let raw = self.full_text(id);
            raw.contains('\n') || raw.contains('\r')
        }
    }

    fn is_leaf_construct(&self, id: Id) -> bool {
        matches!(
            self.kind(id),
            NodeKind::Directive
                | NodeKind::Echo
                | NodeKind::RawEcho
                | NodeKind::TripleEcho
                | NodeKind::PhpBlock
                | NodeKind::PhpTag
                | NodeKind::BladeComment
                | NodeKind::Comment
                | NodeKind::BogusComment
                | NodeKind::NonOutput
        )
    }

    fn is_blade_or_php_fallback_construct(&self, id: Id) -> bool {
        matches!(
            self.kind(id),
            NodeKind::Directive
                | NodeKind::Echo
                | NodeKind::RawEcho
                | NodeKind::TripleEcho
                | NodeKind::PhpBlock
                | NodeKind::PhpTag
                | NodeKind::BladeComment
        )
    }

    pub(crate) fn element_content_range(&self, id: Id) -> (usize, usize) {
        let n = self.n(id);
        if n.open_tag_end_offset > 0 && n.closing_tag_start_offset > n.open_tag_end_offset {
            return (n.open_tag_end_offset, n.closing_tag_start_offset);
        }
        match (self.first_child(id), self.last_child(id)) {
            (Some(f), Some(l)) => (self.n(f).start, self.n(l).end),
            _ => (n.start, n.start),
        }
    }

    fn collect_leaf_constructs(&self, id: Id, start: usize, end: usize) -> Vec<Id> {
        let mut out = Vec::new();
        let mut stack: Vec<Id> = self.n(id).children.iter().rev().copied().collect();
        while let Some(current) = stack.pop() {
            let c = self.n(current);
            if c.end <= start || c.start >= end {
                continue;
            }
            if self.is_leaf_construct(current) {
                if c.start >= start && c.end <= end {
                    out.push(current);
                }
                if self.kind(current) != NodeKind::Directive || c.children.is_empty() {
                    continue;
                }
            }
            stack.extend(c.children.iter().rev().copied());
        }
        out.sort_by_key(|&c| (self.n(c).start, self.n(c).end));
        out
    }

    fn replacement_range(&self, id: Id) -> (usize, usize) {
        let n = self.n(id);
        if self.kind(id) != NodeKind::Directive {
            return (n.start, n.end);
        }
        let tokens = self.tokens_of(id);
        match (tokens.first(), tokens.last()) {
            (Some(f), Some(l)) => (f.start, l.end),
            _ => (n.start, n.end),
        }
    }

    fn construct_replacement_text(&self, id: Id) -> String {
        match self.kind(id) {
            NodeKind::Echo | NodeKind::RawEcho | NodeKind::TripleEcho => {
                let rendered = doc_to_flat_string(&self.print_echo(id))
                    .unwrap_or_else(|| self.full_text(id).to_owned());
                self.normalize_multiline_echo_indent_text(id, &rendered)
            }
            NodeKind::Directive => self.render_directive_tokens(id),
            _ => self.full_text(id).to_owned(),
        }
    }

    fn is_construct_inside_script_literal_or_comment(
        &self,
        id: Id,
        start: usize,
        end: usize,
    ) -> bool {
        let (s, e) = self.replacement_range(id);
        s >= start && e <= end && script_scan_state(self.source(), start, s) != ScriptScan::Code
    }

    fn is_directive_inside_script_literal_or_comment(
        &self,
        id: Id,
        start: usize,
        end: usize,
    ) -> bool {
        self.kind(id) == NodeKind::Directive
            && self.is_construct_inside_script_literal_or_comment(id, start, end)
    }

    fn is_construct_inside_script_comment(&self, id: Id, start: usize, end: usize) -> bool {
        let (s, e) = self.replacement_range(id);
        s >= start
            && e <= end
            && matches!(
                script_scan_state(self.source(), start, s),
                ScriptScan::LineComment | ScriptScan::BlockComment
            )
    }

    fn is_directive_inside_style_literal_or_comment(
        &self,
        id: Id,
        start: usize,
        end: usize,
    ) -> bool {
        if self.kind(id) != NodeKind::Directive {
            return false;
        }
        let (s, e) = self.replacement_range(id);
        if s < start || e > end {
            return false;
        }
        let src = self.source();
        is_inside_style_comment_by_line(src, start, s)
            || is_slash_delimited_style_literal(src.as_bytes(), s, e, start, end)
            || is_likely_inside_style_literal_or_comment(src, start, s)
    }

    fn is_construct_inside_style_literal_or_comment(
        &self,
        id: Id,
        start: usize,
        end: usize,
    ) -> bool {
        if self.kind(id) == NodeKind::Directive {
            return self.is_directive_inside_style_literal_or_comment(id, start, end);
        }
        let (s, e) = self.replacement_range(id);
        if s < start || e > end {
            return false;
        }
        is_inside_style_comment_by_line(self.source(), start, s)
            || is_likely_inside_style_literal_or_comment(self.source(), start, s)
    }

    fn is_structural_script_directive(&self, id: Id) -> bool {
        if self.kind(id) != NodeKind::Directive {
            return false;
        }
        if !self.n(id).children.is_empty() {
            return true;
        }
        let Some(d) = self
            .directive_name_of(id)
            .and_then(|n| self.ast.directives.get(&n))
        else {
            return false;
        };
        d.role != Role::None
            || d.is_condition
            || d.is_switch
            || d.is_switch_branch
            || d.is_switch_terminator
            || d.is_conditional_pair
            || d.is_conditional_close
            || !d.condition_like_branches.is_empty()
            || !d.terminators.is_empty()
    }

    fn directive_line_has_other_text(&self, id: Id, start: usize, end: usize) -> bool {
        let (s, e) = self.replacement_range(id);
        let src = self.source();
        let (ls, le) = line_bounds(src, s, e, start, end);
        !js_trim(&src[ls.min(s)..s]).is_empty() || !js_trim(&src[e..le.max(e)]).is_empty()
    }

    fn is_standalone_script_directive(&self, id: Id, start: usize, end: usize) -> bool {
        if self.kind(id) != NodeKind::Directive {
            return false;
        }
        let (s, e) = self.replacement_range(id);
        if s < start || e > end || script_scan_state(self.source(), start, s) != ScriptScan::Code {
            return false;
        }
        !self.directive_line_has_other_text(id, start, end)
    }

    fn is_inline_script_directive_expression(&self, id: Id, start: usize, end: usize) -> bool {
        self.kind(id) == NodeKind::Directive
            && self.n(id).children.is_empty()
            && !self.is_structural_script_directive(id)
            && !self.is_directive_inside_script_literal_or_comment(id, start, end)
            && self.directive_line_has_other_text(id, start, end)
    }

    fn is_inline_style_directive_expression(&self, id: Id, start: usize, end: usize) -> bool {
        self.kind(id) == NodeKind::Directive
            && self.n(id).children.is_empty()
            && !self.is_structural_script_directive(id)
            && !self.is_directive_inside_style_literal_or_comment(id, start, end)
            && self.directive_line_has_other_text(id, start, end)
    }

    fn is_style_directive_boundary(&self, id: Id) -> bool {
        let Some(name) = self.directive_name_of(id) else {
            return false;
        };
        STYLE_BRANCHES.contains(&name.as_str())
            || STYLE_CLOSERS.contains(&name.as_str())
            || name
                .strip_prefix("end")
                .is_some_and(|rest| self.ast.directives.has_seen(rest))
    }

    fn body_lang(&self, id: Id) -> Option<(&'static str, Option<Lang>)> {
        let parser = self.infer_element_parser(id)?;
        Some((parser, Lang::from_parser(parser, self.is_module_script(id))))
    }

    pub(crate) fn is_module_script(&self, id: Id) -> bool {
        let ty = self.attr_map_get(id, "type");
        ty.as_deref() == Some("module")
            || (matches!(ty.as_deref(), Some("text/babel" | "text/jsx"))
                && self.attr_map_get(id, "data-type").as_deref() == Some("module"))
    }

    /// `shouldBypassStyleParserEmbedding`
    pub(crate) fn should_bypass_style_parser_embedding(&self, id: Id) -> bool {
        if self.kind(id) != NodeKind::Element || self.n(id).tag_name != "style" {
            return false;
        }
        if !self
            .infer_element_parser(id)
            .is_some_and(|p| matches!(p, "css" | "scss" | "less"))
        {
            return false;
        }
        let (start, end) = self.element_content_range(id);
        if end <= start {
            return false;
        }
        let raw = &self.source()[start..end];
        has_unstable_escaped_slash_literal(raw.as_bytes())
            || utf16_len(raw) >= LARGE_STYLE_EMBED_CHAR_THRESHOLD
            || raw.matches('\n').count() + 1 > LARGE_STYLE_EMBED_LINE_THRESHOLD
    }

    /// `shouldUseMixedRawContentEmbedding`
    pub(crate) fn should_use_mixed_raw_content_embedding(&self, id: Id) -> bool {
        if self.should_bypass_style_parser_embedding(id) || self.infer_element_parser(id).is_none()
        {
            return false;
        }
        let (start, end) = self.element_content_range(id);
        let constructs = self.collect_leaf_constructs(id, start, end);
        if self.n(id).tag_name == "script" {
            // `bladePhpFormatting` is off.
            if constructs.is_empty() {
                return false;
            }
            if constructs.iter().any(|&c| {
                self.is_echo_construct(c)
                    && !self.is_construct_inside_script_literal_or_comment(c, start, end)
            }) {
                return true;
            }
            if constructs.iter().any(|&c| {
                self.is_multiline_echo_construct(c)
                    && !self.is_construct_inside_script_literal_or_comment(c, start, end)
            }) {
                return true;
            }
            if constructs.iter().any(|&c| {
                matches!(self.kind(c), NodeKind::PhpBlock | NodeKind::PhpTag)
                    || (self.kind(c) == NodeKind::Directive
                        && !self.is_directive_inside_script_literal_or_comment(c, start, end)
                        && (self.is_structural_script_directive(c)
                            || self.is_expression_directive(c)
                            || self.is_inline_script_directive_expression(c, start, end)
                            || self.is_standalone_script_directive(c, start, end)))
            }) {
                return true;
            }
            return constructs.iter().any(|&c| {
                self.kind(c) == NodeKind::Directive
                    && self.is_known_blade_directive(c)
                    && self.is_directive_inside_script_literal_or_comment(c, start, end)
            });
        }
        if self
            .n(id)
            .children
            .iter()
            .any(|&c| self.is_blade_construct_child(c, RawContext::Style))
        {
            return true;
        }
        if constructs.iter().any(|&c| {
            self.kind(c) == NodeKind::Directive
                && self.is_known_blade_directive(c)
                && self.is_directive_inside_style_literal_or_comment(c, start, end)
        }) {
            return true;
        }
        if has_style_raw_blade_fallback(&self.source()[start..end]) {
            return true;
        }
        self.parent_contains_blade_syntax(id, RawContext::Style)
    }

    /// `shouldUseUnparsedRawContentEmbedding`
    pub(crate) fn should_use_unparsed_raw_content_embedding(&self, id: Id) -> bool {
        if self.should_bypass_style_parser_embedding(id) {
            return true;
        }
        let (start, end) = self.element_content_range(id);
        let constructs = self.collect_leaf_constructs(id, start, end);
        if self.infer_element_parser(id).is_none() {
            return constructs
                .iter()
                .any(|&c| self.is_blade_or_php_fallback_construct(c));
        }
        if !constructs
            .iter()
            .any(|&c| self.is_multiline_echo_construct(c))
        {
            return false;
        }
        self.n(id).tag_name == "script"
            && constructs.iter().any(|&c| {
                self.is_multiline_echo_construct(c)
                    && self.is_construct_inside_script_comment(c, start, end)
            })
    }

    /// `buildScriptLikeElementDoc`
    pub(crate) fn script_like_element_doc(
        &self,
        id: Id,
        content: Doc,
        raw: &str,
        already_indented: bool,
    ) -> Doc {
        let empty = js_trim(raw).is_empty();
        let body = if empty {
            Doc::default()
        } else if already_indented {
            concat(vec![hardline(), content])
        } else {
            indent(concat(vec![hardline(), content]))
        };
        let mut parts = vec![
            self.opening_tag_prefix(id),
            group(self.print_opening_tag(id)),
            body,
        ];
        parts.push(if empty { Doc::default() } else { hardline() });
        parts.extend(self.print_closing_tag(id));
        parts.push(self.closing_tag_suffix(id));
        concat(parts)
    }

    /// `embedUnparsedRawContentElement`
    pub(crate) fn embed_unparsed_raw_content_element(&self, id: Id) -> Doc {
        let (start, end) = self.element_content_range(id);
        let raw = &self.source()[start..end];
        let fallback = normalize_embedded_raw_content_fallback(raw);
        self.script_like_element_doc(id, concat(replace_eol(&fallback, &hardline())), raw, false)
    }

    fn fallback_doc(&self, id: Id, raw: &str) -> Doc {
        let fallback = normalize_embedded_raw_content_fallback(raw);
        self.script_like_element_doc(id, concat(replace_eol(&fallback, &hardline())), raw, false)
    }

    /// `embedMixedRawContentElement`
    pub(crate) fn embed_mixed_raw_content_element(&self, id: Id) -> Option<Doc> {
        let (parser, lang) = self.body_lang(id)?;
        let style_parser = matches!(parser, "css" | "scss" | "less");
        let tag = self.n(id).tag_name.clone();
        let (start, end) = self.element_content_range(id);
        let source = self.source();
        let raw = &source[start..end];
        let constructs: Vec<Id> = self
            .collect_leaf_constructs(id, start, end)
            .into_iter()
            .filter(|&c| {
                if self.kind(c) != NodeKind::Directive {
                    return true;
                }
                if tag == "script"
                    && self.is_directive_inside_script_literal_or_comment(c, start, end)
                {
                    return false;
                }
                if tag == "style"
                    && self.is_directive_inside_style_literal_or_comment(c, start, end)
                {
                    return false;
                }
                !(tag == "style"
                    && !self.is_blade_construct_child(c, RawContext::Style)
                    && !self.is_style_directive_boundary(c))
            })
            .collect();
        let key = marker_session_key(source, start, end);
        let candidates: Vec<Candidate> = constructs
            .iter()
            .map(|&c| {
                let (s, e) = self.replacement_range(c);
                let inside_literal = tag == "style"
                    && self.is_construct_inside_style_literal_or_comment(c, start, end);
                let mut kind = self.placeholder_kind(c);
                if kind == Slot::Stmt && self.kind(c) == NodeKind::Directive {
                    if tag == "script" && self.is_inline_script_directive_expression(c, start, end)
                    {
                        kind = Slot::Expr;
                    }
                    if tag == "style" && self.is_inline_style_directive_expression(c, start, end) {
                        kind = Slot::Expr;
                    }
                }
                let in_value = style_parser && is_in_css_value_context(source, s, start);
                let base = if inside_literal {
                    self.full_text(c).to_owned()
                } else {
                    self.construct_replacement_text(c)
                };
                let replacement = if in_value {
                    normalize_multiline_replacement_text(&base)
                } else {
                    base
                };
                Candidate {
                    node: c,
                    start: s,
                    end: e,
                    kind,
                    in_value,
                    replacement,
                }
            })
            .collect();

        let mut slots: Vec<(usize, usize, PlaceholderSlot)> = Vec::new();
        let mut i = 0;
        while i < candidates.len() {
            let c = &candidates[i];
            if style_parser && c.in_value {
                let (gs, mut ge) = (c.start, c.end);
                let mut replacement = c.replacement.clone();
                let mut has_stmt = c.kind == Slot::Stmt;
                let mut j = i + 1;
                while j < candidates.len() {
                    let n = &candidates[j];
                    if !n.in_value || !js_trim(&source[ge..n.start.max(ge)]).is_empty() {
                        break;
                    }
                    replacement.push('\n');
                    replacement.push_str(&n.replacement);
                    ge = n.end;
                    has_stmt |= n.kind == Slot::Stmt;
                    j += 1;
                }
                if has_stmt {
                    let lines: Vec<String> = replacement
                        .split('\n')
                        .map(|l| js_trim(l.strip_suffix('\r').unwrap_or(l)).to_owned())
                        .filter(|l| !l.is_empty())
                        .map(|l| format!("  {l}"))
                        .collect();
                    replacement = format!("\n{}\n  ", lines.join("\n"));
                }
                let marker = placeholder_marker(&key, Slot::Expr, slots.len(), style_parser, true);
                slots.push((
                    gs,
                    ge,
                    PlaceholderSlot {
                        node: c.node,
                        marker,
                        replacement,
                    },
                ));
                i = j;
                continue;
            }
            let marker = placeholder_marker(&key, c.kind, slots.len(), style_parser, c.in_value);
            slots.push((
                c.start,
                c.end,
                PlaceholderSlot {
                    node: c.node,
                    marker,
                    replacement: c.replacement.clone(),
                },
            ));
            i += 1;
        }

        let mut masked = String::new();
        let mut cursor = start;
        for (s, e, slot) in &slots {
            masked.push_str(&source[cursor..(*s).max(cursor)]);
            masked.push_str(&slot.marker);
            cursor = *e;
        }
        masked.push_str(&source[cursor.min(end)..end]);

        let comment_slots = if tag == "style" {
            Some(mask_style_directive_like_comments(&masked, &key))
        } else {
            None
        };
        let for_formatting = comment_slots
            .as_ref()
            .map_or(masked.as_str(), |(t, _)| t.as_str());
        let normalized = dedent_string(strip_boundary_line_breaks(for_formatting));
        let value = if parser == "markdown" {
            dedent_string(strip_first_blank_line(&normalized))
        } else {
            normalized
        };

        let Some(formatted) = lang.and_then(|l| oxc::format_body(&value, l)) else {
            return Some(self.fallback_doc(id, raw));
        };
        let slot_refs: Vec<&PlaceholderSlot> = slots.iter().map(|(_, _, s)| s).collect();
        let unit = " ".repeat(TAB_WIDTH);
        let Some(mut restored) = self.apply_marker_replacements(
            &formatted,
            &slot_refs,
            (tag == "script").then_some(unit.as_str()),
        ) else {
            return Some(self.fallback_doc(id, raw));
        };
        if let Some((_, comments)) = &comment_slots
            && !comments.is_empty()
        {
            let Some(r) = apply_style_comment_replacements(&restored, comments) else {
                return Some(self.fallback_doc(id, raw));
            };
            restored = r;
        }
        let output = normalize_embedded_output(&restored, tag == "style");
        Some(self.script_like_element_doc(
            id,
            concat(replace_eol(&output, &hardline())),
            raw,
            false,
        ))
    }

    fn apply_marker_replacements(
        &self,
        value: &str,
        slots: &[&PlaceholderSlot],
        unit: Option<&str>,
    ) -> Option<String> {
        let mut out = value.to_owned();
        let mut all_found = true;
        for slot in slots {
            let Some(start) = out.find(&slot.marker) else {
                all_found = false;
                continue;
            };
            if out[start + slot.marker.len()..].contains(&slot.marker) {
                all_found = false;
                continue;
            }
            let mut replacement = slot.replacement.clone();
            let multiline_directive = self.kind(slot.node) == NodeKind::Directive
                && (replacement.contains('\n') || replacement.contains('\r'));
            if let Some(unit) = unit {
                if multiline_directive {
                    replacement =
                        normalize_multiline_directive_replacement_text(&replacement, unit);
                }
                if self.is_multiline_echo_construct(slot.node) || multiline_directive {
                    let line_start = out[..start].rfind('\n').map_or(0, |p| p + 1);
                    let ind = &out[line_start..start];
                    let ind = &ind[..indent_len(ind)];
                    replacement = indent_continuation_lines(&replacement, ind);
                }
            }
            out = format!(
                "{}{}{}",
                &out[..start],
                replacement,
                &out[start + slot.marker.len()..]
            );
        }
        all_found.then_some(out)
    }

    /// The `style` element embed with non-text children (`handleElementEmbed`).
    pub(crate) fn embed_style_element_children(&self, id: Id) -> Option<Doc> {
        let (parser, lang) = self.body_lang(id)?;
        let lang = lang?;
        let raw = self.raw_element_content(id);
        let value = if parser == "markdown" {
            dedent_string(strip_first_blank_line(raw))
        } else {
            raw.to_owned()
        };
        let doc = oxc::body_to_doc(&self.ids, &value, lang)?;
        let empty = js_trim(raw).is_empty() || doc.is_empty_str();
        let mut parts = vec![
            self.opening_tag_prefix(id),
            group(self.print_opening_tag(id)),
        ];
        if !empty {
            parts.push(indent(concat(vec![hardline(), doc])));
            parts.push(hardline());
        }
        parts.extend(self.print_closing_tag(id));
        parts.push(self.closing_tag_suffix(id));
        Some(concat(parts))
    }

    fn raw_element_content(&self, id: Id) -> &str {
        let n = self.n(id);
        if !n.has_closing_tag {
            return "";
        }
        let (s, e) = self.element_content_range(id);
        if e <= s { "" } else { &self.source()[s..e] }
    }

    /// The text embed of a `<script>`/`<style>` body (`handleTextEmbed`).
    pub(crate) fn embed_script_text(&self, id: Id) -> Option<Doc> {
        let parent = self.n(id).parent?;
        if !self.is_script_like(parent) || self.should_bypass_style_parser_embedding(parent) {
            return None;
        }
        let (parser, lang) = self.body_lang(parent)?;
        if parser == "css" && self.parent_contains_blade_syntax(parent, RawContext::Style) {
            return None;
        }
        let raw = &self.n(id).raw_text;
        let value = if parser == "markdown" {
            dedent_string(strip_first_blank_line(raw))
        } else {
            raw.clone()
        };
        let doc = oxc::body_to_doc(&self.ids, &value, lang?)?;
        Some(concat(vec![
            crate::doc::break_parent(),
            self.opening_tag_prefix(id),
            doc,
            self.closing_tag_suffix(id),
        ]))
    }
}

/// `replace(/^[^\S\n]*\n/, "")`
fn strip_first_blank_line(s: &str) -> &str {
    let t = s.trim_start_matches(|c: char| c != '\n' && is_js_whitespace(c));
    t.strip_prefix('\n').unwrap_or(s)
}

/// `replacementText.replace(/\n(?=.)/gu, `\n${lineIndent}`)`
fn indent_continuation_lines(s: &str, ind: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '\n'
            && chars
                .peek()
                .is_some_and(|&n| !matches!(n, '\n' | '\r' | '\u{2028}' | '\u{2029}'))
        {
            out.push_str(ind);
        }
    }
    out
}

fn doc_to_flat_string(doc: &Doc) -> Option<String> {
    match doc {
        Doc::Str(s) => Some(s.to_string()),
        Doc::Array(parts) => parts.iter().map(doc_to_flat_string).collect(),
        _ => None,
    }
}

fn marker_session_key(source: &str, start: usize, end: usize) -> String {
    let base = format!(
        "{}_{}_{}",
        to_base36(utf16_len(source)),
        to_base36(utf16_len(&source[..start])),
        to_base36(utf16_len(&source[..end]))
    );
    let mut attempt = 0;
    loop {
        let candidate = if attempt == 0 {
            base.clone()
        } else {
            format!("{base}_{}", to_base36(attempt))
        };
        if !source.contains(&format!("__blade_expr_slot_{candidate}_"))
            && !source.contains(&format!("__blade_stmt_slot_{candidate}_"))
            && !source.contains(&format!("__blade_value_slot_{candidate}_"))
        {
            return candidate;
        }
        attempt += 1;
    }
}

fn placeholder_marker(key: &str, kind: Slot, index: usize, style: bool, in_value: bool) -> String {
    if style {
        if in_value {
            return format!("__blade_value_slot_{key}_{index}__");
        }
        return match kind {
            Slot::Expr => format!("/*__blade_expr_slot_{key}_{index}__*/"),
            Slot::Stmt => format!("@__blade_stmt_slot_{key}_{index};"),
        };
    }
    match kind {
        Slot::Expr => format!("__blade_expr_slot_{key}_{index}__"),
        Slot::Stmt => format!("/*__blade_stmt_slot_{key}_{index}__*/"),
    }
}

/// `isInCssValueContext`
fn is_in_css_value_context(source: &str, slot_start: usize, content_start: usize) -> bool {
    let b = source.as_bytes();
    let mut i = slot_start;
    while i > content_start {
        i -= 1;
        let ch = b[i];
        match ch {
            b' ' | b'\t' | b'\r' | b'\n' => {}
            b':' => return true,
            b'}' => {
                if (i > content_start && b[i - 1] == b'}')
                    || (i + 1 < slot_start && b[i + 1] == b'}')
                    || (i > content_start && b[i - 1] == b'!')
                {
                    continue;
                }
                return false;
            }
            b'{' => {
                if (i > content_start && b[i - 1] == b'{')
                    || (i + 1 < slot_start && matches!(b[i + 1], b'{' | b'!'))
                {
                    continue;
                }
                return false;
            }
            b';' => return false,
            _ => {}
        }
    }
    false
}

fn mask_style_directive_like_comments(value: &str, key: &str) -> (String, Vec<(String, String)>) {
    let mut slots = Vec::new();
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(open) = rest.find("/*") {
        let Some(close) = rest[open + 2..].find("*/") else {
            break;
        };
        let end = open + 2 + close + 2;
        let comment = &rest[open..end];
        out.push_str(&rest[..open]);
        if comment.contains('@') {
            let marker = format!("/*__blade_comment_slot_{key}_{}__*/", slots.len());
            out.push_str(&marker);
            slots.push((marker, comment.to_owned()));
        } else {
            out.push_str(comment);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    (out, slots)
}

fn apply_style_comment_replacements(value: &str, slots: &[(String, String)]) -> Option<String> {
    let mut out = value.to_owned();
    let mut all_found = true;
    for (marker, text) in slots {
        let Some(start) = out.find(marker.as_str()) else {
            all_found = false;
            continue;
        };
        if out[start + marker.len()..].contains(marker.as_str()) {
            all_found = false;
            continue;
        }
        out = format!("{}{}{}", &out[..start], text, &out[start + marker.len()..]);
    }
    all_found.then_some(out)
}
