//! The flat tree builder, ported from prettier-plugin-blade's `tree/tree-builder.ts` and helpers.

use std::collections::{HashMap, HashSet};

use crate::directives::{Role, TreeDirectives};
use crate::lexer::{Token, TokenType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum NodeKind {
    #[default]
    Root,
    Element,
    Text,
    Echo,
    RawEcho,
    TripleEcho,
    Directive,
    DirectiveBlock,
    Verbatim,
    PhpBlock,
    PhpTag,
    Comment,
    BogusComment,
    ConditionalComment,
    BladeComment,
    Doctype,
    Cdata,
    Decl,
    Attribute,
    JsxAttribute,
    UnpairedClosingTag,
    NonOutput,
    ElementName,
    ClosingElementName,
    AttributeWhitespace,
    AttributeName,
    AttributeValue,
    ProcessingInstruction,
    IgnoreRange,
    FrontMatter,
}

pub(crate) const NONE: usize = usize::MAX;

#[derive(Debug, Clone)]
pub(crate) struct FlatNode {
    pub(crate) kind: NodeKind,
    pub(crate) parent: usize,
    pub(crate) first_child: usize,
    pub(crate) last_child: usize,
    pub(crate) next_sibling: usize,
    pub(crate) token_start: usize,
    pub(crate) token_count: usize,
    pub(crate) generic_offset: usize,
    pub(crate) data: usize,
}

fn flat(kind: NodeKind, parent: usize, token_start: usize, token_count: usize) -> FlatNode {
    FlatNode {
        kind,
        parent,
        first_child: NONE,
        last_child: NONE,
        next_sibling: NONE,
        token_start,
        token_count,
        generic_offset: 0,
        data: 0,
    }
}

fn flat_data(
    kind: NodeKind,
    parent: usize,
    token_start: usize,
    token_count: usize,
    data: usize,
) -> FlatNode {
    FlatNode {
        data,
        ..flat(kind, parent, token_start, token_count)
    }
}

// construct-scanner.ts

fn construct_end(t: TokenType) -> Option<TokenType> {
    use TokenType::*;
    match t {
        EchoStart => Some(EchoEnd),
        RawEchoStart => Some(RawEchoEnd),
        TripleEchoStart => Some(TripleEchoEnd),
        PhpTagStart => Some(PhpTagEnd),
        PhpBlockStart => Some(PhpBlockEnd),
        _ => None,
    }
}

fn construct_kind(t: TokenType) -> Option<NodeKind> {
    use TokenType::*;
    match t {
        EchoStart => Some(NodeKind::Echo),
        RawEchoStart => Some(NodeKind::RawEcho),
        TripleEchoStart => Some(NodeKind::TripleEcho),
        PhpTagStart => Some(NodeKind::PhpTag),
        PhpBlockStart => Some(NodeKind::PhpBlock),
        _ => None,
    }
}

fn is_construct_start(t: TokenType) -> bool {
    construct_end(t).is_some()
}

fn scan_construct(tokens: &[Token], mut pos: usize, end: usize) -> (usize, usize) {
    if pos >= end {
        return (pos, 0);
    }
    let Some(end_type) = construct_end(tokens[pos].kind) else {
        return (pos + 1, 1);
    };
    let mut count = 1;
    pos += 1;
    while pos < end && tokens[pos].kind != end_type {
        count += 1;
        pos += 1;
    }
    if pos < end && tokens[pos].kind == end_type {
        count += 1;
        pos += 1;
    }
    (pos, count)
}

fn count_construct_tokens(tokens: &[Token], pos: usize, end: usize) -> usize {
    scan_construct(tokens, pos, end).1
}

fn advance_past(tokens: &[Token], pos: usize, end: usize) -> usize {
    if pos >= end {
        return pos;
    }
    if is_construct_start(tokens[pos].kind) {
        return scan_construct(tokens, pos, end).0;
    }
    pos + 1
}

// optional-tags.ts

struct Closing {
    siblings: Option<&'static [&'static str]>,
    parents: Option<&'static [&'static str]>,
}

const P_SIBLINGS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "div",
    "dl",
    "fieldset",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "main",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "ul",
];

fn closing_conditions(element: &str) -> Option<Closing> {
    let c = |s: &'static [&'static str], p: &'static [&'static str]| Closing {
        siblings: Some(s),
        parents: Some(p),
    };
    Some(match element {
        "li" => c(&["li"], &["ul", "ol", "menu"]),
        "dt" | "dd" => c(&["dt", "dd"], &["dl"]),
        "p" => Closing {
            siblings: Some(P_SIBLINGS),
            parents: None,
        },
        "option" => c(&["option", "optgroup"], &["select", "datalist", "optgroup"]),
        "optgroup" => c(&["optgroup"], &["select"]),
        "rt" | "rp" => c(&["rt", "rp"], &["ruby", "rtc"]),
        "rb" => c(&["rb", "rt", "rp", "rtc"], &["ruby"]),
        "caption" | "colgroup" => c(&["colgroup", "thead", "tbody", "tfoot", "tr"], &["table"]),
        "thead" | "tbody" => c(&["tbody", "tfoot"], &["table"]),
        "tfoot" => Closing {
            siblings: None,
            parents: Some(&["table"]),
        },
        "tr" => c(&["tr"], &["table", "thead", "tbody", "tfoot"]),
        "td" | "th" => c(&["td", "th"], &["tr"]),
        _ => return None,
    })
}

fn is_in_valid_parent_context(element: &str, parent: Option<&str>) -> bool {
    let Some(c) = closing_conditions(element) else {
        return false;
    };
    let has_parent_rules = c.parents.is_some();
    let has_sibling_rules = c.siblings.is_some();
    if (parent == Some(element) || parent.is_none())
        && (has_parent_rules || parent == Some(element))
    {
        return false;
    }
    if let Some(parents) = c.parents {
        return parent.is_some_and(|p| parents.contains(&p));
    }
    has_sibling_rules
}

fn should_auto_close_element(
    element: &str,
    next: Option<&str>,
    parent: Option<&str>,
    at_parent_end: bool,
) -> bool {
    let Some(c) = closing_conditions(element) else {
        return false;
    };
    if let (Some(next), Some(s)) = (next, c.siblings)
        && s.contains(&next)
    {
        return true;
    }
    if at_parent_end && let (Some(p), Some(parents)) = (parent, c.parents) {
        return parents.contains(&p);
    }
    false
}

pub(crate) const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

// argument-scanner.ts

fn arg_skip_ws(s: &[u8], mut pos: usize) -> usize {
    while pos < s.len() && matches!(s[pos], b' ' | b'\t' | b'\n' | b'\r') {
        pos += 1;
    }
    pos
}

fn arg_skip_quoted(s: &[u8], mut pos: usize, quote: u8) -> usize {
    while pos < s.len() {
        if s[pos] == b'\\' {
            pos += 2;
            continue;
        }
        if s[pos] == quote {
            return pos + 1;
        }
        pos += 1;
    }
    pos
}

fn arg_skip_heredoc(s: &[u8], pos: usize) -> usize {
    let mut ident_start = pos;
    while ident_start < s.len() && matches!(s[ident_start], b' ' | b'\t' | b'\n' | b'\r') {
        ident_start += 1;
    }
    let nowdoc = ident_start < s.len() && s[ident_start] == b'\'';
    if nowdoc {
        ident_start += 1;
    }
    let mut ident_end = ident_start;
    while ident_end < s.len() && (s[ident_end].is_ascii_alphanumeric() || s[ident_end] == b'_') {
        ident_end += 1;
    }
    let ident = &s[ident_start..ident_end];
    if ident.is_empty() {
        return pos;
    }
    let mut pos = ident_end;
    if nowdoc && pos < s.len() && s[pos] == b'\'' {
        pos += 1;
    }
    while pos < s.len() && s[pos] != b'\n' {
        pos += 1;
    }
    if pos < s.len() {
        pos += 1;
    }
    while pos < s.len() {
        let mut check = pos;
        while check < s.len() && (s[check] == b' ' || s[check] == b'\t') {
            check += 1;
        }
        if s[check..].starts_with(ident) {
            let after = check + ident.len();
            if after >= s.len() || s[after] == b';' || s[after] == b'\n' {
                return if after < s.len() { after + 1 } else { after };
            }
        }
        while pos < s.len() && s[pos] != b'\n' {
            pos += 1;
        }
        if pos < s.len() {
            pos += 1;
        }
    }
    pos
}

pub(crate) fn count_arguments(args: &str) -> usize {
    let s = args.as_bytes();
    if s.is_empty() {
        return 0;
    }
    let mut pos = arg_skip_ws(s, 0);
    if pos >= s.len() {
        return 0;
    }
    let mut count = 1;
    let mut depth: i32 = 0;
    while pos < s.len() {
        let ch = s[pos];
        if ch == b'"' || ch == b'\'' {
            pos = arg_skip_quoted(s, pos + 1, ch);
            continue;
        }
        if pos + 2 < s.len() && &s[pos..pos + 3] == b"<<<" {
            pos = arg_skip_heredoc(s, pos + 3);
            continue;
        }
        match ch {
            b'(' | b'[' | b'{' => {
                depth += 1;
                pos += 1;
                continue;
            }
            b')' | b']' | b'}' => {
                depth -= 1;
                pos += 1;
                continue;
            }
            b',' if depth == 0 => count += 1,
            _ => {}
        }
        pos += 1;
    }
    count
}

pub(crate) fn starts_with_array(args: &str) -> bool {
    let pos = arg_skip_ws(args.as_bytes(), 0);
    pos < args.len() && args.as_bytes()[pos] == b'['
}

pub(crate) fn unwrap_parentheses(args: &str) -> &str {
    let trimmed = js_trim(args);
    if trimmed.starts_with('(') && trimmed.ends_with(')') && trimmed.len() >= 2 {
        return &trimmed[1..trimmed.len() - 1];
    }
    trimmed
}

/// JavaScript's `String.prototype.trim` (Unicode whitespace and line terminators).
pub(crate) fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_whitespace)
}

pub(crate) fn js_trim_start(s: &str) -> &str {
    s.trim_start_matches(is_js_whitespace)
}

pub(crate) fn js_trim_end(s: &str) -> &str {
    s.trim_end_matches(is_js_whitespace)
}

pub(crate) fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

// rawtext-script-scanner.ts

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScanState {
    Code,
    Single,
    Double,
    Template,
    LineComment,
    BlockComment,
    Regex,
    RegexClass,
}

fn is_ident_part(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c >= 0x80
}

pub(crate) fn script_scan_state(source: &[u8], content_start: usize, offset: usize) -> ScanState {
    let mut state = ScanState::Code;
    let mut template_stack: Vec<i32> = Vec::new();
    let mut i = content_start;
    while i < offset {
        let ch = source[i];
        let next = if i + 1 < offset { source[i + 1] } else { 0 };
        match state {
            ScanState::LineComment => {
                if ch == b'\n' {
                    state = ScanState::Code;
                }
            }
            ScanState::BlockComment => {
                if ch == b'*' && next == b'/' {
                    state = ScanState::Code;
                    i += 1;
                }
            }
            ScanState::Single | ScanState::Double => {
                let q = if state == ScanState::Single {
                    b'\''
                } else {
                    b'"'
                };
                if ch == b'\\' {
                    i += 1;
                } else if ch == q {
                    state = ScanState::Code;
                }
            }
            ScanState::Template => {
                if ch == b'\\' {
                    i += 1;
                } else if ch == b'$' && next == b'{' {
                    template_stack.push(1);
                    state = ScanState::Code;
                    i += 1;
                } else if ch == b'`' {
                    state = ScanState::Code;
                }
            }
            ScanState::Regex => {
                if ch == b'\\' {
                    i += 1;
                } else if ch == b'[' {
                    state = ScanState::RegexClass;
                } else if ch == b'/' {
                    state = ScanState::Code;
                }
            }
            ScanState::RegexClass => {
                if ch == b'\\' {
                    i += 1;
                } else if ch == b']' {
                    state = ScanState::Regex;
                }
            }
            ScanState::Code => {
                if let Some(top) = template_stack.last_mut() {
                    if ch == b'{' {
                        *top += 1;
                        i += 1;
                        continue;
                    }
                    if ch == b'}' {
                        *top -= 1;
                        if *top == 0 {
                            template_stack.pop();
                            state = ScanState::Template;
                        }
                        i += 1;
                        continue;
                    }
                }
                if ch == b'/' && next == b'/' {
                    state = ScanState::LineComment;
                    i += 1;
                } else if ch == b'/' && next == b'*' {
                    state = ScanState::BlockComment;
                    i += 1;
                } else if ch == b'/' && is_likely_regex_start(source, content_start, i) {
                    state = ScanState::Regex;
                } else if ch == b'\'' {
                    state = ScanState::Single;
                } else if ch == b'"' {
                    state = ScanState::Double;
                } else if ch == b'`' {
                    state = ScanState::Template;
                }
            }
        }
        i += 1;
    }
    state
}

pub(crate) fn is_likely_inside_script_literal_or_comment(
    source: &[u8],
    content_start: usize,
    offset: usize,
) -> bool {
    script_scan_state(source, content_start, offset) != ScanState::Code
}

fn prev_ident_token(source: &[u8], content_start: usize, index: usize) -> Option<&[u8]> {
    if index < content_start || !is_ident_part(source[index]) {
        return None;
    }
    let mut start = index;
    while start > content_start && is_ident_part(source[start - 1]) {
        start -= 1;
    }
    Some(&source[start..=index])
}

fn is_regex_after_condition_paren(source: &[u8], content_start: usize, close: usize) -> bool {
    let mut depth = 0;
    let mut open = None;
    let mut i = close as isize;
    while i >= content_start as isize {
        let ch = source[i as usize];
        if ch == b')' {
            depth += 1;
        } else if ch == b'(' {
            depth -= 1;
            if depth == 0 {
                open = Some(i as usize);
                break;
            }
        }
        i -= 1;
    }
    let Some(open) = open else {
        return false;
    };
    let mut i = open as isize - 1;
    while i >= content_start as isize {
        let ch = source[i as usize];
        if matches!(ch, b' ' | b'\t' | b'\r' | b'\n') {
            i -= 1;
            continue;
        }
        return prev_ident_token(source, content_start, i as usize)
            .is_some_and(|t| matches!(t, b"if" | b"while" | b"for" | b"with" | b"catch"));
    }
    false
}

fn is_likely_regex_start(source: &[u8], content_start: usize, slash: usize) -> bool {
    let mut i = slash as isize - 1;
    while i >= content_start as isize {
        let ch = source[i as usize];
        if matches!(ch, b' ' | b'\t' | b'\r' | b'\n') {
            i -= 1;
            continue;
        }
        if matches!(ch, b'\'' | b'"' | b'`') {
            return false;
        }
        if is_ident_part(ch) {
            return prev_ident_token(source, content_start, i as usize).is_some_and(|t| {
                matches!(
                    t,
                    b"return"
                        | b"throw"
                        | b"do"
                        | b"case"
                        | b"delete"
                        | b"void"
                        | b"typeof"
                        | b"instanceof"
                        | b"in"
                        | b"of"
                        | b"yield"
                        | b"await"
                        | b"else"
                )
            });
        }
        if ch == b')' {
            return is_regex_after_condition_paren(source, content_start, i as usize);
        }
        if ch == b'+' || ch == b'-' {
            let mut j = i - 1;
            while j >= content_start as isize
                && matches!(source[j as usize], b' ' | b'\t' | b'\r' | b'\n')
            {
                j -= 1;
            }
            return !(j >= content_start as isize && source[j as usize] == ch);
        }
        if ch == b']' || ch == b'}' {
            return false;
        }
        return true;
    }
    true
}

// malformed-tags.ts

pub(crate) fn has_swallowed_tag_syntax(opening: &str) -> bool {
    let Some(nl) = opening.find(['\r', '\n']) else {
        return false;
    };
    let b = &opening.as_bytes()[nl..];
    (0..b.len()).any(|i| {
        b[i] == b'<' && i + 1 < b.len() && b[i + 1] != b'/' && b[i + 1].is_ascii_alphabetic()
    })
}

// directive-token-index.ts

struct DirectiveTokenIndex {
    by_name: HashMap<String, Vec<usize>>,
    all_positions: Vec<usize>,
    all_names: Vec<String>,
    has_args: HashMap<usize, bool>,
}

fn check_directive_args_fast(
    tokens: &[Token],
    start: usize,
    end: usize,
) -> (bool, Option<usize>, usize) {
    let mut consumed = 0;
    let mut i = start;
    if i < end
        && tokens[i].kind == TokenType::Whitespace
        && i + 1 < end
        && tokens[i + 1].kind == TokenType::DirectiveArgs
    {
        consumed += 1;
        i += 1;
    }
    if i < end && tokens[i].kind == TokenType::DirectiveArgs {
        return (true, Some(i), consumed + 1);
    }
    (false, None, consumed)
}

fn bsearch_gte(arr: &[usize], target: usize) -> Option<usize> {
    let i = arr.partition_point(|&x| x < target);
    if i < arr.len() { Some(i) } else { None }
}

impl DirectiveTokenIndex {
    fn new(tokens: &[Token], source: &str, include: impl Fn(usize) -> bool) -> Self {
        let mut idx = DirectiveTokenIndex {
            by_name: HashMap::new(),
            all_positions: Vec::new(),
            all_names: Vec::new(),
            has_args: HashMap::new(),
        };
        for (i, t) in tokens.iter().enumerate() {
            if t.kind != TokenType::Directive || !include(i) {
                continue;
            }
            let name = directive_name(t, source);
            idx.all_positions.push(i);
            idx.all_names.push(name.clone());
            idx.has_args
                .insert(i, check_directive_args_fast(tokens, i + 1, tokens.len()).0);
            idx.by_name.entry(name).or_default().push(i);
        }
        idx
    }

    fn exists_after(&self, name: &str, min: usize) -> bool {
        self.by_name
            .get(&name.to_ascii_lowercase())
            .is_some_and(|p| p.last().is_some_and(|&l| l >= min))
    }

    fn exists_between(&self, name: &str, min: usize, max: usize) -> bool {
        let Some(p) = self.by_name.get(&name.to_ascii_lowercase()) else {
            return false;
        };
        bsearch_gte(p, min).is_some_and(|i| p[i] < max)
    }

    fn has_args(&self, pos: usize) -> bool {
        self.has_args.get(&pos) == Some(&true)
    }

    fn find_matching_terminator(
        &self,
        openers: &[String],
        start: usize,
        terminators: &[String],
        max: Option<usize>,
        initial_nesting: usize,
    ) -> Option<usize> {
        let first = bsearch_gte(&self.all_positions, start)?;
        let openers: HashSet<String> = openers.iter().map(|s| s.to_ascii_lowercase()).collect();
        let terms: HashSet<String> = terminators.iter().map(|s| s.to_ascii_lowercase()).collect();
        let mut nesting = initial_nesting;
        for i in first..self.all_positions.len() {
            if max.is_some_and(|m| self.all_positions[i] >= m) {
                return None;
            }
            let name = &self.all_names[i];
            if openers.contains(name) {
                nesting += 1;
                continue;
            }
            if !terms.contains(name) {
                continue;
            }
            if nesting == 0 {
                return Some(self.all_positions[i]);
            }
            nesting -= 1;
        }
        None
    }

    fn find_matching_boundary(
        &self,
        openers: &[String],
        start: usize,
        terminators: &[String],
        branches: &[String],
        initial_nesting: usize,
        should_count: &dyn Fn(usize, &str) -> bool,
    ) -> Option<usize> {
        let first = bsearch_gte(&self.all_positions, start)?;
        let openers: HashSet<String> = openers.iter().map(|s| s.to_ascii_lowercase()).collect();
        let terms: HashSet<String> = terminators.iter().map(|s| s.to_ascii_lowercase()).collect();
        let branches: HashSet<String> = branches.iter().map(|s| s.to_ascii_lowercase()).collect();
        let mut nesting = initial_nesting;
        for i in first..self.all_positions.len() {
            let name = &self.all_names[i];
            if openers.contains(name) {
                nesting += 1;
                continue;
            }
            if branches.contains(name) {
                if nesting == 0 {
                    let pos = self.all_positions[i];
                    if should_count(pos, name) {
                        return Some(pos);
                    }
                }
                continue;
            }
            if !terms.contains(name) {
                continue;
            }
            if nesting == 0 {
                return Some(self.all_positions[i]);
            }
            nesting -= 1;
        }
        None
    }

    fn analyze_unknown_family(
        &self,
        openers: &[String],
        start: usize,
        terminators: &[String],
        branches: &[String],
        max: Option<usize>,
    ) -> (Option<usize>, Option<String>, Option<usize>) {
        let openers: HashSet<String> = openers.iter().map(|s| s.to_ascii_lowercase()).collect();
        let terms: HashSet<String> = terminators.iter().map(|s| s.to_ascii_lowercase()).collect();
        let branches: HashSet<String> = branches.iter().map(|s| s.to_ascii_lowercase()).collect();
        let Some(first) = bsearch_gte(&self.all_positions, start) else {
            return (None, None, None);
        };
        let mut nesting = 0;
        let mut branch_idx = None;
        for i in first..self.all_positions.len() {
            if max.is_some_and(|m| self.all_positions[i] >= m) {
                return (None, None, None);
            }
            let name = &self.all_names[i];
            if openers.contains(name) {
                nesting += 1;
                continue;
            }
            if terms.contains(name) {
                if nesting == 0 {
                    return (Some(self.all_positions[i]), Some(name.clone()), branch_idx);
                }
                nesting -= 1;
                continue;
            }
            if branches.contains(name) && nesting == 0 && branch_idx.is_none() {
                branch_idx = Some(self.all_positions[i]);
            }
        }
        (None, None, None)
    }
}

pub(crate) fn directive_name(t: &Token, source: &str) -> String {
    let text = &source[t.start..t.end];
    text.strip_prefix('@').unwrap_or(text).to_ascii_lowercase()
}

// tree-builder.ts

#[derive(Clone)]
struct DirectiveFrame {
    block_idx: usize,
    start_directive_idx: usize,
    name: String,
    element_stack_base: usize,
    terminators: Vec<String>,
    branches: Vec<String>,
    openers: Vec<String>,
}

#[derive(Clone)]
struct ConditionFrame {
    block_idx: usize,
    current_branch_idx: usize,
    name: String,
    element_stack_base: usize,
}

struct SwitchFrame {
    block_idx: usize,
    switch_directive_idx: usize,
    current_case_idx: Option<usize>,
    name: String,
    element_stack_base: usize,
}

struct Family {
    else_name: String,
    end_name: String,
    openers: Vec<String>,
    has_advisory_condition: bool,
}

pub(crate) struct TreeBuilder<'a> {
    tokens: &'a [Token],
    source: &'a str,
    directives: &'a TreeDirectives,
    pub(crate) nodes: Vec<FlatNode>,
    open_elements: Vec<usize>,
    open_directives: Vec<DirectiveFrame>,
    open_conditions: Vec<ConditionFrame>,
    open_switches: Vec<SwitchFrame>,
    open_php_tag_conditions: Vec<ConditionFrame>,
    pos: usize,
    tag_names: HashMap<usize, String>,
    raw_tag_names: HashMap<usize, String>,
    dynamic_tag_names: HashSet<usize>,
    tag_name_stacks: HashMap<String, Vec<usize>>,
    malformed_raw_element_indexes: HashSet<usize>,
    malformed_raw_element_sources: Vec<(String, usize)>,
    directive_index: Option<DirectiveTokenIndex>,
    script_literal_directives: Option<HashSet<usize>>,
    attribute_literal_directives: Option<HashSet<usize>>,
    attribute_region_end: Option<usize>,
    family_cache: HashMap<String, Option<std::rc::Rc<Family>>>,
}

fn php_tag_shorthand(source: &str) -> Option<(&'static str, &'static str, &'static str)> {
    // (role, name, context)
    let lower = source.to_ascii_lowercase();
    let rest = lower.strip_prefix("<?php")?.strip_suffix("?>")?;
    let body = js_trim(rest);
    if body.is_empty() {
        return None;
    }
    let word_then = |kw: &str| -> Option<&str> {
        let r = body.strip_prefix(kw)?;
        if r.chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric() || c == '_')
        {
            return None;
        }
        Some(r)
    };
    let ends_colon = |s: &str| js_trim_end(s).ends_with(':');
    let only = |s: &str, tail: &[&str]| {
        let t = js_trim(s);
        tail.contains(&t)
    };
    for (kw, role, ctx) in [("if", "open", "if"), ("elseif", "branch", "if")] {
        if let Some(r) = word_then(kw)
            && ends_colon(r)
        {
            return Some((role, kw, ctx));
        }
    }
    if let Some(r) = body.strip_prefix("else")
        && only(r, &[":"])
    {
        return Some(("branch", "else", "if"));
    }
    if let Some(r) = body.strip_prefix("endif")
        && (only(r, &["", ";"]))
    {
        return Some(("close", "endif", "if"));
    }
    if let Some(r) = word_then("switch")
        && ends_colon(r)
    {
        return Some(("open", "switch", "switch"));
    }
    if let Some(r) = word_then("case")
        && ends_colon(r)
    {
        return Some(("branch", "case", "switch"));
    }
    if let Some(r) = body.strip_prefix("default")
        && only(r, &[":"])
    {
        return Some(("branch", "default", "switch"));
    }
    if let Some(r) = body.strip_prefix("endswitch")
        && only(r, &["", ";"])
    {
        return Some(("close", "endswitch", "switch"));
    }
    for (open, close) in [
        ("for", "endfor"),
        ("foreach", "endforeach"),
        ("while", "endwhile"),
        ("declare", "enddeclare"),
    ] {
        if let Some(r) = word_then(open)
            && ends_colon(r)
        {
            return Some(("open", open, open));
        }
        if let Some(r) = body.strip_prefix(close)
            && only(r, &["", ";"])
        {
            return Some(("close", close, open));
        }
    }
    None
}

impl<'a> TreeBuilder<'a> {
    pub(crate) fn new(
        tokens: &'a [Token],
        source: &'a str,
        directives: &'a TreeDirectives,
    ) -> Self {
        TreeBuilder {
            tokens,
            source,
            directives,
            nodes: Vec::new(),
            open_elements: Vec::new(),
            open_directives: Vec::new(),
            open_conditions: Vec::new(),
            open_switches: Vec::new(),
            open_php_tag_conditions: Vec::new(),
            pos: 0,
            tag_names: HashMap::new(),
            raw_tag_names: HashMap::new(),
            dynamic_tag_names: HashSet::new(),
            tag_name_stacks: HashMap::new(),
            malformed_raw_element_indexes: HashSet::new(),
            malformed_raw_element_sources: Vec::new(),
            directive_index: None,
            script_literal_directives: None,
            attribute_literal_directives: None,
            attribute_region_end: None,
            family_cache: HashMap::new(),
        }
    }

    pub(crate) fn build(mut self) -> Vec<FlatNode> {
        self.nodes.push(flat(NodeKind::Root, NONE, 0, 0));
        self.open_elements.push(0);
        while self.pos < self.tokens.len() {
            self.process_token();
        }
        self.close_remaining_directives();
        self.close_remaining_elements();
        self.nodes
    }

    fn kind_at(&self, i: usize) -> TokenType {
        self.tokens[i].kind
    }

    fn process_token(&mut self) {
        use TokenType::*;
        match self.kind_at(self.pos) {
            LessThan => self.process_element_start(),
            Text | TagName | Whitespace => self.process_text(),
            AtSign => {
                let start = self.pos;
                self.pos += 1;
                self.add_child(flat(NodeKind::NonOutput, 0, start, 1));
            }
            EchoStart => self.process_echo_span(EchoEnd, NodeKind::Echo),
            RawEchoStart => self.process_echo_span(RawEchoEnd, NodeKind::RawEcho),
            TripleEchoStart => self.process_echo_span(TripleEchoEnd, NodeKind::TripleEcho),
            Directive => self.process_directive(),
            VerbatimStart => self.create_block_node(VerbatimEnd, NodeKind::Verbatim),
            BladeCommentStart => self.create_block_node(BladeCommentEnd, NodeKind::BladeComment),
            CommentStart => self.create_block_node(CommentEnd, NodeKind::Comment),
            BogusComment => {
                self.add_child(flat_data(NodeKind::BogusComment, 0, self.pos, 1, 1));
                self.pos += 1;
            }
            DoctypeStart => self.create_block_node(DoctypeEnd, NodeKind::Doctype),
            IgnoreRange => {
                self.add_child(flat(NodeKind::IgnoreRange, 0, self.pos, 1));
                self.pos += 1;
            }
            PhpBlockStart => self.create_block_node(PhpBlockEnd, NodeKind::PhpBlock),
            PhpTagStart => self.process_php_tag(),
            PhpBlockEnd => {
                let start = self.pos;
                self.pos += 1;
                self.add_child(flat(NodeKind::Directive, 0, start, 1));
            }
            ConditionalCommentStart => self.create_conditional_comment(),
            CdataStart => self.create_block_node(CdataEnd, NodeKind::Cdata),
            DeclStart => self.create_decl(),
            PIStart => self.create_block_node(PIEnd, NodeKind::ProcessingInstruction),
            SyntheticClose => self.pos += 1,
            _ => self.emit_single_token_text(),
        }
    }

    fn add_child(&mut self, mut node: FlatNode) -> usize {
        self.implicitly_close_void_elements();
        let parent = self.current_parent();
        node.parent = parent;
        let idx = self.nodes.len();
        self.nodes.push(node);
        if self.nodes[parent].first_child == NONE {
            self.nodes[parent].first_child = idx;
        } else {
            let last = self.nodes[parent].last_child;
            self.nodes[last].next_sibling = idx;
        }
        self.nodes[parent].last_child = idx;
        idx
    }

    fn current_parent(&self) -> usize {
        self.open_elements.last().copied().unwrap_or(0)
    }

    fn create_first_child(
        &mut self,
        parent: usize,
        kind: NodeKind,
        start: usize,
        count: usize,
    ) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(flat(kind, parent, start, count));
        self.nodes[parent].first_child = idx;
        self.nodes[parent].last_child = idx;
        idx
    }

    fn create_sibling_child(
        &mut self,
        prev: usize,
        parent: usize,
        kind: NodeKind,
        start: usize,
        count: usize,
    ) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(flat(kind, parent, start, count));
        self.nodes[prev].next_sibling = idx;
        self.nodes[parent].last_child = idx;
        idx
    }

    fn link_child(&mut self, parent: usize, last: &mut usize, node: FlatNode) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(node);
        if *last == NONE {
            self.nodes[parent].first_child = idx;
        } else {
            self.nodes[*last].next_sibling = idx;
        }
        self.nodes[parent].last_child = idx;
        *last = idx;
        idx
    }

    fn pop_if_top(&mut self, idx: usize) {
        if self.open_elements.last() == Some(&idx) {
            let popped = self.open_elements.pop().unwrap();
            self.cleanup_tag_name_stack(popped);
        }
    }

    fn pop_elements_to_depth(&mut self, depth: usize) {
        while self.open_elements.len() > depth {
            let popped = self.open_elements.pop().unwrap();
            self.cleanup_tag_name_stack(popped);
        }
    }

    fn cleanup_tag_name_stack(&mut self, idx: usize) {
        let tag = self.tag_names.remove(&idx);
        self.raw_tag_names.remove(&idx);
        self.dynamic_tag_names.remove(&idx);
        let Some(tag) = tag else {
            return;
        };
        if let Some(stack) = self.tag_name_stacks.get_mut(&tag) {
            stack.pop();
            if stack.is_empty() {
                self.tag_name_stacks.remove(&tag);
            }
        }
    }

    fn implicitly_close_void_elements(&mut self) {
        while self.open_elements.len() > 1 {
            let top = *self.open_elements.last().unwrap();
            let Some(tag) = self.tag_names.get(&top).cloned() else {
                break;
            };
            if !VOID_ELEMENTS.contains(&tag.as_str()) {
                break;
            }
            self.open_elements.pop();
            if let Some(stack) = self.tag_name_stacks.get_mut(&tag) {
                stack.pop();
                if stack.is_empty() {
                    self.tag_name_stacks.remove(&tag);
                }
            }
        }
    }

    fn auto_close_elements_for_sibling(&mut self, new_tag: &str, close_at: usize) {
        if new_tag.is_empty() {
            return;
        }
        if let Some(c) = closing_conditions(new_tag)
            && let Some(parents) = c.parents
        {
            if let Some(container) = self.find_nearest_valid_container(parents) {
                self.auto_close_elements_between(container, new_tag, close_at);
            }
            return;
        }
        self.auto_close_immediate_parent(new_tag, close_at);
    }

    fn auto_close_elements_between(&mut self, container: usize, new_tag: &str, close_at: usize) {
        let mut close_up_to = None;
        let mut i = self.open_elements.len() - 1;
        while i > container {
            let el = self.open_elements[i];
            if self.nodes[el].kind == NodeKind::Element {
                let tag = self.tag_names.get(&el).cloned().unwrap_or_default();
                if !tag.is_empty() {
                    let parent_tag = self.parent_tag_name_of(el);
                    if should_auto_close_element(&tag, Some(new_tag), parent_tag.as_deref(), false)
                    {
                        close_up_to = Some(i);
                    }
                }
            }
            i -= 1;
        }
        if let Some(up_to) = close_up_to {
            while self.open_elements.len() > up_to {
                let popped = self.open_elements.pop().unwrap();
                self.finalize_implicitly_closed(popped, close_at);
                self.cleanup_tag_name_stack(popped);
            }
        }
    }

    fn auto_close_immediate_parent(&mut self, new_tag: &str, close_at: usize) {
        if self.open_elements.len() <= 1 {
            return;
        }
        let current = *self.open_elements.last().unwrap();
        if self.nodes[current].kind != NodeKind::Element {
            return;
        }
        let tag = self.tag_names.get(&current).cloned().unwrap_or_default();
        if tag.is_empty() {
            return;
        }
        let parent_tag = self.parent_tag_name_of(current);
        if !is_in_valid_parent_context(&tag, parent_tag.as_deref()) {
            return;
        }
        if should_auto_close_element(&tag, Some(new_tag), parent_tag.as_deref(), false) {
            self.open_elements.pop();
            self.finalize_implicitly_closed(current, close_at);
            self.cleanup_tag_name_stack(current);
        }
    }

    fn finalize_implicitly_closed(&mut self, el: usize, close_at: usize) {
        if self.nodes[el].kind != NodeKind::Element {
            return;
        }
        let count = close_at.saturating_sub(self.nodes[el].token_start);
        if count > self.nodes[el].token_count {
            self.nodes[el].token_count = count;
        }
    }

    fn find_nearest_valid_container(&self, valid: &[&str]) -> Option<usize> {
        let mut i = self.open_elements.len() - 1;
        while i > 0 {
            let el = self.open_elements[i];
            if self.nodes[el].kind == NodeKind::Element {
                let tag = self.tag_names.get(&el).map_or("", String::as_str);
                if !tag.is_empty() {
                    if valid.contains(&tag) {
                        return Some(i);
                    }
                    if tag == "template" {
                        return None;
                    }
                }
            }
            i -= 1;
        }
        None
    }

    fn parent_tag_name_of(&self, idx: usize) -> Option<String> {
        let parent = self.nodes[idx].parent;
        if parent == NONE || parent == 0 || parent >= self.nodes.len() {
            return None;
        }
        if self.nodes[parent].kind != NodeKind::Element {
            return None;
        }
        self.tag_names.get(&parent).cloned()
    }

    fn close_remaining_elements(&mut self) {
        let total = self.tokens.len();
        for &el in &self.open_elements {
            if el == 0 {
                continue;
            }
            self.nodes[el].token_count = total - self.nodes[el].token_start;
        }
    }

    fn process_text(&mut self) {
        let parent = self.current_parent();
        let last = self.nodes[parent].last_child;
        if last != NONE && self.nodes[last].kind == NodeKind::Text {
            self.nodes[last].token_count += 1;
            self.pos += 1;
            return;
        }
        let start = self.pos;
        self.pos += 1;
        self.add_child(flat(NodeKind::Text, 0, start, 1));
    }

    fn emit_single_token_text(&mut self) {
        let start = self.pos;
        self.pos += 1;
        self.add_child(flat(NodeKind::Text, 0, start, 1));
    }

    fn process_echo_span(&mut self, end: TokenType, kind: NodeKind) {
        use TokenType::*;
        let start = self.pos;
        self.pos += 1;
        while self.pos < self.tokens.len() {
            let t = self.kind_at(self.pos);
            if t == end {
                self.pos += 1;
                break;
            }
            if matches!(
                t,
                EchoStart | RawEchoStart | TripleEchoStart | BladeCommentStart
            ) {
                self.add_child(flat(NodeKind::Text, 0, start, self.pos - start));
                return;
            }
            self.pos += 1;
        }
        self.add_child(flat(kind, 0, start, self.pos - start));
    }

    fn create_block_node(&mut self, end: TokenType, kind: NodeKind) {
        let start = self.pos;
        let mut end_pos = self.pos;
        let mut closing = false;
        while end_pos < self.tokens.len() {
            if self.kind_at(end_pos) == end {
                closing = true;
                end_pos += 1;
                break;
            }
            end_pos += 1;
        }
        self.add_child(flat_data(
            kind,
            0,
            start,
            end_pos - start,
            usize::from(closing),
        ));
        self.pos = end_pos;
    }

    fn php_tag_token_count(&self, start: usize) -> (usize, bool) {
        let mut end = start;
        while end < self.tokens.len() {
            if self.kind_at(end) == TokenType::PhpTagEnd {
                return (end + 1 - start, true);
            }
            end += 1;
        }
        (end - start, false)
    }

    fn add_standalone_php_tag(&mut self, start: usize, count: usize, closing: bool) {
        self.add_child(flat_data(
            NodeKind::PhpTag,
            0,
            start,
            count,
            usize::from(closing),
        ));
        self.pos += count;
    }

    fn process_php_tag(&mut self) {
        let start = self.pos;
        let (count, closing) = self.php_tag_token_count(start);
        if count == 0 {
            self.pos += 1;
            return;
        }
        let raw = &self.source[self.tokens[start].start..self.tokens[start + count - 1].end];
        let Some((role, name, ctx)) = php_tag_shorthand(raw) else {
            self.add_standalone_php_tag(start, count, closing);
            return;
        };
        match role {
            "open" => {
                let block = self.add_child(flat(NodeKind::DirectiveBlock, 0, start, count));
                let base = self.open_elements.len();
                self.open_elements.push(block);
                let branch = self.add_child(flat_data(
                    NodeKind::PhpTag,
                    0,
                    start,
                    count,
                    usize::from(closing),
                ));
                self.open_elements.push(branch);
                self.open_php_tag_conditions.push(ConditionFrame {
                    block_idx: block,
                    current_branch_idx: branch,
                    name: ctx.to_string(),
                    element_stack_base: base,
                });
                self.pos += count;
            }
            "branch" => {
                let Some(frame) = self.open_php_tag_conditions.last().cloned() else {
                    self.add_standalone_php_tag(start, count, closing);
                    return;
                };
                let matching = (frame.name == "if" && (name == "elseif" || name == "else"))
                    || (frame.name == "switch" && (name == "case" || name == "default"));
                if !matching {
                    self.add_standalone_php_tag(start, count, closing);
                    return;
                }
                self.pop_if_top(frame.current_branch_idx);
                self.pop_elements_to_depth(frame.element_stack_base + 1);
                let branch = self.add_child(flat_data(
                    NodeKind::PhpTag,
                    0,
                    start,
                    count,
                    usize::from(closing),
                ));
                self.open_elements.push(branch);
                self.open_php_tag_conditions
                    .last_mut()
                    .unwrap()
                    .current_branch_idx = branch;
                self.pos += count;
            }
            _ => {
                let Some(frame) = self.open_php_tag_conditions.last().cloned() else {
                    self.add_standalone_php_tag(start, count, closing);
                    return;
                };
                let expected = match frame.name.as_str() {
                    "if" => "endif",
                    "switch" => "endswitch",
                    "for" => "endfor",
                    "foreach" => "endforeach",
                    "while" => "endwhile",
                    _ => "enddeclare",
                };
                if name != expected {
                    self.add_standalone_php_tag(start, count, closing);
                    return;
                }
                self.open_php_tag_conditions.pop();
                self.pop_elements_to_depth(frame.element_stack_base + 1);
                self.pop_if_top(frame.current_branch_idx);
                self.add_child(flat_data(
                    NodeKind::PhpTag,
                    0,
                    start,
                    count,
                    usize::from(closing),
                ));
                self.pop_if_top(frame.block_idx);
                let end = start + count;
                self.nodes[frame.block_idx].token_count =
                    end - self.nodes[frame.block_idx].token_start;
                self.pos += count;
            }
        }
    }

    fn create_conditional_comment(&mut self) {
        let start = self.pos;
        let total = self.tokens.len();
        let mut end = start + 1;
        let mut closing = false;
        while end < total {
            if self.kind_at(end) == TokenType::ConditionalCommentEnd {
                closing = true;
                break;
            }
            end += 1;
        }
        let full_end = if closing { end + 1 } else { end };
        let idx = self.add_child(flat_data(
            NodeKind::ConditionalComment,
            0,
            start,
            full_end - start,
            usize::from(closing),
        ));
        let depth = self.open_elements.len();
        self.open_elements.push(idx);
        self.pos = start + 1;
        while self.pos < end {
            self.process_token();
        }
        while self.open_elements.len() > depth + 1 {
            let p = self.open_elements.pop().unwrap();
            self.cleanup_tag_name_stack(p);
        }
        let p = self.open_elements.pop().unwrap();
        self.cleanup_tag_name_stack(p);
        self.pos = full_end;
    }

    fn create_decl(&mut self) {
        use TokenType::*;
        let total = self.tokens.len();
        let start = self.pos;
        let mut end = self.pos;
        let mut closing = false;
        while end < total {
            if self.kind_at(end) == DeclEnd {
                closing = true;
                break;
            }
            end += 1;
        }
        let count = if closing {
            end + 1 - start
        } else {
            end - start
        };
        let decl = self.add_child(flat_data(
            NodeKind::Decl,
            0,
            start,
            count,
            usize::from(closing),
        ));
        self.open_elements.push(decl);
        self.pos += 1;
        while self.pos < total {
            let t = self.kind_at(self.pos);
            if t == DeclEnd {
                self.pos += 1;
                break;
            }
            if t == Whitespace {
                self.add_child(flat(NodeKind::AttributeWhitespace, 0, self.pos, 1));
                self.pos += 1;
                continue;
            }
            if matches!(t, EchoStart | RawEchoStart | TripleEchoStart) {
                let s = self.pos;
                let kind = construct_kind(t).unwrap_or(NodeKind::Echo);
                let n = count_construct_tokens(self.tokens, s, total);
                self.pos += n;
                self.add_child(flat(kind, 0, s, n));
                continue;
            }
            if matches!(
                t,
                AttributeName | BoundAttribute | EscapedAttribute | ShorthandAttribute
            ) {
                self.build_decl_attribute(end);
                continue;
            }
            self.pos += 1;
        }
        self.open_elements.pop();
    }

    fn build_decl_attribute(&mut self, attr_end: usize) {
        use TokenType::*;
        let total = self.tokens.len();
        let attr_start = self.pos;
        let mut name_count = 0;
        while self.pos < total && self.pos < attr_end {
            let t = self.kind_at(self.pos);
            if matches!(
                t,
                AttributeName | EchoStart | RawEchoStart | TripleEchoStart
            ) {
                name_count += 1;
                self.pos += 1;
                continue;
            }
            break;
        }
        if name_count == 0 {
            self.pos += 1;
            return;
        }
        let mut has_value = false;
        let mut value_start = 0;
        let mut value_count = 0;
        let mut check = self.pos;
        if check < total && self.kind_at(check) == Whitespace {
            check += 1;
        }
        if check < total && self.kind_at(check) == Equals {
            self.pos = check + 1;
            if self.pos < total && self.kind_at(self.pos) == Whitespace {
                self.pos += 1;
            }
            if self.pos < total {
                value_start = self.pos;
                value_count = 0;
                if self.kind_at(self.pos) == Quote {
                    value_count = 1;
                    self.pos += 1;
                    while self.pos < total && self.pos < attr_end {
                        let t = self.kind_at(self.pos);
                        if t == Quote {
                            value_count += 1;
                            self.pos += 1;
                            break;
                        }
                        if t == DeclEnd {
                            break;
                        }
                        value_count += 1;
                        self.pos += 1;
                    }
                }
                has_value = value_count > 0;
            }
        }
        let attr_count = self.pos - attr_start;
        let attr = self.add_child(flat(NodeKind::Attribute, 0, attr_start, attr_count));
        let name = self.create_first_child(attr, NodeKind::AttributeName, attr_start, name_count);
        self.build_decl_attribute_parts(attr_start, name_count, name, false);
        if has_value && value_count > 0 {
            let value = self.create_sibling_child(
                name,
                attr,
                NodeKind::AttributeValue,
                value_start,
                value_count,
            );
            self.build_decl_attribute_parts(value_start, value_count, value, true);
        }
    }

    fn build_decl_attribute_parts(
        &mut self,
        start: usize,
        count: usize,
        parent: usize,
        is_value: bool,
    ) {
        use TokenType::*;
        let mut last = NONE;
        for i in 0..count {
            let pos = start + i;
            let t = self.kind_at(pos);
            if is_value && matches!(t, Quote | EchoEnd | RawEchoEnd | TripleEchoEnd) {
                continue;
            }
            let kind = match t {
                EchoStart => NodeKind::Echo,
                RawEchoStart => NodeKind::RawEcho,
                TripleEchoStart => NodeKind::TripleEcho,
                _ => NodeKind::Text,
            };
            self.link_child(parent, &mut last, flat(kind, parent, pos, 1));
        }
    }

    fn process_element_start(&mut self) {
        use TokenType::*;
        let total = self.tokens.len();
        let start = self.pos;
        self.pos += 1;
        if self.pos < total && self.kind_at(self.pos) == Slash {
            self.process_element_end();
            return;
        }
        let name_start = self.pos;
        let (name_count, generic_offset) = self.scan_element_name();
        let raw_tag = self.raw_name_text(name_start, name_count);
        let dynamic = self.is_dynamic_tag_name(name_start, name_count);
        if self.pos < total && self.kind_at(self.pos) == TsxGenericType {
            self.pos += 1;
        }
        if name_count == 0 {
            self.add_child(flat(NodeKind::Text, 0, start, 1));
            return;
        }
        let static_name = self.tag_name_text(name_start, name_count);
        let lower = static_name.to_ascii_lowercase();
        if !dynamic {
            self.auto_close_elements_for_sibling(&lower, start);
        }
        let el = self.add_child(flat(NodeKind::Element, 0, start, 0));
        let name_idx = self.create_first_child(el, NodeKind::ElementName, name_start, name_count);
        self.build_element_name_parts(name_start, name_count, name_idx);
        self.build_attributes(el);
        if self.has_swallowed_tag_syntax_in_opening(start, self.pos) {
            self.malformed_raw_element_indexes.insert(el);
            self.malformed_raw_element_sources
                .push((lower.clone(), self.tokens[start].start));
        }
        let mut self_closing = false;
        let mut synthetic = false;
        if self.pos < total && self.kind_at(self.pos) == Slash {
            self.pos += 1;
            self_closing = self.pos < total && self.kind_at(self.pos) == GreaterThan;
        }
        if self.pos < total {
            match self.kind_at(self.pos) {
                GreaterThan => self.pos += 1,
                SyntheticClose => {
                    self.pos += 1;
                    synthetic = true;
                }
                _ => {}
            }
        }
        self.nodes[el].token_count = self.pos - start;
        self.nodes[el].generic_offset = if generic_offset > 0 {
            generic_offset + 1
        } else {
            0
        };
        self.nodes[el].data = usize::from(self_closing);
        let mut stay_open = !self_closing;
        if synthetic && stay_open && (self.pos >= total || self.kind_at(self.pos) == LessThan) {
            stay_open = false;
        }
        if !stay_open {
            return;
        }
        self.open_elements.push(el);
        self.tag_names.insert(el, lower.clone());
        self.raw_tag_names.insert(el, raw_tag);
        if dynamic {
            self.dynamic_tag_names.insert(el);
        }
        if !lower.is_empty() && !dynamic {
            let depth = self.open_elements.len() - 1;
            self.tag_name_stacks.entry(lower).or_default().push(depth);
        }
    }

    fn process_element_end(&mut self) {
        use TokenType::*;
        let total = self.tokens.len();
        let start = self.pos - 1;
        self.pos += 1;
        let name_start = self.pos;
        let (mut name_count, _) = self.scan_element_name();
        if name_count == 0 && self.pos < total && self.kind_at(self.pos) == AttributeName {
            name_count = 1;
            self.pos += 1;
        }
        let closing_name = if name_count > 0 {
            self.tag_name_text(name_start, name_count)
        } else {
            String::new()
        };
        let closing_raw = if name_count > 0 {
            self.raw_name_text(name_start, name_count)
        } else {
            String::new()
        };
        let closing_dynamic = name_count > 0 && self.is_dynamic_tag_name(name_start, name_count);
        while self.pos < total && !matches!(self.kind_at(self.pos), GreaterThan | SyntheticClose) {
            self.pos += 1;
        }
        if self.pos < total {
            self.pos += 1;
        }
        if name_count == 0 {
            self.add_child(flat(
                NodeKind::UnpairedClosingTag,
                0,
                start,
                self.pos - start,
            ));
            return;
        }
        self.close_malformed_raw_source(&closing_name);
        let search_limit = self
            .open_directives
            .last()
            .map_or(1, |d| d.element_stack_base + 1);
        let lower = closing_name.to_ascii_lowercase();
        let mut found = None;
        if !closing_dynamic && let Some(stack) = self.tag_name_stacks.get(&lower) {
            let count = self.open_elements.len();
            for &oi in stack.iter().rev() {
                if oi >= search_limit && oi < count {
                    let el = self.open_elements[oi];
                    if self.tag_names.get(&el).map_or("", String::as_str) == lower {
                        found = Some(oi);
                        break;
                    }
                }
            }
        }
        if found.is_none() {
            let mut i = self.open_elements.len() as isize - 1;
            while i >= search_limit as isize {
                let el = self.open_elements[i as usize];
                if self.nodes[el].kind == NodeKind::Element {
                    let open_tag = self.tag_names.get(&el).cloned().unwrap_or_default();
                    let open_raw = self.raw_tag_names.get(&el).cloned().unwrap_or_default();
                    let open_dynamic = self.dynamic_tag_names.contains(&el);
                    if open_dynamic || closing_dynamic {
                        if open_raw == closing_raw {
                            found = Some(i as usize);
                            break;
                        }
                    } else if open_tag.to_ascii_lowercase() == lower
                        || is_slot_closing_match(&open_raw, &closing_raw)
                    {
                        found = Some(i as usize);
                        break;
                    }
                }
                i -= 1;
            }
        }
        if let Some(oi) = found {
            let depth = self.open_elements.len() - oi;
            let el = self.open_elements[oi];
            self.nodes[el].token_count = self.pos - self.nodes[el].token_start;
            let idx = self.nodes.len();
            self.nodes.push(flat(
                NodeKind::ClosingElementName,
                el,
                name_start,
                name_count,
            ));
            let last = self.nodes[el].last_child;
            if last != NONE {
                self.nodes[last].next_sibling = idx;
            } else {
                self.nodes[el].first_child = idx;
            }
            self.nodes[el].last_child = idx;
            self.build_element_name_parts(name_start, name_count, idx);
            for _ in 0..depth {
                if self.open_elements.len() > 1 {
                    let p = self.open_elements.pop().unwrap();
                    self.cleanup_tag_name_stack(p);
                }
            }
            return;
        }
        self.add_child(flat_data(
            NodeKind::UnpairedClosingTag,
            0,
            start,
            self.pos - start,
            name_count,
        ));
    }

    fn scan_element_name(&mut self) -> (usize, usize) {
        use TokenType::*;
        let limit = self.tokens.len();
        let mut count = 0;
        let mut generic = 0;
        while self.pos < limit {
            let t = self.kind_at(self.pos);
            if matches!(
                t,
                Whitespace | GreaterThan | Slash | Directive | TsxGenericType | SyntheticClose
            ) {
                if t == TsxGenericType {
                    generic = count + 1;
                }
                break;
            }
            if t == TagName {
                count += 1;
                self.pos += 1;
                continue;
            }
            if is_construct_start(t) {
                let (np, n) = scan_construct(self.tokens, self.pos, limit);
                count += n;
                self.pos = np;
                continue;
            }
            break;
        }
        (count, generic)
    }

    fn build_element_name_parts(&mut self, start: usize, count: usize, parent: usize) {
        let end = start + count;
        let mut i = start;
        let mut last = NONE;
        while i < end {
            let t = self.kind_at(i);
            let mut n = 1;
            if t == TokenType::TagName {
                self.link_child(parent, &mut last, flat(NodeKind::Text, parent, i, 1));
            } else if is_construct_start(t) {
                let kind = construct_kind(t).unwrap_or(NodeKind::Echo);
                n = count_construct_tokens(self.tokens, i, end);
                self.link_child(parent, &mut last, flat(kind, parent, i, n));
            }
            i += n;
        }
    }

    fn tag_name_text(&self, start: usize, count: usize) -> String {
        let mut s = String::new();
        for i in start..(start + count).min(self.tokens.len()) {
            if self.kind_at(i) == TokenType::TagName {
                s.push_str(&self.source[self.tokens[i].start..self.tokens[i].end]);
            }
        }
        s
    }

    fn raw_name_text(&self, start: usize, count: usize) -> String {
        let mut s = String::new();
        for i in start..(start + count).min(self.tokens.len()) {
            s.push_str(&self.source[self.tokens[i].start..self.tokens[i].end]);
        }
        s
    }

    fn is_dynamic_tag_name(&self, start: usize, count: usize) -> bool {
        (start..(start + count).min(self.tokens.len()))
            .any(|i| self.kind_at(i) != TokenType::TagName)
    }

    fn build_attributes(&mut self, el: usize) {
        use TokenType::*;
        let attr_end = self.find_attribute_region_end();
        self.open_elements.push(el);
        let saved_directives = std::mem::take(&mut self.open_directives);
        let saved_conditions = std::mem::take(&mut self.open_conditions);
        let saved_region = self.attribute_region_end;
        self.attribute_region_end = Some(attr_end);
        while self.pos < attr_end {
            let t = self.kind_at(self.pos);
            if self.open_directives.last().is_some_and(|d| d.name == "php") {
                if t == Directive && directive_name(&self.tokens[self.pos], self.source) == "endphp"
                {
                    self.process_directive();
                    continue;
                }
                if is_construct_start(t) {
                    self.process_construct_in_attributes();
                } else {
                    self.process_text();
                }
                continue;
            }
            if t == Whitespace {
                self.add_child(flat(NodeKind::AttributeWhitespace, 0, self.pos, 1));
                self.pos += 1;
                continue;
            }
            if t == Directive {
                if self.is_non_structural_directive_token(self.pos) {
                    self.build_unified_attribute(attr_end);
                    continue;
                }
                self.process_directive();
                continue;
            }
            if t == JsxShorthandAttribute {
                self.add_child(flat(NodeKind::JsxAttribute, 0, self.pos, 1));
                self.pos += 1;
                continue;
            }
            self.build_unified_attribute(attr_end);
        }
        self.close_remaining_attribute_directives();
        self.open_directives = saved_directives;
        self.open_conditions = saved_conditions;
        self.attribute_region_end = saved_region;
        self.open_elements.pop();
    }

    fn find_attribute_region_end(&self) -> usize {
        use TokenType::*;
        let total = self.tokens.len();
        let mut end = self.pos;
        while end < total {
            let t = self.kind_at(end);
            if matches!(t, GreaterThan | SyntheticClose) {
                break;
            }
            if t == Slash
                && end + 1 < total
                && matches!(self.kind_at(end + 1), GreaterThan | SyntheticClose)
            {
                break;
            }
            end += 1;
        }
        end
    }

    fn build_unified_attribute(&mut self, attr_end: usize) {
        let attr_start = self.pos;
        let b = self.scan_attribute_bounds(attr_end);
        if b.length == 0 {
            self.pos += 1;
            return;
        }
        self.pos = b.end;
        let first = self.kind_at(attr_start);
        if is_construct_start(first)
            && count_construct_tokens(self.tokens, attr_start, attr_end) == b.length
        {
            self.pos = attr_start;
            self.process_construct_in_attributes();
            return;
        }
        if b.is_name_value {
            let attr = self.add_child(flat(NodeKind::Attribute, 0, attr_start, b.length));
            let name =
                self.create_first_child(attr, NodeKind::AttributeName, attr_start, b.name_count);
            self.build_parts(attr_start, attr_start + b.name_count, name, false);
            if b.value_count > 0 {
                let value = self.create_sibling_child(
                    name,
                    attr,
                    NodeKind::AttributeValue,
                    b.value_start,
                    b.value_count,
                );
                self.build_attribute_value_parts(b.value_start, b.value_count, value);
            }
            return;
        }
        let attr = self.add_child(flat(NodeKind::Attribute, 0, attr_start, b.length));
        let name = self.create_first_child(attr, NodeKind::AttributeName, attr_start, b.length);
        self.build_parts(attr_start, attr_start + b.length, name, false);
    }

    fn scan_attribute_bounds(&self, attr_end: usize) -> Bounds {
        use TokenType::*;
        let attr_start = self.pos;
        let mut equals = None;
        let mut name_end = None;
        let mut scan = self.pos;
        let mut last_non_ws = self.pos;
        while scan < attr_end {
            let t = self.kind_at(scan);
            if t == Whitespace {
                let mut look = scan + 1;
                while look < attr_end && self.kind_at(look) == Whitespace {
                    look += 1;
                }
                if look < attr_end && self.kind_at(look) == Equals {
                    scan = look;
                    continue;
                }
                break;
            }
            if t == Equals {
                equals = Some(scan);
                name_end = Some(last_non_ws);
                scan += 1;
                while scan < attr_end && self.kind_at(scan) == Whitespace {
                    scan += 1;
                }
                if scan < attr_end {
                    if self.kind_at(scan) == Quote {
                        scan += 1;
                        while scan < attr_end && self.kind_at(scan) != Quote {
                            scan = advance_past(self.tokens, scan, attr_end);
                        }
                        if scan < attr_end {
                            scan += 1;
                        }
                    } else {
                        while scan < attr_end && self.kind_at(scan) != Whitespace {
                            scan = advance_past(self.tokens, scan, attr_end);
                        }
                    }
                }
                break;
            }
            scan = advance_past(self.tokens, scan, attr_end);
            last_non_ws = scan;
        }
        let length = scan - attr_start;
        if length == 0 {
            return Bounds {
                length: 0,
                end: scan,
                is_name_value: false,
                name_count: 0,
                value_start: 0,
                value_count: 0,
            };
        }
        if let (Some(eq), Some(ne)) = (equals, name_end) {
            let name_count = ne - attr_start;
            let mut value_start = eq + 1;
            while value_start < scan && self.kind_at(value_start) == Whitespace {
                value_start += 1;
            }
            return Bounds {
                length,
                end: scan,
                is_name_value: true,
                name_count,
                value_start,
                value_count: scan - value_start,
            };
        }
        Bounds {
            length,
            end: scan,
            is_name_value: false,
            name_count: 0,
            value_start: 0,
            value_count: 0,
        }
    }

    fn build_attribute_value_parts(&mut self, start: usize, count: usize, parent: usize) {
        let end = start + count;
        let mut i = start;
        if i < end && self.kind_at(i) == TokenType::Quote {
            i += 1;
        }
        let mut eff_end = end;
        if eff_end > start && self.kind_at(eff_end - 1) == TokenType::Quote {
            eff_end -= 1;
        }
        self.build_parts(i, eff_end, parent, true);
    }

    fn build_parts(&mut self, start: usize, end: usize, parent: usize, as_value: bool) {
        use TokenType::*;
        let mut i = start;
        let mut last = NONE;
        while i < end {
            let t = self.kind_at(i);
            let mut n = 1;
            let text_like = if as_value {
                matches!(t, AttributeValue | Text)
            } else {
                matches!(
                    t,
                    AttributeName | BoundAttribute | EscapedAttribute | ShorthandAttribute | Text
                )
            };
            let node = if text_like {
                flat(NodeKind::Text, parent, i, 1)
            } else if is_construct_start(t) {
                n = count_construct_tokens(self.tokens, i, end);
                flat(construct_kind(t).unwrap_or(NodeKind::Echo), parent, i, n)
            } else if as_value && t == Directive {
                let mut dir_end = i + 1;
                if dir_end < end && self.kind_at(dir_end) == DirectiveArgs {
                    dir_end += 1;
                }
                n = dir_end - i;
                flat(NodeKind::Directive, parent, i, n)
            } else {
                flat(NodeKind::Text, parent, i, 1)
            };
            self.link_child(parent, &mut last, node);
            i += n;
        }
    }

    fn process_construct_in_attributes(&mut self) {
        let start = self.pos;
        let kind = construct_kind(self.kind_at(start)).unwrap_or(NodeKind::Echo);
        let n = count_construct_tokens(self.tokens, start, self.tokens.len());
        self.pos += n;
        self.add_child(flat(kind, 0, start, n));
    }

    fn close_remaining_attribute_directives(&mut self) {
        while let Some(frame) = self.open_conditions.pop() {
            self.pop_if_top(frame.current_branch_idx);
            self.pop_if_top(frame.block_idx);
            self.nodes[frame.block_idx].token_count =
                self.pos - self.nodes[frame.block_idx].token_start;
        }
        while let Some(frame) = self.open_directives.pop() {
            self.pop_if_top(frame.start_directive_idx);
            self.pop_if_top(frame.block_idx);
            self.nodes[frame.block_idx].token_count =
                self.pos - self.nodes[frame.block_idx].token_start;
        }
    }

    fn process_directive(&mut self) {
        let start = self.pos;
        if self.is_inside_malformed_raw_element() || self.is_non_structural_directive_token(start) {
            let (_, _, consumed) =
                check_directive_args_fast(self.tokens, start + 1, self.tokens.len());
            let n = 1 + consumed;
            self.add_child(flat(NodeKind::Directive, 0, start, n));
            self.pos += n;
            return;
        }
        let name = directive_name(&self.tokens[self.pos], self.source);
        let (has_args, args_idx, consumed) =
            check_directive_args_fast(self.tokens, self.pos + 1, self.tokens.len());
        let n = 1 + consumed;
        let args =
            args_idx.map(|i| self.source[self.tokens[i].start..self.tokens[i].end].to_string());

        if self.is_switch_related(&name) {
            self.process_switch_directive(&name, start, n);
            return;
        }
        if self.directives.is_conditional_pair(&name) {
            if self.should_pair_directive(&name, start, n, args.as_deref()) {
                if self.directives.is_condition(&name) {
                    self.open_condition(&name, start, n);
                } else {
                    self.open_paired_directive(&name, start, n, None);
                }
            } else {
                self.create_standalone(start, n);
            }
            return;
        }
        if self.is_condition_related(&name) {
            self.process_condition_directive(&name, start, n, args.as_deref());
            return;
        }
        if self.should_treat_as_branch_of_open_directive(&name, args.as_deref()) {
            self.open_directive_branch(&name, start, n);
            return;
        }
        if self
            .open_directives
            .iter()
            .any(|f| f.terminators.contains(&name))
        {
            self.close_directive(&name, start, n);
            return;
        }
        if name == "php" && has_args {
            self.create_standalone(start, n);
            return;
        }
        if self.directives.is_paired(&name) && self.should_open_known_paired(&name, start, n) {
            self.open_paired_directive(&name, start, n, None);
            return;
        }
        if self.try_open_discovered_directive(&name, start, n) {
            return;
        }
        self.create_standalone(start, n);
    }

    fn create_standalone(&mut self, start: usize, n: usize) {
        self.add_child(flat(NodeKind::Directive, 0, start, n));
        self.pos += n;
    }

    fn directive_index(&mut self) -> &DirectiveTokenIndex {
        if self.directive_index.is_none() {
            self.ensure_literal_indexes();
            let script = self.script_literal_directives.as_ref().unwrap();
            let attr = self.attribute_literal_directives.as_ref().unwrap();
            let idx = DirectiveTokenIndex::new(self.tokens, self.source, |i| {
                !script.contains(&i) && !attr.contains(&i)
            });
            self.directive_index = Some(idx);
        }
        self.directive_index.as_ref().unwrap()
    }

    fn try_open_discovered_directive(&mut self, name: &str, start: usize, n: usize) -> bool {
        let Some(family) = self.resolve_family(name) else {
            return false;
        };
        let dstart = start + n;
        if !family.has_advisory_condition {
            let terms = vec![family.end_name.clone()];
            let search_end = self.directive_search_end(&family.openers, dstart, &terms, &[]);
            let idx = self.directive_index();
            if !in_range(idx, &family.end_name, dstart, search_end) {
                return false;
            }
            if idx
                .find_matching_terminator(&family.openers, dstart, &terms, search_end, 0)
                .is_none()
            {
                return false;
            }
            self.open_paired_directive(
                name,
                start,
                n,
                Some((terms, Vec::new(), family.openers.clone())),
            );
            return true;
        }
        let generic = self.directives.condition_terminators().to_vec();
        let terminators = if generic.contains(&family.end_name) {
            generic.clone()
        } else {
            let mut v = vec![family.end_name.clone()];
            v.extend(generic.iter().cloned());
            v
        };
        let branches = vec![
            family.else_name.clone(),
            "else".to_string(),
            "elseif".to_string(),
        ];
        let search_end =
            self.directive_search_end(&family.openers, dstart, &terminators, &branches);
        let idx = self.directive_index();
        if !in_range(idx, &family.end_name, dstart, search_end)
            && !in_range(idx, &family.else_name, dstart, search_end)
            && !generic.iter().any(|g| in_range(idx, g, dstart, search_end))
        {
            return false;
        }
        let (term_idx, term_name, branch_idx) = idx.analyze_unknown_family(
            &family.openers,
            dstart,
            &terminators,
            &branches,
            search_end,
        );
        let (Some(_), Some(term_name)) = (term_idx, term_name) else {
            return false;
        };
        let condition_like = branch_idx.is_some() || term_name != family.end_name;
        let meta = if condition_like {
            (terminators, branches, family.openers.clone())
        } else {
            (
                vec![family.end_name.clone()],
                Vec::new(),
                family.openers.clone(),
            )
        };
        self.open_paired_directive(name, start, n, Some(meta));
        true
    }

    fn resolve_family(&mut self, name: &str) -> Option<std::rc::Rc<Family>> {
        let name = name.to_ascii_lowercase();
        if let Some(f) = self.family_cache.get(&name) {
            return f.clone();
        }
        let result = self.compute_family(&name).map(std::rc::Rc::new);
        self.family_cache.insert(name, result.clone());
        result
    }

    fn compute_family(&self, name: &str) -> Option<Family> {
        if self.directives.is_directive(name) {
            return None;
        }
        let mut base = name.to_string();
        let mut unless_name = format!("unless{name}");
        let mut is_unless = false;
        if let Some(candidate) = name.strip_prefix("unless")
            && !candidate.is_empty()
        {
            if self.directives.is_directive(candidate) {
                return None;
            }
            base = candidate.to_string();
            unless_name = name.to_string();
            is_unless = true;
        }
        let else_name = format!("else{base}");
        let end_name = format!("end{base}");
        let has_pair =
            self.directives.has_advisory_pair(&base) || self.directives.has_seen(&end_name);
        let has_cond =
            self.directives.has_advisory_condition(&base) || self.directives.has_seen(&else_name);
        if !has_pair && !has_cond {
            return None;
        }
        let openers = if is_unless || self.directives.has_seen(&unless_name) {
            vec![base, unless_name]
        } else {
            vec![name.to_string()]
        };
        Some(Family {
            else_name,
            end_name,
            openers,
            has_advisory_condition: has_cond,
        })
    }

    fn should_open_known_paired(&mut self, name: &str, start: usize, n: usize) -> bool {
        let (terms, branches) = match self.directives.get(name) {
            Some(d) => (d.terminators.clone(), d.condition_like_branches.clone()),
            None => (Vec::new(), Vec::new()),
        };
        !terms.is_empty()
            && self.has_matching_terminator_in_scope(name, start + n, Some(&terms), None, &branches)
    }

    fn openers_for_terminators(&self, name: &str, terms: &[String]) -> Vec<String> {
        let openers = self.directives.openers_for_terminators(terms);
        if !openers.is_empty() {
            return openers;
        }
        if name.is_empty() {
            Vec::new()
        } else {
            vec![name.to_ascii_lowercase()]
        }
    }

    fn directive_search_end(
        &mut self,
        openers: &[String],
        start: usize,
        terms: &[String],
        branches: &[String],
    ) -> Option<usize> {
        let mut end = self.attribute_region_end;
        let b = self.find_open_element_boundary(start);
        end = earlier(end, b);
        let b = self.find_open_directive_boundary(openers, terms, branches, start);
        end = earlier(end, b);
        let b = self.find_open_condition_boundary(openers, terms, branches, start);
        end = earlier(end, b);
        let b = self.find_open_switch_boundary(openers, terms, start);
        earlier(end, b)
    }

    fn find_open_element_boundary(&self, start: usize) -> Option<usize> {
        use TokenType::*;
        let mut containing = None;
        for &el in self.open_elements.iter().rev() {
            if self.nodes.get(el).map(|n| n.kind) != Some(NodeKind::Element) {
                continue;
            }
            if let Some(t) = self.tag_names.get(&el) {
                containing = Some(t.clone());
                break;
            }
        }
        let containing = containing?;
        let mut depth = 0usize;
        let n = self.tokens.len();
        for i in start..n {
            if self.kind_at(i) != LessThan || i + 2 >= n {
                continue;
            }
            if self.kind_at(i + 1) == Slash {
                if self.kind_at(i + 2) != TagName {
                    continue;
                }
                let t = &self.tokens[i + 2];
                if self.source[t.start..t.end].to_ascii_lowercase() != containing {
                    continue;
                }
                if depth == 0 {
                    return Some(i);
                }
                depth -= 1;
                continue;
            }
            if self.kind_at(i + 1) != TagName {
                continue;
            }
            let t = &self.tokens[i + 1];
            if self.source[t.start..t.end].to_ascii_lowercase() != containing {
                continue;
            }
            if !self.is_self_closing_element_at(i) {
                depth += 1;
            }
        }
        None
    }

    fn is_self_closing_element_at(&self, start: usize) -> bool {
        use TokenType::*;
        for i in start + 1..self.tokens.len() {
            match self.kind_at(i) {
                GreaterThan => return self.kind_at(i - 1) == Slash,
                SyntheticClose | LessThan => return false,
                _ => {}
            }
        }
        false
    }

    fn find_open_directive_boundary(
        &mut self,
        openers: &[String],
        terms: &[String],
        branches: &[String],
        start: usize,
    ) -> Option<usize> {
        let frames = self.open_directives.clone();
        let mut boundary = None;
        for frame in &frames {
            if frame.terminators.is_empty() && frame.branches.is_empty() {
                continue;
            }
            let reserves =
                intersects(&frame.openers, openers) || intersects(&frame.terminators, terms);
            let b = boundary_branches(&frame.branches, branches);
            if let Some(i) = self.find_frame_boundary(
                &frame.openers,
                start,
                &frame.terminators,
                &b,
                reserves,
                openers,
            ) {
                boundary = earlier(boundary, Some(i));
            }
        }
        boundary
    }

    fn find_open_condition_boundary(
        &mut self,
        openers: &[String],
        terms: &[String],
        branches: &[String],
        start: usize,
    ) -> Option<usize> {
        if self.open_conditions.is_empty() {
            return None;
        }
        let terminators = self.directives.condition_terminators().to_vec();
        let cond_openers = self.openers_for_terminators("", &terminators);
        let frames = self.open_conditions.clone();
        let mut boundary = None;
        for frame in &frames {
            let b = boundary_branches(&self.condition_boundary_branches(&frame.name), branches);
            let reserves = intersects(&cond_openers, openers) || intersects(&terminators, terms);
            if let Some(i) =
                self.find_frame_boundary(&cond_openers, start, &terminators, &b, reserves, openers)
            {
                boundary = earlier(boundary, Some(i));
            }
        }
        boundary
    }

    fn condition_boundary_branches(&self, name: &str) -> Vec<String> {
        let terms: HashSet<&String> = self.directives.condition_terminators().iter().collect();
        self.directives
            .branches(name)
            .into_iter()
            .filter(|b| !terms.contains(b))
            .collect()
    }

    fn find_open_switch_boundary(
        &mut self,
        openers: &[String],
        terms: &[String],
        start: usize,
    ) -> Option<usize> {
        let frames: Vec<String> = self.open_switches.iter().map(|f| f.name.clone()).collect();
        let mut boundary = None;
        let endswitch = vec!["endswitch".to_string()];
        for name in &frames {
            let reserves = openers.contains(name) || intersects(&endswitch, terms);
            let branches = self.directives.switch_branches(name);
            if let Some(i) = self.find_frame_boundary(
                std::slice::from_ref(name),
                start,
                &endswitch,
                &branches,
                reserves,
                openers,
            ) {
                boundary = earlier(boundary, Some(i));
            }
        }
        boundary
    }

    fn find_frame_boundary(
        &mut self,
        openers: &[String],
        start: usize,
        terminators: &[String],
        branches: &[String],
        reserves: bool,
        candidate_openers: &[String],
    ) -> Option<usize> {
        self.directive_index();
        let idx = self.directive_index.as_ref().unwrap();
        let candidate_set: Option<HashSet<&String>> = if candidate_openers.is_empty() {
            None
        } else {
            Some(candidate_openers.iter().collect())
        };
        let should_count = |token_idx: usize, name: &str| -> bool {
            let Some(set) = &candidate_set else {
                return true;
            };
            let shadowed = self.is_branch_boundary_shadowed(start, token_idx, name);
            if !set.iter().any(|s| s.as_str() == name) {
                return !shadowed;
            }
            match self.directives.get(name) {
                Some(d) if d.role == Role::Opening && !d.terminators.is_empty() => {
                    !idx.has_args(token_idx) && !shadowed
                }
                _ => !shadowed,
            }
        };
        let find = |nesting: usize| -> Option<usize> {
            if branches.is_empty() {
                idx.find_matching_terminator(openers, start, terminators, None, nesting)
            } else {
                idx.find_matching_boundary(
                    openers,
                    start,
                    terminators,
                    branches,
                    nesting,
                    &should_count,
                )
            }
        };
        if !reserves {
            return find(0);
        }
        find(1).or_else(|| find(0))
    }

    fn is_branch_boundary_shadowed(&self, start: usize, boundary: usize, branch: &str) -> bool {
        let branch = branch.to_ascii_lowercase();
        let mut stack: Vec<(Vec<String>, Vec<String>)> = Vec::new();
        for i in start..boundary {
            let t = &self.tokens[i];
            if t.kind != TokenType::Directive || self.is_non_structural_directive_token_ro(i) {
                continue;
            }
            let name = directive_name(t, self.source);
            if let Some(top) = stack.last() {
                if top.0.contains(&name) {
                    stack.pop();
                    continue;
                }
                if top.1.contains(&name) {
                    continue;
                }
            }
            let has_args = check_directive_args_fast(self.tokens, i + 1, boundary).0;
            if let Some(shield) = self.branch_boundary_shield(&name, has_args, &branch) {
                stack.push(shield);
            }
        }
        !stack.is_empty()
    }

    fn branch_boundary_shield(
        &self,
        name: &str,
        has_args: bool,
        branch: &str,
    ) -> Option<(Vec<String>, Vec<String>)> {
        let d = self.directives.get(name)?;
        if d.role != Role::Opening || d.terminators.is_empty() {
            return None;
        }
        if name == branch && !has_args {
            return None;
        }
        let branches = if d.is_condition {
            self.condition_boundary_branches(name)
        } else {
            d.condition_like_branches.clone()
        };
        if !branches.iter().any(|b| b == branch) {
            return None;
        }
        let terms = if d.is_condition {
            self.directives.condition_terminators().to_vec()
        } else {
            d.terminators.clone()
        };
        Some((terms, branches))
    }

    fn should_treat_as_branch_of_open_directive(&self, name: &str, args: Option<&str>) -> bool {
        if !self
            .open_directives
            .iter()
            .any(|f| f.branches.iter().any(|b| b == name))
        {
            return false;
        }
        if args.is_some()
            && let Some(d) = self.directives.get(name)
            && d.role == Role::Opening
            && !d.terminators.is_empty()
        {
            return false;
        }
        true
    }

    fn open_paired_directive(
        &mut self,
        name: &str,
        start: usize,
        n: usize,
        meta: Option<(Vec<String>, Vec<String>, Vec<String>)>,
    ) {
        let block = self.add_child(flat(NodeKind::DirectiveBlock, 0, start, n));
        let base = self.open_elements.len();
        self.open_elements.push(block);
        let start_dir = self.add_child(flat(NodeKind::Directive, 0, start, n));
        let (terminators, branches, openers) = match meta {
            Some(m) => m,
            None => {
                let d = self.directives.get(name);
                let terms = d.map_or_default(|d| d.terminators.clone());
                let branches = d.map_or_default(|d| d.condition_like_branches.clone());
                let openers = self.openers_for_terminators(name, &terms);
                (terms, branches, openers)
            }
        };
        self.open_directives.push(DirectiveFrame {
            block_idx: block,
            start_directive_idx: start_dir,
            name: name.to_string(),
            element_stack_base: base,
            terminators,
            branches,
            openers,
        });
        self.open_elements.push(start_dir);
        self.pos += n;
    }

    fn open_directive_branch(&mut self, name: &str, start: usize, n: usize) {
        let Some(matched) = self
            .open_directives
            .iter()
            .rposition(|f| f.branches.iter().any(|b| b == name))
        else {
            self.create_standalone(start, n);
            return;
        };
        while self.open_directives.len() - 1 > matched {
            let f = self.open_directives.pop().unwrap();
            self.close_unmatched_at_boundary(&f, start);
        }
        let (start_dir, base) = {
            let f = &self.open_directives[matched];
            (f.start_directive_idx, f.element_stack_base)
        };
        self.pop_if_top(start_dir);
        self.pop_elements_to_depth(base + 1);
        let branch = self.add_child(flat(NodeKind::Directive, 0, start, n));
        self.open_elements.push(branch);
        self.open_directives[matched].start_directive_idx = branch;
        self.pos += n;
    }

    fn close_directive(&mut self, name: &str, start: usize, n: usize) {
        let Some(matched) = self
            .open_directives
            .iter()
            .rposition(|f| f.terminators.iter().any(|t| t == name))
        else {
            self.create_standalone(start, n);
            return;
        };
        while self.open_directives.len() - 1 > matched {
            let f = self.open_directives.pop().unwrap();
            self.close_unmatched_at_boundary(&f, start);
        }
        let f = self.open_directives.pop().unwrap();
        self.pop_elements_to_depth(f.element_stack_base + 2);
        self.pop_if_top(f.start_directive_idx);
        self.add_child(flat(NodeKind::Directive, 0, start, n));
        self.pop_if_top(f.block_idx);
        self.nodes[f.block_idx].token_count = start + n - self.nodes[f.block_idx].token_start;
        self.pos += n;
    }

    fn close_unmatched_at_boundary(&mut self, f: &DirectiveFrame, boundary: usize) {
        self.pop_elements_to_depth(f.element_stack_base + 2);
        self.pop_if_top(f.start_directive_idx);
        self.pop_if_top(f.block_idx);
        self.nodes[f.block_idx].token_count = boundary - self.nodes[f.block_idx].token_start;
    }

    fn is_condition_related(&self, name: &str) -> bool {
        if self.directives.is_condition(name) {
            return true;
        }
        if let Some(c) = self.open_conditions.last()
            && self
                .directives
                .branches(&c.name)
                .iter()
                .any(|b| b.to_ascii_lowercase() == name)
        {
            return true;
        }
        self.directives
            .condition_terminators()
            .iter()
            .any(|t| t == name)
    }

    fn process_condition_directive(
        &mut self,
        name: &str,
        start: usize,
        n: usize,
        args: Option<&str>,
    ) {
        if self.try_handle_condition_with_open_directive(name, start, n, args) {
            return;
        }
        let is_final = self
            .directives
            .condition_terminators()
            .iter()
            .any(|t| t == name);
        if is_final && !self.open_conditions.is_empty() {
            self.close_condition(start, n);
            return;
        }
        if let Some(c) = self.open_conditions.last()
            && self
                .directives
                .branches(&c.name)
                .iter()
                .any(|b| b.to_ascii_lowercase() == name)
        {
            self.open_condition_branch(start, n);
            return;
        }
        if self.directives.is_condition(name)
            && self
                .directives
                .get(name)
                .is_some_and(|d| d.role == Role::Opening)
        {
            if self.should_open_known_condition(name, start, n) {
                self.open_condition(name, start, n);
            } else {
                self.create_standalone(start, n);
            }
            return;
        }
        self.create_standalone(start, n);
    }

    fn should_open_known_condition(&mut self, name: &str, start: usize, n: usize) -> bool {
        let terms = self.directives.condition_terminators().to_vec();
        let branches = self.condition_boundary_branches(name);
        !terms.is_empty()
            && self.has_matching_terminator_in_scope(name, start + n, Some(&terms), None, &branches)
    }

    fn try_handle_condition_with_open_directive(
        &mut self,
        name: &str,
        start: usize,
        n: usize,
        args: Option<&str>,
    ) -> bool {
        let mut matched = None;
        for (i, f) in self.open_directives.iter().enumerate().rev() {
            if f.branches.iter().any(|b| b == name) {
                matched = Some((i, true));
                break;
            }
            if f.terminators.iter().any(|t| t == name) {
                matched = Some((i, false));
                break;
            }
        }
        let Some((fi, is_branch)) = matched else {
            return false;
        };
        if is_branch
            && args.is_some()
            && self
                .directives
                .get(name)
                .is_some_and(|d| d.role == Role::Opening)
            && self.directives.is_condition(name)
        {
            return false;
        }
        if let Some(c) = self.open_conditions.last() {
            let terms = self.directives.condition_terminators();
            let cond_match = terms.iter().any(|t| t == name)
                || self.directives.branches(&c.name).iter().any(|b| b == name);
            if cond_match {
                let cond_start = self
                    .nodes
                    .get(c.current_branch_idx)
                    .map_or(-1, |n| n.token_start as isize);
                let dir_start = self
                    .nodes
                    .get(self.open_directives[fi].start_directive_idx)
                    .map_or(-1, |n| n.token_start as isize);
                if cond_start >= dir_start {
                    return false;
                }
            }
        }
        if is_branch {
            self.open_directive_branch(name, start, n);
        } else {
            self.close_directive(name, start, n);
        }
        true
    }

    fn open_condition(&mut self, name: &str, start: usize, n: usize) {
        let block = self.add_child(flat(NodeKind::DirectiveBlock, 0, start, n));
        let base = self.open_elements.len();
        self.open_elements.push(block);
        let branch = self.add_child(flat(NodeKind::Directive, 0, start, n));
        self.open_elements.push(branch);
        self.open_conditions.push(ConditionFrame {
            block_idx: block,
            current_branch_idx: branch,
            name: name.to_string(),
            element_stack_base: base,
        });
        self.pos += n;
    }

    fn open_condition_branch(&mut self, start: usize, n: usize) {
        let Some(f) = self.open_conditions.last().cloned() else {
            self.create_standalone(start, n);
            return;
        };
        self.pop_if_top(f.current_branch_idx);
        self.pop_elements_to_depth(f.element_stack_base + 1);
        let branch = self.add_child(flat(NodeKind::Directive, 0, start, n));
        self.open_elements.push(branch);
        self.open_conditions.last_mut().unwrap().current_branch_idx = branch;
        self.pos += n;
    }

    fn close_condition(&mut self, start: usize, n: usize) {
        let Some(f) = self.open_conditions.pop() else {
            self.create_standalone(start, n);
            return;
        };
        self.pop_elements_to_depth(f.element_stack_base + 1);
        self.pop_if_top(f.current_branch_idx);
        self.add_child(flat(NodeKind::Directive, 0, start, n));
        self.pop_if_top(f.block_idx);
        self.nodes[f.block_idx].token_count = start + n - self.nodes[f.block_idx].token_start;
        self.pos += n;
    }

    fn is_switch_related(&self, name: &str) -> bool {
        if self.directives.is_switch(name) || name == "endswitch" {
            return true;
        }
        !self.open_switches.is_empty()
            && (self.directives.is_switch_branch(name)
                || self.directives.is_switch_terminator(name))
    }

    fn process_switch_directive(&mut self, name: &str, start: usize, n: usize) {
        let open = !self.open_switches.is_empty();
        if name == "endswitch" && open {
            let f = self.open_switches.pop().unwrap();
            self.pop_elements_to_depth(f.element_stack_base + 2);
            if let Some(c) = f.current_case_idx {
                self.pop_if_top(c);
            }
            self.pop_if_top(f.switch_directive_idx);
            self.add_child(flat(NodeKind::Directive, 0, start, n));
            self.pop_if_top(f.block_idx);
            self.nodes[f.block_idx].token_count = start + n - self.nodes[f.block_idx].token_start;
            self.pos += n;
        } else if self.directives.is_switch_terminator(name) && open {
            if self
                .open_switches
                .last()
                .unwrap()
                .current_case_idx
                .is_none()
            {
                self.create_standalone(start, n);
                return;
            }
            self.add_child(flat(NodeKind::Directive, 0, start, n));
            self.pos += n;
        } else if open && self.directives.is_switch_branch(name) {
            let (base, case) = {
                let f = self.open_switches.last().unwrap();
                (f.element_stack_base, f.current_case_idx)
            };
            self.pop_elements_to_depth(base + 2);
            if let Some(c) = case {
                self.pop_if_top(c);
            }
            let idx = self.add_child(flat(NodeKind::Directive, 0, start, n));
            self.open_switches.last_mut().unwrap().current_case_idx = Some(idx);
            self.open_elements.push(idx);
            self.pos += n;
        } else if self.directives.is_switch(name) {
            let terms = vec!["endswitch".to_string()];
            let openers = vec![name.to_string()];
            if self.has_matching_terminator_in_scope(
                name,
                start + n,
                Some(&terms),
                Some(&openers),
                &[],
            ) {
                let block = self.add_child(flat(NodeKind::DirectiveBlock, 0, start, n));
                let base = self.open_elements.len();
                self.open_elements.push(block);
                let sw = self.add_child(flat(NodeKind::Directive, 0, start, n));
                self.open_switches.push(SwitchFrame {
                    block_idx: block,
                    switch_directive_idx: sw,
                    current_case_idx: None,
                    name: name.to_string(),
                    element_stack_base: base,
                });
                self.open_elements.push(sw);
                self.pos += n;
            } else {
                self.create_standalone(start, n);
            }
        } else {
            self.create_standalone(start, n);
        }
    }

    fn should_pair_directive(
        &mut self,
        name: &str,
        start: usize,
        n: usize,
        args: Option<&str>,
    ) -> bool {
        let Some(strategy) = self.directives.pairing_strategy(name) else {
            return false;
        };
        let arg_count = |a: Option<&str>| a.map_or(0, |a| count_arguments(unwrap_parentheses(a)));
        match strategy {
            "lang_style" => {
                if args.is_none_or(|a| starts_with_array(unwrap_parentheses(a))) {
                    return self.has_matching_terminator_in_scope(name, start + n, None, None, &[]);
                }
                false
            }
            "section_style" => {
                if let Some(a) = args
                    && count_arguments(unwrap_parentheses(a)) >= 2
                {
                    return false;
                }
                self.has_matching_terminator_in_scope(name, start + n, None, None, &[])
            }
            "no_arguments_block" => {
                arg_count(args) == 0
                    && self.has_matching_terminator_in_scope(name, start + n, None, None, &[])
            }
            "single_argument_block" => {
                arg_count(args) == 1
                    && self.has_matching_terminator_in_scope(name, start + n, None, None, &[])
            }
            _ => false,
        }
    }

    fn has_matching_terminator_in_scope(
        &mut self,
        name: &str,
        after: usize,
        terms_override: Option<&[String]>,
        openers: Option<&[String]>,
        branches: &[String],
    ) -> bool {
        let terms =
            terms_override.map_or_else(|| self.directives.terminators(name), <[String]>::to_vec);
        let search_terms = if terms.is_empty() {
            vec![format!("end{name}")]
        } else {
            terms
        };
        let search_openers = openers.map_or_else(
            || self.openers_for_terminators(name, &search_terms),
            <[String]>::to_vec,
        );
        let search_end = self.directive_search_end(&search_openers, after, &search_terms, branches);
        self.directive_index()
            .find_matching_terminator(&search_openers, after, &search_terms, search_end, 0)
            .is_some()
    }

    fn close_remaining_directives(&mut self) {
        let end = self.tokens.len();
        while let Some(f) = self.open_switches.pop() {
            if let Some(c) = f.current_case_idx {
                self.pop_if_top(c);
            }
            self.pop_if_top(f.switch_directive_idx);
            self.pop_if_top(f.block_idx);
            self.nodes[f.block_idx].token_count = end - self.nodes[f.block_idx].token_start;
        }
        while let Some(f) = self.open_conditions.pop() {
            self.pop_if_top(f.current_branch_idx);
            self.pop_if_top(f.block_idx);
            self.nodes[f.block_idx].token_count = end - self.nodes[f.block_idx].token_start;
        }
        while let Some(f) = self.open_php_tag_conditions.pop() {
            self.pop_if_top(f.current_branch_idx);
            self.pop_if_top(f.block_idx);
            self.nodes[f.block_idx].token_count = end - self.nodes[f.block_idx].token_start;
        }
        while let Some(f) = self.open_directives.pop() {
            self.pop_if_top(f.start_directive_idx);
            self.pop_if_top(f.block_idx);
            self.nodes[f.block_idx].token_count = end - self.nodes[f.block_idx].token_start;
        }
    }

    fn ensure_literal_indexes(&mut self) {
        if self.script_literal_directives.is_none() {
            self.script_literal_directives = Some(self.build_script_literal_indexes());
        }
        if self.attribute_literal_directives.is_none() {
            self.attribute_literal_directives = Some(self.build_attribute_literal_indexes());
        }
    }

    fn is_non_structural_directive_token(&mut self, i: usize) -> bool {
        if self.tokens.get(i).map(|t| t.kind) != Some(TokenType::Directive) {
            return false;
        }
        self.ensure_literal_indexes();
        self.is_non_structural_directive_token_ro(i)
    }

    fn is_non_structural_directive_token_ro(&self, i: usize) -> bool {
        self.script_literal_directives
            .as_ref()
            .is_some_and(|s| s.contains(&i))
            || self
                .attribute_literal_directives
                .as_ref()
                .is_some_and(|s| s.contains(&i))
    }

    fn is_inside_malformed_raw_element(&self) -> bool {
        if self
            .open_elements
            .iter()
            .any(|e| self.malformed_raw_element_indexes.contains(e))
        {
            return true;
        }
        let Some(t) = self.tokens.get(self.pos) else {
            return false;
        };
        self.malformed_raw_element_sources
            .iter()
            .any(|(_, s)| t.start > *s)
    }

    fn close_malformed_raw_source(&mut self, name: &str) {
        let lower = name.to_ascii_lowercase();
        if let Some(i) = self
            .malformed_raw_element_sources
            .iter()
            .rposition(|(n, _)| *n == lower)
        {
            self.malformed_raw_element_sources.remove(i);
        }
    }

    fn has_swallowed_tag_syntax_in_opening(&self, start: usize, end: usize) -> bool {
        if end <= start || end >= self.tokens.len() {
            return false;
        }
        let s = self.tokens[start].start + 1;
        let e = self.tokens[end].end;
        has_swallowed_tag_syntax(&self.source[s..e])
    }

    fn build_script_literal_indexes(&self) -> HashSet<usize> {
        let mut out = HashSet::new();
        let total = self.tokens.len();
        let src = self.source.as_bytes();
        let mut i = 0;
        while i < total {
            if !self.is_static_opening_tag_at(i, "script") {
                i += 1;
                continue;
            }
            let Some(open_end) = self.find_tag_end_token_index(i) else {
                i += 1;
                continue;
            };
            if self.kind_at(open_end) != TokenType::GreaterThan {
                i += 1;
                continue;
            }
            let content_start = self.tokens[open_end].end;
            let closing = self.find_static_closing_tag_start(open_end + 1, "script");
            let content_end = closing.unwrap_or(total);
            for j in open_end + 1..content_end {
                if self.kind_at(j) == TokenType::Directive
                    && is_likely_inside_script_literal_or_comment(
                        src,
                        content_start,
                        self.tokens[j].start,
                    )
                {
                    out.insert(j);
                }
            }
            let Some(c) = closing else {
                break;
            };
            i = c + 1;
        }
        out
    }

    fn build_attribute_literal_indexes(&self) -> HashSet<usize> {
        use TokenType::*;
        let mut out = HashSet::new();
        let total = self.tokens.len();
        let src = self.source.as_bytes();
        let mut i = 0;
        while i < total {
            if self.kind_at(i) != LessThan || self.tokens.get(i + 1).map(|t| t.kind) == Some(Slash)
            {
                i += 1;
                continue;
            }
            let Some(tag_end) = self.find_tag_end_token_index(i) else {
                i += 1;
                continue;
            };
            let mut j = i + 1;
            while j < tag_end {
                if self.kind_at(j) != Equals {
                    j += 1;
                    continue;
                }
                let mut vs = j + 1;
                while vs < tag_end && self.kind_at(vs) == Whitespace {
                    vs += 1;
                }
                if vs >= tag_end || self.kind_at(vs) != Quote {
                    j += 1;
                    continue;
                }
                let Some(ve) = (vs + 1..tag_end).find(|&k| self.kind_at(k) == Quote) else {
                    j += 1;
                    continue;
                };
                let content_start = self.tokens[vs].end;
                for k in vs + 1..ve {
                    if self.kind_at(k) == Directive
                        && is_likely_inside_script_literal_or_comment(
                            src,
                            content_start,
                            self.tokens[k].start,
                        )
                    {
                        out.insert(k);
                    }
                }
                j = ve + 1;
            }
            i = tag_end + 1;
        }
        for i in 0..total {
            if self.kind_at(i) != Directive || out.contains(&i) {
                continue;
            }
            let Some(cs) = self.find_recoverable_attribute_value_content_start(i) else {
                continue;
            };
            if is_likely_inside_script_literal_or_comment(src, cs, self.tokens[i].start) {
                out.insert(i);
            }
        }
        out
    }

    fn find_recoverable_attribute_value_content_start(&self, idx: usize) -> Option<usize> {
        use TokenType::*;
        let mut tag_start = None;
        let mut i = idx as isize - 1;
        while i >= 0 {
            let k = self.kind_at(i as usize);
            if k == GreaterThan {
                return None;
            }
            if k == LessThan {
                let next = self.tokens.get(i as usize + 1).map(|t| t.kind);
                if next == Some(Slash) || next != Some(TagName) {
                    return None;
                }
                tag_start = Some(i as usize);
                break;
            }
            i -= 1;
        }
        let tag_start = tag_start?;
        let src = self.source.as_bytes();
        // isBeforeSourceTagClose
        let mut quote = 0u8;
        for &ch in &src[self.tokens[tag_start].start + 1..self.tokens[idx].start] {
            if quote != 0 {
                if ch == quote {
                    quote = 0;
                }
                continue;
            }
            if ch == b'"' || ch == b'\'' {
                quote = ch;
                continue;
            }
            if ch == b'>' {
                return None;
            }
        }
        let mut i = idx - 1;
        while i > tag_start {
            if self.kind_at(i) == Quote {
                let mut b = i as isize - 1;
                while b > tag_start as isize && self.kind_at(b as usize) == Whitespace {
                    b -= 1;
                }
                if b > tag_start as isize
                    && self.kind_at(b as usize) == Equals
                    && self.is_unclosed_quote_before(self.tokens[i].start, self.tokens[idx].start)
                {
                    return Some(self.tokens[i].end);
                }
            }
            i -= 1;
        }
        let ts = self.tokens[tag_start].start;
        let offset = self.tokens[idx].start;
        let mut i = offset - 1;
        while i > ts {
            let ch = src[i];
            if ch == b'"' || ch == b'\'' {
                let mut b = i as isize - 1;
                while b > ts as isize && matches!(src[b as usize], b' ' | b'\t' | b'\n' | b'\r') {
                    b -= 1;
                }
                if b > ts as isize
                    && src[b as usize] == b'='
                    && self.is_unclosed_quote_before(i, offset)
                {
                    return Some(i + 1);
                }
            }
            i -= 1;
        }
        None
    }

    fn is_unclosed_quote_before(&self, quote_offset: usize, offset: usize) -> bool {
        let src = self.source.as_bytes();
        let q = src[quote_offset];
        if q != b'"' && q != b'\'' {
            return false;
        }
        !src[quote_offset + 1..offset].contains(&q)
    }

    fn is_static_opening_tag_at(&self, i: usize, tag: &str) -> bool {
        if i + 1 >= self.tokens.len()
            || self.kind_at(i) != TokenType::LessThan
            || self.kind_at(i + 1) != TokenType::TagName
        {
            return false;
        }
        let t = &self.tokens[i + 1];
        self.source[t.start..t.end].eq_ignore_ascii_case(tag)
    }

    fn find_static_closing_tag_start(&self, start: usize, tag: &str) -> Option<usize> {
        let mut i = start;
        while i + 2 < self.tokens.len() {
            if self.kind_at(i) == TokenType::LessThan
                && self.kind_at(i + 1) == TokenType::Slash
                && self.kind_at(i + 2) == TokenType::TagName
            {
                let t = &self.tokens[i + 2];
                if self.source[t.start..t.end].eq_ignore_ascii_case(tag) {
                    return Some(i);
                }
            }
            i += 1;
        }
        None
    }

    fn find_tag_end_token_index(&self, start: usize) -> Option<usize> {
        for i in start + 1..self.tokens.len() {
            let t = self.kind_at(i);
            if matches!(t, TokenType::GreaterThan | TokenType::SyntheticClose) {
                return Some(i);
            }
            if i > start + 1 && t == TokenType::LessThan {
                return None;
            }
        }
        None
    }
}

struct Bounds {
    length: usize,
    end: usize,
    is_name_value: bool,
    name_count: usize,
    value_start: usize,
    value_count: usize,
}

fn is_slot_closing_match(open: &str, close: &str) -> bool {
    if !close.eq_ignore_ascii_case("x-slot") {
        return false;
    }
    let o = open.to_ascii_lowercase();
    o.starts_with("x-slot:") || o.starts_with("x-slot[")
}

fn earlier(current: Option<usize>, candidate: Option<usize>) -> Option<usize> {
    match (current, candidate) {
        (c, None) => c,
        (None, c) => c,
        (Some(a), Some(b)) => Some(a.min(b)),
    }
}

fn intersects(a: &[String], b: &[String]) -> bool {
    !a.is_empty() && !b.is_empty() && a.iter().any(|x| b.contains(x))
}

fn boundary_branches(frame: &[String], candidate: &[String]) -> Vec<String> {
    if candidate.is_empty() || frame.is_empty() {
        return frame.to_vec();
    }
    frame
        .iter()
        .filter(|b| !candidate.contains(b))
        .cloned()
        .collect()
}

fn in_range(idx: &DirectiveTokenIndex, name: &str, min: usize, max: Option<usize>) -> bool {
    match max {
        None => idx.exists_after(name, min),
        Some(m) => idx.exists_between(name, min, m),
    }
}
