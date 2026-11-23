//! The Blade/HTML lexer, ported from prettier-plugin-blade's `lexer/lexer.ts`.

use crate::directives::Directives;
use crate::frontend::is_frontend_event_style_at_name;
use crate::scan::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum TokenType {
    Text,
    Whitespace,
    AtSign,
    AttributeName,
    AttributeValue,
    BladeCommentStart,
    BladeCommentEnd,
    BogusComment,
    BoundAttribute,
    CdataStart,
    CdataEnd,
    CommentStart,
    CommentEnd,
    ConditionalCommentStart,
    ConditionalCommentEnd,
    DeclStart,
    DeclEnd,
    Directive,
    DirectiveArgs,
    DoctypeStart,
    Doctype,
    DoctypeEnd,
    EchoStart,
    EchoContent,
    EchoEnd,
    Equals,
    EscapedAttribute,
    GreaterThan,
    IgnoreRange,
    JsxAttributeValue,
    JsxShorthandAttribute,
    LessThan,
    PIStart,
    PIEnd,
    PhpBlockStart,
    PhpBlock,
    PhpBlockEnd,
    PhpTagStart,
    PhpContent,
    PhpTagEnd,
    Quote,
    RawEchoStart,
    RawEchoEnd,
    ShorthandAttribute,
    Slash,
    SyntheticClose,
    TagName,
    TripleEchoStart,
    TripleEchoEnd,
    TsxGenericType,
    VerbatimStart,
    VerbatimEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) kind: TokenType,
    pub(crate) start: usize,
    pub(crate) end: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum State {
    Data,
    RawText,
    BladeComment,
    Comment,
    TagName,
    BeforeAttrName,
    AttrName,
    AfterAttrName,
    BeforeAttrValue,
    AttrValueQuoted,
    AttrValueUnquoted,
}

impl State {
    fn in_tag(self) -> bool {
        matches!(
            self,
            State::TagName
                | State::BeforeAttrName
                | State::AttrName
                | State::AfterAttrName
                | State::BeforeAttrValue
                | State::AttrValueQuoted
                | State::AttrValueUnquoted
        )
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ResumeState {
    pub(crate) state: State,
    pub(crate) return_state: State,
    pub(crate) rawtext_tag_name: String,
    pub(crate) current_tag_name: String,
    pub(crate) is_closing_tag: bool,
    pub(crate) continued_tag_name: bool,
    pub(crate) in_xml_declaration: bool,
    pub(crate) verbatim: bool,
    pub(crate) verbatim_return_state: Option<State>,
    pub(crate) verbatim_start_token_index: Option<usize>,
    pub(crate) php_block: bool,
    pub(crate) php_block_start_token_index: Option<usize>,
    pub(crate) php_tag: bool,
    pub(crate) attr_php_directive_depth: i32,
}

impl Default for ResumeState {
    fn default() -> Self {
        ResumeState {
            state: State::Data,
            return_state: State::Data,
            rawtext_tag_name: String::new(),
            current_tag_name: String::new(),
            is_closing_tag: false,
            continued_tag_name: false,
            in_xml_declaration: false,
            verbatim: false,
            verbatim_return_state: None,
            verbatim_start_token_index: None,
            php_block: false,
            php_block_start_token_index: None,
            php_tag: false,
            attr_php_directive_depth: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct IgnoreRangeRegion {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) resume: ResumeState,
}

/// Collects `format-ignore-start` / `-end` comment pairs (`lexer/ignore-ranges.ts`).
pub(crate) struct IgnoreRangeCollector<'a> {
    source: &'a str,
    ranges: Vec<IgnoreRangeRegion>,
    depth: usize,
    outer_start: Option<usize>,
}

impl<'a> IgnoreRangeCollector<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        IgnoreRangeCollector {
            source,
            ranges: Vec::new(),
            depth: 0,
            outer_start: None,
        }
    }

    fn handle_comment(
        &mut self,
        blade: bool,
        start: usize,
        end: usize,
        origin: State,
        tag_start: Option<usize>,
        resume: ResumeState,
    ) {
        if origin == State::AttrValueQuoted || (!blade && origin == State::RawText) {
            return;
        }
        let Some(kind) = crate::ignore::comment_kind(&self.source[start..end], blade) else {
            return;
        };
        if kind == crate::ignore::IgnoreKind::Start {
            if self.depth == 0 {
                let tag_state = matches!(
                    origin,
                    State::TagName
                        | State::BeforeAttrName
                        | State::AttrName
                        | State::AfterAttrName
                        | State::BeforeAttrValue
                        | State::AttrValueUnquoted
                );
                self.outer_start = Some(match tag_start {
                    Some(t) if tag_state => t,
                    _ => start,
                });
            }
            self.depth += 1;
            return;
        }
        if kind != crate::ignore::IgnoreKind::End || self.depth == 0 {
            return;
        }
        self.depth -= 1;
        if self.depth != 0 {
            return;
        }
        self.ranges.push(IgnoreRangeRegion {
            start: self.outer_start.unwrap_or(start),
            end,
            resume,
        });
        self.outer_start = None;
    }

    pub(crate) fn finish(mut self, resume: ResumeState, eof: usize) -> Vec<IgnoreRangeRegion> {
        if self.depth > 0
            && let Some(start) = self.outer_start
        {
            self.ranges.push(IgnoreRangeRegion {
                start,
                end: eof,
                resume,
            });
        }
        self.ranges
    }
}

pub(crate) fn collect_ignore_ranges(source: &str) -> Vec<IgnoreRangeRegion> {
    let directives = Directives::accept_all();
    let mut lexer = Lexer::new(source, &directives, &[]);
    lexer.collector = Some(IgnoreRangeCollector::new(source));
    lexer.run();
    let collector = lexer
        .collector
        .take()
        .unwrap_or_else(|| IgnoreRangeCollector::new(source));
    collector.finish(ResumeState::default(), source.len())
}

const ATTR_LIKE_AT_NAME_CONTINUATION: &[u8] = b".-:[]";

pub(crate) struct Lexer<'a> {
    src: &'a [u8],
    text: &'a str,
    pos: usize,
    len: usize,
    state: State,
    return_state: State,
    tokens: Vec<Token>,
    verbatim: bool,
    verbatim_return_state: Option<State>,
    verbatim_start_token_index: Option<usize>,
    php_block: bool,
    php_block_start_token_index: Option<usize>,
    php_tag: bool,
    attr_php_directive_depth: i32,
    rawtext_tag_name: String,
    current_tag_name: String,
    current_tag_start: Option<usize>,
    ignore_ranges: &'a [IgnoreRangeRegion],
    next_ignore_range_index: usize,
    pub(crate) collector: Option<IgnoreRangeCollector<'a>>,
    pending_html_comment_origin: Option<State>,
    pending_html_comment_tag_start: Option<usize>,
    pending_blade_comment_origin: Option<State>,
    pending_blade_comment_tag_start: Option<usize>,
    is_closing_tag: bool,
    continued_tag_name: bool,
    in_xml_declaration: bool,
    directives: &'a Directives,
}

pub(crate) fn tokenize(
    source: &str,
    directives: &Directives,
    ignore_ranges: &[IgnoreRangeRegion],
) -> Vec<Token> {
    Lexer::new(source, directives, ignore_ranges).tokenize()
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(
        source: &'a str,
        directives: &'a Directives,
        ignore_ranges: &'a [IgnoreRangeRegion],
    ) -> Self {
        Lexer {
            src: source.as_bytes(),
            text: source,
            pos: 0,
            len: source.len(),
            state: State::Data,
            return_state: State::Data,
            tokens: Vec::new(),
            verbatim: false,
            verbatim_return_state: None,
            verbatim_start_token_index: None,
            php_block: false,
            php_block_start_token_index: None,
            php_tag: false,
            attr_php_directive_depth: 0,
            rawtext_tag_name: String::new(),
            current_tag_name: String::new(),
            current_tag_start: None,
            ignore_ranges,
            next_ignore_range_index: 0,
            collector: None,
            pending_html_comment_origin: None,
            pending_html_comment_tag_start: None,
            pending_blade_comment_origin: None,
            pending_blade_comment_tag_start: None,
            is_closing_tag: false,
            continued_tag_name: false,
            in_xml_declaration: false,
            directives,
        }
    }

    fn peek_ahead(&self, n: usize) -> Option<u8> {
        let p = self.pos + n;
        if p < self.len {
            Some(self.src[p])
        } else {
            None
        }
    }

    fn lower_slice(&self, a: usize, b: usize) -> String {
        self.text[a..b].to_ascii_lowercase()
    }

    fn is_at_attribute_candidate(&self, name_lower: &str, after_name: usize) -> bool {
        if after_name >= self.len {
            return is_frontend_event_style_at_name(name_lower);
        }
        let immediate = self.src[after_name];
        if ATTR_LIKE_AT_NAME_CONTINUATION.contains(&immediate) {
            return true;
        }
        let mut pos = after_name;
        while pos < self.len && is_space(self.src[pos]) {
            pos += 1;
        }
        if pos < self.len && self.src[pos] == b'=' {
            return true;
        }
        if !is_frontend_event_style_at_name(name_lower) {
            return false;
        }
        pos >= self.len || self.src[pos] == b'>' || self.src[pos] == b'/' || is_space(immediate)
    }

    pub(crate) fn tokenize(mut self) -> Vec<Token> {
        self.run();
        self.tokens
    }

    fn run(&mut self) {
        while self.pos < self.len {
            if self.try_emit_ignore_range() {
                continue;
            }
            match self.state {
                State::Data => self.scan_data(),
                State::RawText => self.scan_rawtext(),
                State::BladeComment => self.scan_blade_comment_content(),
                State::Comment => self.scan_comment(),
                State::TagName => self.scan_tag_name(),
                State::BeforeAttrName => self.scan_before_attr_name(),
                State::AttrName => self.scan_attr_name(),
                State::AfterAttrName => self.scan_after_attr_name(),
                State::BeforeAttrValue => self.scan_before_attr_value(),
                State::AttrValueQuoted => self.scan_attr_value_quoted(),
                State::AttrValueUnquoted => self.scan_attr_value_unquoted(),
            }
        }
        if self.state.in_tag() {
            self.emit(TokenType::SyntheticClose, self.pos, self.pos);
        }
    }

    fn emit(&mut self, kind: TokenType, start: usize, end: usize) {
        self.tokens.push(Token { kind, start, end });
    }

    fn snapshot(&self) -> ResumeState {
        ResumeState {
            state: self.state,
            return_state: self.return_state,
            rawtext_tag_name: self.rawtext_tag_name.clone(),
            current_tag_name: self.current_tag_name.clone(),
            is_closing_tag: self.is_closing_tag,
            continued_tag_name: self.continued_tag_name,
            in_xml_declaration: self.in_xml_declaration,
            verbatim: self.verbatim,
            verbatim_return_state: self.verbatim_return_state,
            verbatim_start_token_index: self.verbatim_start_token_index,
            php_block: self.php_block,
            php_block_start_token_index: self.php_block_start_token_index,
            php_tag: self.php_tag,
            attr_php_directive_depth: self.attr_php_directive_depth,
        }
    }

    fn restore(&mut self, r: &ResumeState) {
        self.state = r.state;
        self.return_state = r.return_state;
        self.rawtext_tag_name.clone_from(&r.rawtext_tag_name);
        self.current_tag_name.clone_from(&r.current_tag_name);
        self.is_closing_tag = r.is_closing_tag;
        self.continued_tag_name = r.continued_tag_name;
        self.in_xml_declaration = r.in_xml_declaration;
        self.verbatim = r.verbatim;
        self.verbatim_return_state = r.verbatim_return_state;
        self.verbatim_start_token_index = r.verbatim_start_token_index;
        self.php_block = r.php_block;
        self.php_block_start_token_index = r.php_block_start_token_index;
        self.php_tag = r.php_tag;
        self.attr_php_directive_depth = r.attr_php_directive_depth;
    }

    fn try_emit_ignore_range(&mut self) -> bool {
        let Some(range) = self.ignore_ranges.get(self.next_ignore_range_index) else {
            return false;
        };
        if self.pos != range.start {
            return false;
        }
        self.emit(TokenType::IgnoreRange, range.start, range.end);
        self.pos = range.end;
        self.restore(&range.resume);
        self.next_ignore_range_index += 1;
        true
    }

    fn next_ignore_range_start(&self) -> Option<usize> {
        self.ignore_ranges
            .get(self.next_ignore_range_index)
            .map(|r| r.start)
    }

    fn record_blade_comment(
        &mut self,
        start: usize,
        end: usize,
        origin: State,
        tag_start: Option<usize>,
    ) {
        if self.collector.is_some() {
            let snap = self.snapshot();
            let Some(collector) = self.collector.as_mut() else {
                return;
            };
            collector.handle_comment(true, start, end, origin, tag_start, snap);
        }
    }

    fn record_html_comment(
        &mut self,
        start: usize,
        end: usize,
        origin: State,
        tag_start: Option<usize>,
    ) {
        if self.collector.is_some() {
            let snap = self.snapshot();
            let Some(collector) = self.collector.as_mut() else {
                return;
            };
            collector.handle_comment(false, start, end, origin, tag_start, snap);
        }
    }

    fn begin_blade_comment_capture(&mut self, origin: State, tag_start: Option<usize>) {
        self.pending_blade_comment_origin = Some(origin);
        self.pending_blade_comment_tag_start = tag_start;
    }

    fn begin_html_comment_capture(&mut self, origin: State, tag_start: Option<usize>) {
        self.pending_html_comment_origin = Some(origin);
        self.pending_html_comment_tag_start = tag_start;
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.len && is_space(self.src[self.pos]) {
            self.pos += 1;
        }
    }

    fn skip_and_emit_whitespace(&mut self) {
        let start = self.pos;
        self.skip_whitespace();
        if start < self.pos {
            self.emit(TokenType::Whitespace, start, self.pos);
        }
    }

    fn skip_to_line_end_inclusive(&mut self) {
        while self.pos < self.len {
            let c = self.src[self.pos];
            self.pos += 1;
            if c == b'\n' || c == b'\r' {
                break;
            }
        }
    }

    fn scan_attribute_php_directive_content(&mut self) -> bool {
        if self.attr_php_directive_depth <= 0 {
            return false;
        }
        let start = self.pos;
        while self.pos < self.len {
            let ch = self.src[self.pos];
            if ch == b'"' || ch == b'\'' {
                self.pos += 1;
                skip_quoted_string(self.src, &mut self.pos, self.len, ch);
                continue;
            }
            if ch == b'`' {
                self.pos += 1;
                skip_backtick_string(self.src, &mut self.pos, self.len);
                continue;
            }
            if ch == b'/' && self.pos + 1 < self.len {
                let next = self.src[self.pos + 1];
                if next == b'/' {
                    self.pos += 2;
                    self.skip_to_line_end_inclusive();
                    continue;
                }
                if next == b'*' {
                    self.pos += 2;
                    skip_block_comment(self.src, &mut self.pos, self.len);
                    continue;
                }
            }
            if ch == b'#' {
                self.pos += 1;
                self.skip_to_line_end_inclusive();
                continue;
            }
            if ch == b'<'
                && self.pos + 2 < self.len
                && self.src[self.pos + 1] == b'<'
                && self.src[self.pos + 2] == b'<'
            {
                self.pos += 3;
                skip_heredoc(self.src, &mut self.pos, self.len);
                continue;
            }
            if ch == b'@' && !self.verbatim && !self.php_tag {
                let mut name_end = self.pos + 1;
                while name_end < self.len
                    && (is_alnum(self.src[name_end]) || self.src[name_end] == b'_')
                {
                    name_end += 1;
                }
                if name_end > self.pos + 1 {
                    let name = self.lower_slice(self.pos + 1, name_end);
                    if name == "php" || name == "endphp" {
                        if start < self.pos {
                            self.emit(TokenType::Text, start, self.pos);
                        }
                        self.return_state = self.state;
                        self.scan_directive();
                        return true;
                    }
                }
            }
            self.pos += 1;
        }
        if start < self.pos {
            self.emit(TokenType::Text, start, self.pos);
        }
        true
    }

    fn skip_line_comment_with_warnings(&mut self) {
        skip_line_comment(self.src, &mut self.pos, self.len);
    }

    fn detect_construct(&self) -> bool {
        if self.pos >= self.len {
            return false;
        }
        let byte = self.src[self.pos];
        let p1 = self.peek_ahead(1);
        if byte == b'{'
            && (p1 == Some(b'{') || (p1 == Some(b'!') && self.peek_ahead(2) == Some(b'!')))
        {
            return true;
        }
        if byte == b'@' && self.pos + 1 < self.len && is_alpha(self.src[self.pos + 1]) {
            return true;
        }
        if byte == b'<' && p1 == Some(b'?') && self.php_tag_start_length(self.pos) > 0 {
            return true;
        }
        false
    }

    fn has_ascii_word_at(&self, pos: usize, word: &str) -> bool {
        if pos + word.len() > self.len {
            return false;
        }
        self.src[pos..pos + word.len()].eq_ignore_ascii_case(word.as_bytes())
    }

    fn next_interesting_tsx(&self, from: usize) -> usize {
        (from..self.len)
            .find(|&i| {
                matches!(
                    self.src[i],
                    b'<' | b'>' | b'"' | b'\'' | b'`' | b'@' | b'\n' | b'\r'
                )
            })
            .unwrap_or(self.len)
    }

    fn next_interesting_balanced(&self, from: usize) -> usize {
        (from..self.len)
            .find(|&i| {
                matches!(
                    self.src[i],
                    b'{' | b'}' | b'(' | b')' | b'"' | b'\'' | b'`' | b'/' | b'@' | b'\n' | b'\r'
                )
            })
            .unwrap_or(self.len)
    }

    fn scan_data(&mut self) {
        let start = self.pos;
        while self.pos < self.len {
            if self.next_ignore_range_start() == Some(self.pos) {
                if start < self.pos {
                    let kind = if self.php_block {
                        TokenType::PhpBlock
                    } else if self.php_tag {
                        TokenType::PhpContent
                    } else {
                        TokenType::Text
                    };
                    self.emit(kind, start, self.pos);
                }
                return;
            }
            let byte = self.src[self.pos];

            if self.php_block {
                if byte == b'\'' || byte == b'"' {
                    self.pos += 1;
                    skip_quoted_string(self.src, &mut self.pos, self.len, byte);
                    continue;
                }
                if byte == b'`' {
                    self.pos += 1;
                    skip_backtick_string(self.src, &mut self.pos, self.len);
                    continue;
                }
                if byte == b'/' && self.pos + 1 < self.len {
                    let next = self.src[self.pos + 1];
                    if next == b'/' {
                        self.pos += 2;
                        self.skip_to_line_end_inclusive();
                        continue;
                    }
                    if next == b'*' {
                        self.pos += 2;
                        skip_block_comment(self.src, &mut self.pos, self.len);
                        continue;
                    }
                }
                if byte == b'#' {
                    self.pos += 1;
                    self.skip_to_line_end_inclusive();
                    continue;
                }
                if byte == b'<'
                    && self.pos + 2 < self.len
                    && self.src[self.pos + 1] == b'<'
                    && self.src[self.pos + 2] == b'<'
                {
                    self.pos += 3;
                    skip_heredoc(self.src, &mut self.pos, self.len);
                    continue;
                }
                if byte == b'@' && self.is_endphp_at(self.pos) {
                    if start < self.pos {
                        self.emit(TokenType::PhpBlock, start, self.pos);
                    }
                    self.scan_directive();
                    return;
                }
                self.pos += 1;
                continue;
            }

            if byte == b'{' && !self.verbatim && !self.php_block && !self.php_tag {
                if self.pos > 0 && self.src[self.pos - 1] == b'@' {
                    self.pos += 1;
                    continue;
                }
                let next1 = self.peek_ahead(1);
                if next1 == Some(b'{') {
                    let next2 = self.peek_ahead(2);
                    let next3 = self.peek_ahead(3);
                    if start < self.pos {
                        self.emit(TokenType::Text, start, self.pos);
                    }
                    if next2 == Some(b'-') && next3 == Some(b'-') {
                        self.begin_blade_comment_capture(State::Data, None);
                        self.scan_blade_comment_start();
                        return;
                    }
                    self.scan_echo();
                    return;
                } else if next1 == Some(b'!')
                    && self.pos + 2 < self.len
                    && self.src[self.pos + 2] == b'!'
                {
                    if start < self.pos {
                        self.emit(TokenType::Text, start, self.pos);
                    }
                    self.scan_raw_echo();
                    return;
                }
                self.pos += 1;
            } else if self.verbatim
                && self.verbatim_return_state == Some(State::RawText)
                && !self.rawtext_tag_name.is_empty()
                && byte == b'<'
                && self.is_rawtext_closing_tag_at(self.pos, &self.rawtext_tag_name.clone())
            {
                self.recover_unclosed_rawtext_verbatim_block(start);
                return;
            } else if byte == b'@' {
                if self.verbatim {
                    if self.is_verbatim_terminator_at(self.pos) {
                        if start < self.pos {
                            self.emit(TokenType::Text, start, self.pos);
                        }
                        self.scan_directive();
                        return;
                    }
                    self.pos += 1;
                    continue;
                }
                if self.php_tag {
                    self.pos += 1;
                    continue;
                }
                if can_start_directive_at(self.src, self.pos, 0) {
                    let next_pos = self.pos + 1;
                    let mut escaped = false;
                    if next_pos < self.len {
                        let nb = self.src[next_pos];
                        if nb == b'@' {
                            escaped = true;
                        } else if nb == b'{' && next_pos + 1 < self.len {
                            let ab = self.src[next_pos + 1];
                            if ab == b'{' || ab == b'!' {
                                escaped = true;
                            }
                        }
                    }
                    if start < self.pos {
                        self.emit(TokenType::Text, start, self.pos);
                    }
                    if escaped {
                        self.emit(TokenType::AtSign, self.pos, self.pos + 1);
                        self.pos += 1;
                        return;
                    }
                    self.scan_directive();
                    return;
                }
                self.pos += 1;
            } else if byte == b'<' && !self.verbatim && !self.php_block && !self.php_tag {
                if self.pos + 9 <= self.len
                    && self.src[self.pos + 1] == b'!'
                    && self.src[self.pos + 2..self.pos + 9].eq_ignore_ascii_case(b"doctype")
                {
                    if start < self.pos {
                        self.emit(TokenType::Text, start, self.pos);
                    }
                    self.scan_doctype();
                    return;
                }
                if self.pos + 3 <= self.len
                    && self.src[self.pos + 1] == b'!'
                    && self.src[self.pos + 2] == b'['
                {
                    if start < self.pos {
                        self.emit(TokenType::Text, start, self.pos);
                    }
                    if self.try_scan_cdata() {
                        return;
                    }
                    self.try_scan_conditional_comment_end();
                    return;
                }
                if self.pos + 4 <= self.len
                    && self.src[self.pos + 1] == b'!'
                    && self.src[self.pos + 2] == b'-'
                    && self.src[self.pos + 3] == b'-'
                {
                    if start < self.pos {
                        self.emit(TokenType::Text, start, self.pos);
                    }
                    if self.try_scan_conditional_comment() {
                        return;
                    }
                    self.state = State::Comment;
                    return;
                }
                if self.pos + 3 <= self.len
                    && self.src[self.pos + 1] == b'!'
                    && self.src[self.pos + 2] == b'-'
                    && (self.pos + 3 >= self.len || self.src[self.pos + 3] != b'-')
                {
                    if start < self.pos {
                        self.emit(TokenType::Text, start, self.pos);
                    }
                    let bogus_start = self.pos;
                    self.pos += 3;
                    while self.pos < self.len {
                        if self.src[self.pos] == b'>' {
                            self.pos += 1;
                            self.emit(TokenType::BogusComment, bogus_start, self.pos);
                            return;
                        }
                        if self.pos + 1 < self.len
                            && self.src[self.pos] == b'-'
                            && self.src[self.pos + 1] == b'>'
                        {
                            self.pos += 2;
                            self.emit(TokenType::BogusComment, bogus_start, self.pos);
                            return;
                        }
                        self.pos += 1;
                    }
                    self.emit(TokenType::BogusComment, bogus_start, self.pos);
                    return;
                }
                if self.pos + 1 < self.len && self.src[self.pos + 1] == b'?' {
                    let scan_start = self.pos;
                    let tokens_before = self.tokens.len();
                    if self.try_scan_decl()
                        || self.try_scan_processing_instruction()
                        || self.try_scan_bogus_comment()
                    {
                        if start < scan_start {
                            self.tokens.insert(
                                tokens_before,
                                Token {
                                    kind: TokenType::Text,
                                    start,
                                    end: scan_start,
                                },
                            );
                        }
                        return;
                    }
                    if self.php_tag_start_length(self.pos) > 0 {
                        if start < self.pos {
                            self.emit(TokenType::Text, start, self.pos);
                        }
                        self.try_scan_php_tag();
                        return;
                    }
                    self.pos += 1;
                    continue;
                }
                if self.pos + 1 < self.len
                    && self.src[self.pos + 1] == b'-'
                    && (self.pos + 2 >= self.len || self.src[self.pos + 2] != b'-')
                {
                    let scan_start = self.pos;
                    let tokens_before = self.tokens.len();
                    if self.try_scan_bogus_comment() {
                        if start < scan_start {
                            self.tokens.insert(
                                tokens_before,
                                Token {
                                    kind: TokenType::Text,
                                    start,
                                    end: scan_start,
                                },
                            );
                        }
                        return;
                    }
                    self.pos += 1;
                    continue;
                }
                let next = self.peek_ahead(1);
                let valid = next
                    .is_some_and(|c| is_alpha(c) || matches!(c, b'/' | b'_' | b'>' | b'{' | b'@'));
                if valid {
                    if start < self.pos {
                        self.emit(TokenType::Text, start, self.pos);
                    }
                    self.scan_tag_open();
                    return;
                }
                self.pos += 1;
            } else if byte == b'?' && self.php_tag {
                if self.is_php_tag_end_at(self.pos) {
                    if start < self.pos {
                        self.emit(TokenType::PhpContent, start, self.pos);
                    }
                    self.emit(TokenType::PhpTagEnd, self.pos, self.pos + 2);
                    self.pos += 2;
                    self.php_tag = false;
                    return;
                }
                self.pos += 1;
            } else {
                self.pos += 1;
            }
        }
        if start < self.pos {
            let kind = if self.php_block {
                TokenType::PhpBlock
            } else if self.php_tag {
                TokenType::PhpContent
            } else {
                TokenType::Text
            };
            self.emit(kind, start, self.pos);
        }
    }

    fn scan_comment(&mut self) {
        let start = self.pos;
        let origin = self
            .pending_html_comment_origin
            .take()
            .unwrap_or(State::Data);
        let tag_start = self.pending_html_comment_tag_start.take();
        self.emit(TokenType::CommentStart, start, start + 4);
        self.pos += 4;
        let content_start = self.pos;
        match find(self.src, b"-->", self.pos) {
            None => {
                if content_start < self.len {
                    self.emit(TokenType::Text, content_start, self.len);
                }
                self.pos = self.len;
                self.state = State::Data;
                self.record_html_comment(start, self.pos, origin, tag_start);
            }
            Some(close) => {
                if content_start < close {
                    self.emit(TokenType::Text, content_start, close);
                }
                self.emit(TokenType::CommentEnd, close, close + 3);
                self.pos = close + 3;
                self.state = State::Data;
                self.record_html_comment(start, self.pos, origin, tag_start);
            }
        }
    }

    fn try_scan_bogus_comment(&mut self) -> bool {
        if self.pos + 1 >= self.len {
            return false;
        }
        let start = self.pos;
        if self.src[self.pos] == b'<' && self.src[self.pos + 1] == b'?' {
            if self.php_tag_start_length(self.pos) > 0 {
                return false;
            }
            self.pos += 2;
            while self.pos < self.len {
                if self.src[self.pos] == b'>' {
                    self.pos += 1;
                    self.emit(TokenType::BogusComment, start, self.pos);
                    return true;
                }
                self.pos += 1;
            }
            self.pos = start;
            return false;
        }
        if self.src[self.pos] == b'<' && self.src[self.pos + 1] == b'-' {
            if self.pos + 2 < self.len && self.src[self.pos + 2] == b'-' {
                return false;
            }
            self.pos += 2;
            while self.pos + 1 < self.len {
                if self.src[self.pos] == b'-' && self.src[self.pos + 1] == b'>' {
                    self.pos += 2;
                    self.emit(TokenType::BogusComment, start, self.pos);
                    return true;
                }
                self.pos += 1;
            }
            self.pos = start;
            return false;
        }
        false
    }

    fn scan_blade_comment_start(&mut self) {
        let start = self.pos;
        self.pos += 4;
        self.emit(TokenType::BladeCommentStart, start, self.pos);
        self.state = State::BladeComment;
    }

    fn scan_blade_comment_content(&mut self) {
        let start = self.pos;
        let comment_start = start - 4;
        let origin = self
            .pending_blade_comment_origin
            .take()
            .unwrap_or(self.return_state);
        let tag_start = self.pending_blade_comment_tag_start.take();
        if self.pos < self.len {
            match find(self.src, b"--}}", self.pos) {
                None => {
                    if start < self.len {
                        self.emit(TokenType::Text, start, self.len);
                    }
                    self.pos = self.len;
                }
                Some(close) => {
                    if close > start {
                        self.emit(TokenType::Text, start, close);
                    }
                    self.emit(TokenType::BladeCommentEnd, close, close + 4);
                    self.pos = close + 4;
                }
            }
        }
        self.state = self.return_state;
        self.return_state = State::Data;
        self.record_blade_comment(comment_start, self.pos, origin, tag_start);
    }

    fn try_scan_conditional_comment(&mut self) -> bool {
        if self.pos + 5 >= self.len || &self.src[self.pos..self.pos + 5] != b"<!--[" {
            return false;
        }
        let start = self.pos;
        let mut scan = self.pos + 5;
        while scan + 1 < self.len {
            if self.src[scan] == b']' && self.src[scan + 1] == b'>' {
                let after = scan + 2;
                if after + 5 <= self.len && &self.src[after..after + 5] == b"<!-->" {
                    return false;
                }
                self.pos = after;
                self.emit(TokenType::ConditionalCommentStart, start, self.pos);
                return true;
            }
            scan += 1;
        }
        self.pos = self.len;
        self.emit(TokenType::ConditionalCommentStart, start, self.pos);
        true
    }

    fn try_scan_conditional_comment_end(&mut self) -> bool {
        if self.pos + 3 > self.len || &self.src[self.pos..self.pos + 3] != b"<![" {
            return false;
        }
        let start = self.pos;
        self.pos += 3;
        while self.pos + 3 < self.len {
            if &self.src[self.pos..self.pos + 4] == b"]-->" {
                self.pos += 4;
                self.emit(TokenType::ConditionalCommentEnd, start, self.pos);
                return true;
            }
            self.pos += 1;
        }
        self.pos = self.len;
        self.emit(TokenType::ConditionalCommentEnd, start, self.pos);
        true
    }

    fn try_scan_cdata(&mut self) -> bool {
        if self.pos + 9 > self.len || &self.src[self.pos..self.pos + 9] != b"<![CDATA[" {
            return false;
        }
        let start = self.pos;
        self.emit(TokenType::CdataStart, start, start + 9);
        self.pos += 9;
        let content_start = self.pos;
        match find(self.src, b"]]>", self.pos) {
            None => {
                if content_start < self.len {
                    self.emit(TokenType::Text, content_start, self.len);
                }
                self.pos = self.len;
            }
            Some(close) => {
                if content_start < close {
                    self.emit(TokenType::Text, content_start, close);
                }
                self.emit(TokenType::CdataEnd, close, close + 3);
                self.pos = close + 3;
            }
        }
        true
    }

    fn try_scan_processing_instruction(&mut self) -> bool {
        if self.pos + 2 > self.len || self.src[self.pos] != b'<' || self.src[self.pos + 1] != b'?' {
            return false;
        }
        if self.pos + 2 >= self.len {
            return false;
        }
        let third = self.src[self.pos + 2];
        if is_space(third) || !is_alpha(third) || self.php_tag_start_length(self.pos) > 0 {
            return false;
        }
        let target_start = self.pos + 2;
        let mut target_end = target_start;
        while target_end < self.len {
            let c = self.src[target_end];
            if is_alnum(c) || matches!(c, b'-' | b'_' | b':') {
                target_end += 1;
            } else {
                break;
            }
        }
        if self.src[target_start..target_end].eq_ignore_ascii_case(b"xml") {
            return false;
        }
        let start = self.pos;
        self.emit(TokenType::PIStart, start, target_end);
        self.pos = target_end;
        let content_start = self.pos;
        while self.pos + 1 < self.len {
            if self.src[self.pos] == b'?' && self.src[self.pos + 1] == b'>' {
                if content_start < self.pos {
                    self.emit(TokenType::Text, content_start, self.pos);
                }
                self.emit(TokenType::PIEnd, self.pos, self.pos + 2);
                self.pos += 2;
                return true;
            }
            self.pos += 1;
        }
        if content_start < self.len {
            self.emit(TokenType::Text, content_start, self.len);
        }
        self.pos = self.len;
        true
    }

    fn try_scan_decl(&mut self) -> bool {
        if self.pos + 5 > self.len || self.src[self.pos] != b'<' || self.src[self.pos + 1] != b'?' {
            return false;
        }
        if !self.has_ascii_word_at(self.pos + 2, "xml") {
            return false;
        }
        if self.pos + 5 < self.len {
            let next = self.src[self.pos + 5];
            if is_alnum(next) || matches!(next, b'_' | b'-' | b':') {
                return false;
            }
        }
        let start = self.pos;
        self.emit(TokenType::DeclStart, start, start + 5);
        self.pos += 5;
        self.in_xml_declaration = true;
        self.state = State::BeforeAttrName;
        true
    }

    fn php_tag_start_length(&self, pos: usize) -> usize {
        if pos + 2 > self.len || self.src[pos] != b'<' || self.src[pos + 1] != b'?' {
            return 0;
        }
        if pos + 5 <= self.len && self.has_ascii_word_at(pos + 2, "php") {
            if pos + 5 < self.len {
                let next = self.src[pos + 5];
                if is_alnum(next) || next == b'_' {
                    return 0;
                }
            }
            return 5;
        }
        if pos + 3 <= self.len && self.src[pos + 2] == b'=' {
            return 3;
        }
        0
    }

    fn is_php_tag_end_at(&self, pos: usize) -> bool {
        pos + 2 <= self.len && self.src[pos] == b'?' && self.src[pos + 1] == b'>'
    }

    fn try_scan_php_tag(&mut self) -> bool {
        if self.pos + 1 >= self.len || self.src[self.pos] != b'<' || self.src[self.pos + 1] != b'?'
        {
            return false;
        }
        let start = self.pos;
        self.pos += 2;
        if self.pos + 3 <= self.len {
            if self.has_ascii_word_at(self.pos, "php") {
                if self.pos + 3 < self.len {
                    let next = self.src[self.pos + 3];
                    if is_alnum(next) || next == b'_' {
                        self.pos = start;
                        return false;
                    }
                }
                self.pos += 3;
            } else if self.pos < self.len && self.src[self.pos] == b'=' {
                self.pos += 1;
            } else {
                self.pos = start;
                return false;
            }
        } else if self.pos < self.len && self.src[self.pos] == b'=' {
            self.pos += 1;
        } else {
            self.pos = start;
            return false;
        }
        self.emit(TokenType::PhpTagStart, start, self.pos);
        self.scan_php_content();
        true
    }

    fn skip_php_line_comment_stopping_at_close(&mut self) {
        while self.pos < self.len {
            let ch = self.src[self.pos];
            if ch == b'\n' || ch == b'\r' {
                self.pos += 1;
                break;
            }
            if self.pos + 2 <= self.len
                && self.src[self.pos] == b'?'
                && self.src[self.pos + 1] == b'>'
            {
                break;
            }
            self.pos += 1;
        }
    }

    fn scan_php_content(&mut self) {
        let start = self.pos;
        loop {
            if self.pos >= self.len {
                if self.pos > start {
                    self.emit(TokenType::PhpContent, start, self.pos);
                }
                return;
            }
            if self.pos + 1 < self.len
                && self.src[self.pos] == b'?'
                && self.src[self.pos + 1] == b'>'
            {
                if self.pos > start {
                    self.emit(TokenType::PhpContent, start, self.pos);
                }
                self.emit(TokenType::PhpTagEnd, self.pos, self.pos + 2);
                self.pos += 2;
                return;
            }
            let byte = self.src[self.pos];
            if byte == b'"' || byte == b'\'' {
                self.pos += 1;
                skip_quoted_string(self.src, &mut self.pos, self.len, byte);
            } else if byte == b'/' && self.pos + 1 < self.len {
                let next = self.src[self.pos + 1];
                if next == b'/' {
                    self.pos += 2;
                    self.skip_php_line_comment_stopping_at_close();
                } else if next == b'*' {
                    self.pos += 2;
                    skip_block_comment(self.src, &mut self.pos, self.len);
                } else {
                    self.pos += 1;
                }
            } else if byte == b'#' {
                self.pos += 1;
                self.skip_php_line_comment_stopping_at_close();
            } else if byte == b'<'
                && self.pos + 2 < self.len
                && self.src[self.pos + 1] == b'<'
                && self.src[self.pos + 2] == b'<'
            {
                self.pos += 3;
                skip_heredoc(self.src, &mut self.pos, self.len);
            } else {
                self.pos += 1;
            }
        }
    }

    fn scan_doctype(&mut self) {
        self.emit(TokenType::DoctypeStart, self.pos, self.pos + 9);
        self.pos += 9;
        self.skip_and_emit_whitespace();
        let content_start = self.pos;
        while self.pos < self.len && self.src[self.pos] != b'>' {
            self.pos += 1;
        }
        if content_start < self.pos {
            self.emit(TokenType::Doctype, content_start, self.pos);
        }
        if self.pos < self.len && self.src[self.pos] == b'>' {
            self.emit(TokenType::DoctypeEnd, self.pos, self.pos + 1);
            self.pos += 1;
        }
    }

    fn scan_tag_open(&mut self) {
        let start = self.pos;
        self.current_tag_name.clear();
        self.current_tag_start = Some(start);
        self.emit(TokenType::LessThan, start, start + 1);
        self.pos += 1;
        if self.pos < self.len && self.src[self.pos] == b'/' {
            self.emit(TokenType::Slash, self.pos, self.pos + 1);
            self.pos += 1;
            self.is_closing_tag = true;
        } else {
            self.is_closing_tag = false;
        }
        self.state = State::TagName;
    }

    fn check_rawtext_mode(&mut self) -> State {
        if self.is_closing_tag {
            return State::Data;
        }
        let lower = self.current_tag_name.to_ascii_lowercase();
        if lower == "script" || lower == "style" {
            self.rawtext_tag_name = lower;
            return State::RawText;
        }
        State::Data
    }

    fn scan_tag_name(&mut self) {
        let start = self.pos;
        if self.continued_tag_name && self.pos < self.len && is_space(self.src[self.pos]) {
            self.continued_tag_name = false;
            self.state = State::BeforeAttrName;
            return;
        }
        if !self.continued_tag_name {
            self.skip_whitespace();
        }
        self.continued_tag_name = false;

        if self.pos >= self.len || self.src[self.pos] == b'>' || self.src[self.pos] == b'/' {
            if start < self.pos {
                self.emit(TokenType::Whitespace, start, self.pos);
            }
            if self.pos < self.len {
                if self.src[self.pos] == b'>' {
                    self.emit(TokenType::GreaterThan, self.pos, self.pos + 1);
                    self.pos += 1;
                    self.state = self.check_rawtext_mode();
                } else {
                    self.emit(TokenType::Slash, self.pos, self.pos + 1);
                    self.pos += 1;
                    if self.pos < self.len && self.src[self.pos] == b'>' {
                        self.emit(TokenType::GreaterThan, self.pos, self.pos + 1);
                        self.pos += 1;
                        self.state = State::Data;
                    } else {
                        self.state = State::BeforeAttrName;
                    }
                }
            } else {
                self.emit(TokenType::SyntheticClose, self.pos, self.pos);
                self.state = State::Data;
            }
            return;
        }

        let name_start = self.pos;
        while self.pos < self.len {
            let ch = self.src[self.pos];
            if ch == b'{' && !self.verbatim && !self.php_block && !self.php_tag {
                if self.pos > name_start {
                    self.emit(TokenType::TagName, name_start, self.pos);
                }
                if self.peek_ahead(1) == Some(b'{') {
                    if self.peek_ahead(2) == Some(b'-') && self.peek_ahead(3) == Some(b'-') {
                        self.return_state = State::TagName;
                        self.continued_tag_name = true;
                        self.begin_blade_comment_capture(State::TagName, self.current_tag_start);
                        self.scan_blade_comment_start();
                        self.scan_blade_comment_content();
                        return;
                    }
                    self.return_state = State::TagName;
                    self.continued_tag_name = true;
                    self.scan_echo();
                    return;
                } else if self.peek_ahead(1) == Some(b'!') && self.peek_ahead(2) == Some(b'!') {
                    self.return_state = State::TagName;
                    self.continued_tag_name = true;
                    self.scan_raw_echo();
                    return;
                }
                break;
            }
            if is_alnum(ch) || matches!(ch, b'-' | b':' | b'_' | b'@' | b'.' | b'[' | b']' | b'$') {
                self.pos += 1;
            } else {
                break;
            }
        }
        if name_start < self.pos {
            let part = self.text[name_start..self.pos].to_string();
            self.current_tag_name.push_str(&part);
            self.emit(TokenType::TagName, name_start, self.pos);
        }
        if self.pos >= self.len {
            self.emit(TokenType::SyntheticClose, self.pos, self.pos);
            self.state = State::Data;
            return;
        }
        let ch = self.src[self.pos];
        if ch == b'<' {
            if self.pos + 1 < self.len && self.src[self.pos + 1] == b'?' && self.try_scan_php_tag()
            {
                self.state = State::TagName;
                self.continued_tag_name = true;
                return;
            }
            if self.try_scan_tsx_generic_type() {
                self.state = State::BeforeAttrName;
                return;
            }
            self.emit(TokenType::SyntheticClose, self.pos, self.pos);
            self.scan_tag_open();
        } else if ch == b'>' {
            self.emit(TokenType::GreaterThan, self.pos, self.pos + 1);
            self.pos += 1;
            self.state = self.check_rawtext_mode();
        } else if ch == b'/' {
            self.emit(TokenType::Slash, self.pos, self.pos + 1);
            self.pos += 1;
            self.skip_whitespace();
            if self.pos < self.len && self.src[self.pos] == b'>' {
                self.emit(TokenType::GreaterThan, self.pos, self.pos + 1);
                self.pos += 1;
                self.state = State::Data;
            } else {
                self.state = State::BeforeAttrName;
            }
        } else {
            self.state = State::BeforeAttrName;
        }
    }

    fn read_at_name(&self, from: usize) -> usize {
        let mut p = from;
        while p < self.len && (is_alnum(self.src[p]) || self.src[p] == b'_') {
            p += 1;
        }
        p
    }

    fn scan_before_attr_name(&mut self) {
        if self.scan_attribute_php_directive_content() {
            return;
        }
        let start = self.pos;
        while self.pos < self.len && is_space(self.src[self.pos]) {
            self.pos += 1;
        }
        if self.is_closing_tag
            && (self.pos >= self.len || !matches!(self.src[self.pos], b'>' | b'/'))
        {
            self.emit(TokenType::SyntheticClose, start, start);
            self.pos = start;
            self.state = State::Data;
            return;
        }
        if start < self.pos {
            self.emit(TokenType::Whitespace, start, self.pos);
        }
        if self.pos >= self.len {
            self.emit(TokenType::SyntheticClose, self.pos, self.pos);
            self.state = State::Data;
            return;
        }
        let ch = self.src[self.pos];

        if ch == b'@' && !self.verbatim && !self.php_block && !self.php_tag {
            let name_start = self.pos + 1;
            let temp = self.read_at_name(name_start);
            if temp > name_start {
                let name_lower = self.lower_slice(name_start, temp);
                if self.is_at_attribute_candidate(&name_lower, temp) {
                } else if self.directives.accepts_all()
                    || self.directives.is_directive_lower(&name_lower)
                {
                    self.return_state = State::BeforeAttrName;
                    self.scan_directive();
                    return;
                }
            }
        }

        if ch == b'<'
            && self.pos + 1 < self.len
            && self.src[self.pos + 1] == b'?'
            && self.try_scan_php_tag()
        {
            self.state = State::BeforeAttrName;
            return;
        }

        if ch == b'<'
            && self.pos + 3 < self.len
            && self.src[self.pos + 1] == b'!'
            && self.src[self.pos + 2] == b'-'
            && self.src[self.pos + 3] == b'-'
        {
            self.begin_html_comment_capture(State::BeforeAttrName, self.current_tag_start);
            self.emit(TokenType::SyntheticClose, self.pos, self.pos);
            self.state = State::Data;
            return;
        }

        if ch == b'<' {
            if self.try_scan_tsx_generic_type() {
                return;
            }
            self.emit(TokenType::SyntheticClose, self.pos, self.pos);
            self.emit(TokenType::LessThan, self.pos, self.pos + 1);
            self.pos += 1;
            self.state = State::TagName;
            return;
        }

        if ch == b'{'
            && !self.verbatim
            && !self.php_block
            && !self.php_tag
            && self.try_scan_jsx_shorthand_attribute()
        {
            return;
        }

        if ch == b'{' && !self.verbatim && !self.php_block && !self.php_tag {
            if self.peek_ahead(1) == Some(b'{') {
                if self.peek_ahead(2) == Some(b'-') && self.peek_ahead(3) == Some(b'-') {
                    self.return_state = State::BeforeAttrName;
                    self.begin_blade_comment_capture(State::BeforeAttrName, self.current_tag_start);
                    self.scan_blade_comment_start();
                    self.scan_blade_comment_content();
                    return;
                }
                self.return_state = State::BeforeAttrName;
                self.scan_echo();
                if self.pos < self.len {
                    let next = self.src[self.pos];
                    if !matches!(
                        next,
                        b' ' | b'\t' | b'\n' | b'\r' | b'>' | b'/' | b'=' | b'<'
                    ) {
                        self.state = State::AttrName;
                        return;
                    }
                }
                return;
            } else if self.peek_ahead(1) == Some(b'!') && self.peek_ahead(2) == Some(b'!') {
                self.return_state = State::BeforeAttrName;
                self.scan_raw_echo();
                return;
            }
        }

        if self.in_xml_declaration
            && ch == b'?'
            && self.pos + 1 < self.len
            && self.src[self.pos + 1] == b'>'
        {
            self.emit(TokenType::DeclEnd, self.pos, self.pos + 2);
            self.pos += 2;
            self.in_xml_declaration = false;
            self.state = State::Data;
            return;
        }

        if ch == b'>' {
            self.emit(TokenType::GreaterThan, self.pos, self.pos + 1);
            self.pos += 1;
            self.state = self.check_rawtext_mode();
        } else if ch == b'/' {
            self.emit(TokenType::Slash, self.pos, self.pos + 1);
            self.pos += 1;
            let ws_start = self.pos;
            while self.pos < self.len && is_space(self.src[self.pos]) {
                self.pos += 1;
            }
            if self.pos < self.len && self.src[self.pos] == b'>' {
                self.emit(TokenType::GreaterThan, self.pos, self.pos + 1);
                self.pos += 1;
            } else if self.pos >= self.len {
                self.emit(TokenType::SyntheticClose, self.pos, self.pos);
            } else {
                self.emit(TokenType::SyntheticClose, ws_start, ws_start);
                self.pos = ws_start;
            }
            self.state = State::Data;
        } else {
            self.state = State::AttrName;
        }
    }

    fn scan_attr_name(&mut self) {
        if self.scan_attribute_php_directive_content() {
            return;
        }
        let start = self.pos;
        let mut kind = TokenType::AttributeName;
        if self.pos < self.len && self.src[self.pos] == b':' {
            kind = match self.src.get(self.pos + 1) {
                Some(b':') => TokenType::EscapedAttribute,
                Some(b'$') => TokenType::ShorthandAttribute,
                _ => TokenType::BoundAttribute,
            };
        }
        loop {
            if self.pos >= self.len {
                break;
            }
            let byte = self.src[self.pos];
            if byte == b'{' && !self.verbatim && !self.php_block && !self.php_tag {
                if self.peek_ahead(1) == Some(b'{') {
                    if self.pos > start {
                        self.emit(kind, start, self.pos);
                    }
                    let saved = self.state;
                    self.return_state = State::Data;
                    if self.peek_ahead(2) == Some(b'-') && self.peek_ahead(3) == Some(b'-') {
                        self.begin_blade_comment_capture(State::AttrName, self.current_tag_start);
                        self.scan_blade_comment_start();
                        self.scan_blade_comment_content();
                    } else {
                        self.scan_echo();
                    }
                    self.state = saved;
                    self.scan_attr_name();
                    return;
                } else if self.peek_ahead(1) == Some(b'!') && self.peek_ahead(2) == Some(b'!') {
                    if self.pos > start {
                        self.emit(kind, start, self.pos);
                    }
                    let saved = self.state;
                    self.return_state = State::Data;
                    self.scan_raw_echo();
                    self.state = saved;
                    self.scan_attr_name();
                    return;
                }
                break;
            }
            if byte == b'<' && self.pos + 1 < self.len && self.src[self.pos + 1] == b'?' {
                if self.pos > start {
                    self.emit(kind, start, self.pos);
                }
                if self.try_scan_php_tag() {
                    self.scan_attr_name();
                    return;
                }
                break;
            }
            if byte == b'@' && !self.verbatim && !self.php_block && !self.php_tag {
                let name_start = self.pos + 1;
                let temp = self.read_at_name(name_start);
                let mut known = false;
                if temp > name_start {
                    let name_lower = self.lower_slice(name_start, temp);
                    known = if self.is_at_attribute_candidate(&name_lower, temp) {
                        false
                    } else {
                        self.directives.accepts_all()
                            || self.directives.is_directive_lower(&name_lower)
                    };
                }
                if known {
                    if self.pos > start {
                        self.emit(kind, start, self.pos);
                    }
                    let saved = self.state;
                    self.return_state = State::Data;
                    self.scan_directive();
                    self.state = saved;
                    self.scan_attr_name();
                    return;
                }
                self.pos += 1;
                continue;
            }
            if is_alnum(byte)
                || matches!(
                    byte,
                    b'-' | b':'
                        | b'_'
                        | b'$'
                        | b'@'
                        | b'.'
                        | b'['
                        | b']'
                        | b'('
                        | b')'
                        | b'*'
                        | b'#'
                )
            {
                self.pos += 1;
            } else {
                break;
            }
        }
        if start < self.pos {
            self.emit(kind, start, self.pos);
        }
        if self.pos >= self.len {
            self.state = State::Data;
            return;
        }
        let ch = self.src[self.pos];
        if ch == b'=' {
            self.emit(TokenType::Equals, self.pos, self.pos + 1);
            self.pos += 1;
            self.state = State::BeforeAttrValue;
        } else if self.in_xml_declaration
            && ch == b'?'
            && self.pos + 1 < self.len
            && self.src[self.pos + 1] == b'>'
        {
            self.emit(TokenType::DeclEnd, self.pos, self.pos + 2);
            self.pos += 2;
            self.in_xml_declaration = false;
            self.state = State::Data;
        } else if ch == b'>' {
            self.emit(TokenType::GreaterThan, self.pos, self.pos + 1);
            self.pos += 1;
            self.state = State::Data;
        } else if ch == b'/' {
            self.emit(TokenType::Slash, self.pos, self.pos + 1);
            self.pos += 1;
            if self.pos < self.len && self.src[self.pos] == b'>' {
                self.emit(TokenType::GreaterThan, self.pos, self.pos + 1);
                self.pos += 1;
                self.state = State::Data;
            } else {
                self.state = State::BeforeAttrName;
            }
        } else if is_space(ch) {
            self.state = State::AfterAttrName;
        } else if ch == b'<' {
            self.state = State::BeforeAttrName;
        } else {
            let w = utf8_len(ch);
            self.emit(TokenType::Text, self.pos, self.pos + w);
            self.pos += w;
            self.state = State::BeforeAttrName;
        }
    }

    fn scan_after_attr_name(&mut self) {
        if self.scan_attribute_php_directive_content() {
            return;
        }
        self.skip_and_emit_whitespace();
        if self.pos >= self.len {
            self.state = State::Data;
            return;
        }
        let ch = self.src[self.pos];
        if ch == b'=' {
            self.emit(TokenType::Equals, self.pos, self.pos + 1);
            self.pos += 1;
            self.state = State::BeforeAttrValue;
        } else if self.in_xml_declaration
            && ch == b'?'
            && self.pos + 1 < self.len
            && self.src[self.pos + 1] == b'>'
        {
            self.emit(TokenType::DeclEnd, self.pos, self.pos + 2);
            self.pos += 2;
            self.in_xml_declaration = false;
            self.state = State::Data;
        } else if ch == b'>' {
            self.emit(TokenType::GreaterThan, self.pos, self.pos + 1);
            self.pos += 1;
            self.state = State::Data;
        } else if ch == b'/' {
            self.emit(TokenType::Slash, self.pos, self.pos + 1);
            self.pos += 1;
            if self.pos < self.len && self.src[self.pos] == b'>' {
                self.emit(TokenType::GreaterThan, self.pos, self.pos + 1);
                self.pos += 1;
                self.state = State::Data;
            } else {
                self.state = State::BeforeAttrName;
            }
        } else {
            self.state = State::BeforeAttrName;
        }
    }

    fn scan_before_attr_value(&mut self) {
        if self.scan_attribute_php_directive_content() {
            return;
        }
        self.skip_and_emit_whitespace();
        if self.pos >= self.len {
            self.state = State::Data;
            return;
        }
        let ch = self.src[self.pos];
        if ch == b'{' && !self.verbatim && !self.php_block && !self.php_tag {
            if self.peek_ahead(1) == Some(b'{') {
                self.return_state = State::BeforeAttrName;
                self.scan_echo();
                self.state = State::BeforeAttrName;
                return;
            } else if self.peek_ahead(1) == Some(b'!') && self.peek_ahead(2) == Some(b'!') {
                self.return_state = State::BeforeAttrName;
                self.scan_raw_echo();
                self.state = State::BeforeAttrName;
                return;
            }
            if self.scan_jsx_attribute_value() {
                self.state = State::BeforeAttrName;
                return;
            }
            self.state = State::AttrValueUnquoted;
            return;
        }
        if ch == b'(' && self.pos + 1 < self.len && self.src[self.pos + 1] == b'{' {
            if self.scan_jsx_attribute_value() {
                self.state = State::BeforeAttrName;
                return;
            }
            self.state = State::AttrValueUnquoted;
            return;
        }
        if ch == b'"' || ch == b'\'' {
            self.state = State::AttrValueQuoted;
        } else if self.in_xml_declaration
            && ch == b'?'
            && self.pos + 1 < self.len
            && self.src[self.pos + 1] == b'>'
        {
            self.emit(TokenType::DeclEnd, self.pos, self.pos + 2);
            self.pos += 2;
            self.in_xml_declaration = false;
            self.state = State::Data;
        } else if ch == b'>' {
            self.emit(TokenType::GreaterThan, self.pos, self.pos + 1);
            self.pos += 1;
            self.state = State::Data;
        } else {
            self.state = State::AttrValueUnquoted;
        }
    }

    fn scan_attr_value_quoted(&mut self) {
        if self.scan_attribute_php_directive_content() {
            return;
        }
        let quote = self.src[self.pos];
        self.emit(TokenType::Quote, self.pos, self.pos + 1);
        self.pos += 1;
        let mut value_start = self.pos;
        loop {
            if self.pos >= self.len {
                if self.pos > value_start {
                    self.emit(TokenType::AttributeValue, value_start, self.pos);
                }
                self.state = State::BeforeAttrName;
                return;
            }
            let next = (self.pos..self.len).find(|&i| {
                let b = self.src[i];
                b == quote || b == b'{' || b == b'@'
            });
            let Some(next) = next else {
                self.pos = self.len;
                if self.pos > value_start {
                    self.emit(TokenType::AttributeValue, value_start, self.pos);
                }
                self.state = State::BeforeAttrName;
                return;
            };
            self.pos = next;
            let byte = self.src[self.pos];
            if byte == quote {
                let mut backslashes = 0;
                let mut check = self.pos;
                while check > value_start && self.src[check - 1] == b'\\' {
                    backslashes += 1;
                    check -= 1;
                }
                if backslashes % 2 == 1 {
                    self.pos += 1;
                    continue;
                }
                if self.pos > value_start {
                    self.emit(TokenType::AttributeValue, value_start, self.pos);
                }
                self.emit(TokenType::Quote, self.pos, self.pos + 1);
                self.pos += 1;
                self.state = State::BeforeAttrName;
                return;
            } else if byte == b'{' && !self.verbatim && !self.php_block && !self.php_tag {
                let saved = self.pos;
                if self.peek_ahead(1) == Some(b'{') {
                    if saved > value_start {
                        self.emit(TokenType::AttributeValue, value_start, saved);
                    }
                    self.return_state = State::AttrValueQuoted;
                    if self.peek_ahead(2) == Some(b'-') && self.peek_ahead(3) == Some(b'-') {
                        self.begin_blade_comment_capture(State::AttrValueQuoted, None);
                        self.scan_blade_comment_start();
                        self.scan_blade_comment_content();
                    } else {
                        self.scan_echo();
                    }
                    value_start = self.pos;
                    continue;
                } else if self.peek_ahead(1) == Some(b'!') && self.peek_ahead(2) == Some(b'!') {
                    if saved > value_start {
                        self.emit(TokenType::AttributeValue, value_start, saved);
                    }
                    self.return_state = State::AttrValueQuoted;
                    self.scan_raw_echo();
                    value_start = self.pos;
                    continue;
                }
                self.pos = saved + 1;
            } else if byte == b'@' && !self.verbatim && !self.php_block && !self.php_tag {
                let can_start = self.pos == value_start || !is_alnum(self.src[self.pos - 1]);
                if can_start {
                    let saved = self.pos;
                    if saved > value_start {
                        self.emit(TokenType::AttributeValue, value_start, saved);
                    }
                    self.return_state = State::AttrValueQuoted;
                    self.scan_directive();
                    value_start = self.pos;
                    continue;
                }
                self.pos += 1;
            } else {
                self.pos += 1;
            }
        }
    }

    fn scan_attr_value_unquoted(&mut self) {
        if self.scan_attribute_php_directive_content() {
            return;
        }
        let mut value_start = self.pos;
        loop {
            if self.pos >= self.len {
                break;
            }
            let byte = self.src[self.pos];
            if self.in_xml_declaration
                && byte == b'?'
                && self.pos + 1 < self.len
                && self.src[self.pos + 1] == b'>'
            {
                break;
            }
            if is_space(byte) || byte == b'>' {
                break;
            }
            if byte == b'<' && self.pos + 1 < self.len && self.src[self.pos + 1] == b'?' {
                let saved = self.pos;
                if saved > value_start {
                    self.emit(TokenType::AttributeValue, value_start, saved);
                }
                if self.try_scan_php_tag() {
                    value_start = self.pos;
                    continue;
                }
                break;
            }
            if byte == b'{' && !self.verbatim && !self.php_block && !self.php_tag {
                if self.peek_ahead(1) == Some(b'{') {
                    if self.pos > value_start {
                        self.emit(TokenType::AttributeValue, value_start, self.pos);
                    }
                    self.return_state = State::AttrValueUnquoted;
                    if self.peek_ahead(2) == Some(b'-') && self.peek_ahead(3) == Some(b'-') {
                        self.begin_blade_comment_capture(
                            State::AttrValueUnquoted,
                            self.current_tag_start,
                        );
                        self.scan_blade_comment_start();
                        self.scan_blade_comment_content();
                    } else {
                        self.scan_echo();
                    }
                    value_start = self.pos;
                    continue;
                } else if self.peek_ahead(1) == Some(b'!') && self.peek_ahead(2) == Some(b'!') {
                    if self.pos > value_start {
                        self.emit(TokenType::AttributeValue, value_start, self.pos);
                    }
                    self.return_state = State::AttrValueUnquoted;
                    self.scan_raw_echo();
                    value_start = self.pos;
                    continue;
                }
                self.pos += 1;
                continue;
            }
            if byte == b'@' && !self.verbatim && !self.php_block && !self.php_tag {
                let can_start = self.pos == value_start || !is_alnum(self.src[self.pos - 1]);
                if can_start {
                    if self.pos > value_start {
                        self.emit(TokenType::AttributeValue, value_start, self.pos);
                    }
                    self.return_state = State::AttrValueUnquoted;
                    self.scan_directive();
                    value_start = self.pos;
                    continue;
                }
                self.pos += 1;
                continue;
            }
            self.pos += 1;
        }
        if value_start < self.pos {
            self.emit(TokenType::AttributeValue, value_start, self.pos);
        }
        self.state = State::BeforeAttrName;
    }

    fn scan_directive(&mut self) {
        let start = self.pos;
        self.pos += 1;
        let name_start = self.pos;
        let pos = self.read_at_name(name_start);
        if pos == name_start {
            self.emit(TokenType::Text, start, start + 1);
            self.state = self.return_state;
            self.return_state = State::Data;
            return;
        }
        let name_lower = self.lower_slice(name_start, pos);
        let is_raw_block = is_verbatim_start(&name_lower) || is_verbatim_end(&name_lower);
        if !is_raw_block && !self.directives.is_directive_lower(&name_lower) {
            self.pos = pos;
            self.emit(TokenType::Text, start, pos);
            self.state = self.return_state;
            self.return_state = State::Data;
            return;
        }
        let mut arg_pos = pos;
        while arg_pos < self.len && is_space(self.src[arg_pos]) {
            arg_pos += 1;
        }
        let has_args = arg_pos < self.len && self.src[arg_pos] == b'(';
        self.pos = pos;
        let in_tag = self.state.in_tag();

        if name_lower == "php" && !has_args && !in_tag {
            self.php_block_start_token_index = Some(self.tokens.len());
            self.emit(TokenType::PhpBlockStart, start, pos);
            self.php_block = true;
            self.state = self.return_state;
            self.return_state = State::Data;
            return;
        }
        if name_lower == "php" && !has_args && in_tag {
            self.emit(TokenType::Directive, start, pos);
            self.attr_php_directive_depth += 1;
            self.state = self.return_state;
            self.return_state = State::Data;
            return;
        }
        if name_lower == "endphp" && !in_tag {
            self.emit(TokenType::PhpBlockEnd, start, pos);
            if self.php_block {
                self.php_block = false;
                self.php_block_start_token_index = None;
            }
            self.state = self.return_state;
            self.return_state = State::Data;
            return;
        }
        if name_lower == "endphp" && in_tag && self.attr_php_directive_depth > 0 {
            self.attr_php_directive_depth -= 1;
        }
        if is_verbatim_start(&name_lower) {
            self.verbatim_start_token_index = Some(self.tokens.len());
            self.emit(TokenType::VerbatimStart, start, pos);
            self.verbatim_return_state = Some(self.state);
            self.verbatim = true;
            self.state = State::Data;
            self.return_state = State::Data;
            return;
        }
        if is_verbatim_end(&name_lower) {
            self.emit(TokenType::VerbatimEnd, start, pos);
            self.state = self.verbatim_return_state.take().unwrap_or(State::Data);
            self.verbatim = false;
            self.verbatim_start_token_index = None;
            self.return_state = State::Data;
            return;
        }
        self.emit(TokenType::Directive, start, pos);
        if has_args && arg_pos > pos {
            self.emit(TokenType::Whitespace, pos, arg_pos);
            self.pos = arg_pos;
        }
        if has_args {
            self.scan_directive_args();
        }
        self.state = self.return_state;
        self.return_state = State::Data;
    }

    fn scan_directive_args(&mut self) {
        let start = self.pos;
        if self.pos >= self.len || self.src[self.pos] != b'(' {
            return;
        }
        self.pos += 1;
        let mut depth = 1;
        let mut first_recovery_line_break: Option<usize> = None;
        while self.pos < self.len && depth > 0 {
            let byte = self.src[self.pos];
            if byte == b'\n' && depth == 1 && first_recovery_line_break.is_none() {
                first_recovery_line_break = Some(self.pos + 1);
            }
            if byte == b'\'' || byte == b'"' {
                self.pos += 1;
                skip_quoted_string(self.src, &mut self.pos, self.len, byte);
            } else if byte == b'`' {
                self.pos += 1;
                skip_backtick_string(self.src, &mut self.pos, self.len);
            } else if byte == b'<'
                && self.peek_ahead(1) == Some(b'<')
                && self.peek_ahead(2) == Some(b'<')
            {
                self.pos += 3;
                skip_heredoc(self.src, &mut self.pos, self.len);
            } else if byte == b'/' && self.peek_ahead(1) == Some(b'*') {
                self.pos += 2;
                skip_block_comment(self.src, &mut self.pos, self.len);
            } else if byte == b'/' && self.peek_ahead(1) == Some(b'/') {
                self.pos += 2;
                self.skip_line_comment_with_warnings();
            } else if byte == b'#' {
                self.pos += 1;
                self.skip_line_comment_with_warnings();
            } else if byte == b'(' {
                depth += 1;
                self.pos += 1;
            } else if byte == b')' {
                depth -= 1;
                self.pos += 1;
            } else {
                self.pos += 1;
            }
        }
        if depth > 0 {
            let fallback = first_recovery_line_break.map_or(self.pos, |b| b.min(self.len));
            self.pos = fallback;
            self.emit(TokenType::DirectiveArgs, start, fallback);
            return;
        }
        self.emit(TokenType::DirectiveArgs, start, self.pos);
    }

    fn is_verbatim_terminator_at(&self, pos: usize) -> bool {
        if pos >= self.len || self.src[pos] != b'@' {
            return false;
        }
        let end = self.read_at_name(pos + 1);
        end > pos + 1 && is_verbatim_end(&self.lower_slice(pos + 1, end))
    }

    fn is_endphp_at(&self, pos: usize) -> bool {
        if pos + 7 > self.len || self.src[pos] != b'@' || !self.has_ascii_word_at(pos + 1, "endphp")
        {
            return false;
        }
        if pos + 7 < self.len {
            let next = self.src[pos + 7];
            if is_alnum(next) || next == b'_' {
                return false;
            }
        }
        true
    }

    fn scan_echo_like(
        &mut self,
        open: usize,
        start_kind: TokenType,
        end_kind: TokenType,
        close: &[u8],
    ) {
        let start = self.pos;
        self.emit(start_kind, start, start + open);
        self.pos += open;
        let content_start = self.pos;
        while self.pos < self.len {
            let byte = self.src[self.pos];
            if byte == b'\'' || byte == b'"' {
                self.pos += 1;
                skip_quoted_string(self.src, &mut self.pos, self.len, byte);
            } else if byte == b'`' {
                self.pos += 1;
                skip_backtick_string(self.src, &mut self.pos, self.len);
            } else if byte == b'<'
                && self.peek_ahead(1) == Some(b'<')
                && self.peek_ahead(2) == Some(b'<')
            {
                self.pos += 3;
                skip_heredoc(self.src, &mut self.pos, self.len);
            } else if byte == b'/' && self.peek_ahead(1) == Some(b'*') {
                self.pos += 2;
                skip_block_comment(self.src, &mut self.pos, self.len);
            } else if byte == b'/' && self.peek_ahead(1) == Some(b'/') {
                self.pos += 2;
                self.skip_line_comment_with_warnings();
            } else if byte == b'#' {
                self.pos += 1;
                self.skip_line_comment_with_warnings();
            } else if byte == b'{' || byte == b'@' || byte == b'<' {
                if self.detect_construct() {
                    if content_start < self.pos {
                        self.emit(TokenType::EchoContent, content_start, self.pos);
                    }
                    self.state = self.return_state;
                    self.return_state = State::Data;
                    return;
                }
                self.pos += 1;
            } else if self.src[self.pos..].starts_with(close) {
                if content_start < self.pos {
                    self.emit(TokenType::EchoContent, content_start, self.pos);
                }
                self.emit(end_kind, self.pos, self.pos + close.len());
                self.pos += close.len();
                self.state = self.return_state;
                self.return_state = State::Data;
                return;
            } else {
                self.pos += 1;
            }
        }
        if content_start < self.pos {
            self.emit(TokenType::EchoContent, content_start, self.pos);
        }
        self.pos = self.len;
        self.state = self.return_state;
        self.return_state = State::Data;
    }

    fn scan_echo(&mut self) {
        if self.peek_ahead(0) == Some(b'{')
            && self.peek_ahead(1) == Some(b'{')
            && self.peek_ahead(2) == Some(b'{')
        {
            self.scan_echo_like(
                3,
                TokenType::TripleEchoStart,
                TokenType::TripleEchoEnd,
                b"}}}",
            );
            return;
        }
        self.scan_echo_like(2, TokenType::EchoStart, TokenType::EchoEnd, b"}}");
    }

    fn scan_raw_echo(&mut self) {
        self.scan_echo_like(3, TokenType::RawEchoStart, TokenType::RawEchoEnd, b"!!}");
    }

    fn try_scan_jsx_shorthand_attribute(&mut self) -> bool {
        if self.pos >= self.len || self.src[self.pos] != b'{' {
            return false;
        }
        if self.pos + 1 < self.len && matches!(self.src[self.pos + 1], b'{' | b'!') {
            return false;
        }
        let start = self.pos;
        let original = self.pos;
        self.pos += 1;
        let mut fast = self.pos;
        while fast < self.len {
            let byte = self.src[fast];
            if byte == b'}' {
                self.pos = fast + 1;
                self.emit(TokenType::JsxShorthandAttribute, start, self.pos);
                self.state = State::BeforeAttrName;
                return true;
            }
            if matches!(byte, b'{' | b'\'' | b'"' | b'`' | b'/') {
                break;
            }
            if matches!(byte, b'\n' | b'\r' | b'@') {
                self.pos = original;
                return false;
            }
            fast += 1;
        }
        self.scan_balanced_js_like(
            start,
            TokenType::JsxShorthandAttribute,
            original,
            1,
            0,
            false,
            true,
            true,
        )
    }

    fn scan_jsx_attribute_value(&mut self) -> bool {
        let start = self.pos;
        let original = self.pos;
        let starts_with_paren = self.src[self.pos] == b'(';
        self.pos += 1;
        let brace_depth = i32::from(!starts_with_paren);
        let paren_depth = i32::from(starts_with_paren);
        if brace_depth == 1 && paren_depth == 0 {
            let mut fast = self.pos;
            while fast < self.len {
                let byte = self.src[fast];
                if byte == b'}' {
                    self.pos = fast + 1;
                    self.emit(TokenType::JsxAttributeValue, start, self.pos);
                    return true;
                }
                if matches!(byte, b'{' | b'(' | b'\'' | b'"' | b'`' | b'/') {
                    break;
                }
                if matches!(byte, b'\n' | b'\r' | b'@') {
                    self.pos = original;
                    return false;
                }
                fast += 1;
            }
        }
        self.scan_balanced_js_like(
            start,
            TokenType::JsxAttributeValue,
            original,
            brace_depth,
            paren_depth,
            true,
            true,
            false,
        )
    }

    fn try_scan_tsx_generic_type(&mut self) -> bool {
        if self.pos >= self.len || self.src[self.pos] != b'<' {
            return false;
        }
        if self.pos + 1 < self.len && self.src[self.pos + 1] == b'?' {
            return false;
        }
        let start = self.pos;
        let original = self.pos;
        self.pos += 1;
        if self.pos < self.len {
            let nb = self.src[self.pos];
            let pascal = self
                .current_tag_name
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_uppercase);
            let mut looks = nb == b'{' || nb.is_ascii_uppercase() || nb == b'_';
            if pascal {
                looks = looks
                    || nb.is_ascii_lowercase()
                    || nb.is_ascii_digit()
                    || matches!(nb, b'[' | b'"' | b'\'' | b'(' | b' ' | b'\t');
            }
            if !looks {
                self.pos = original;
                return false;
            }
        }
        let mut depth = 1;
        loop {
            if self.pos >= self.len {
                self.pos = original;
                return false;
            }
            let next = self.next_interesting_tsx(self.pos);
            if next >= self.len {
                self.pos = original;
                return false;
            }
            self.pos = next;
            let byte = self.src[self.pos];
            match byte {
                b'<' => {
                    depth += 1;
                    self.pos += 1;
                }
                b'>' => {
                    if self.pos > 0 && self.src[self.pos - 1] == b'=' {
                        self.pos += 1;
                        continue;
                    }
                    depth -= 1;
                    self.pos += 1;
                    if depth == 0 {
                        self.emit(TokenType::TsxGenericType, start, self.pos);
                        return true;
                    }
                }
                b'\n' | b'\r' | b'@' => {
                    self.pos = original;
                    return false;
                }
                b'"' | b'\'' => {
                    self.pos += 1;
                    skip_quoted_string(self.src, &mut self.pos, self.len, byte);
                }
                b'`' => {
                    self.pos += 1;
                    skip_template_literal(self.src, &mut self.pos, self.len);
                }
                _ => self.pos += 1,
            }
        }
    }

    fn scan_balanced_js_like(
        &mut self,
        start: usize,
        kind: TokenType,
        original: usize,
        mut brace_depth: i32,
        mut paren_depth: i32,
        track_parens: bool,
        abort_on_directive: bool,
        abort_on_newline: bool,
    ) -> bool {
        loop {
            if self.pos >= self.len {
                self.emit(kind, start, self.pos);
                return true;
            }
            let next = self.next_interesting_balanced(self.pos);
            if next >= self.len {
                self.pos = self.len;
                self.emit(kind, start, self.pos);
                return true;
            }
            self.pos = next;
            let byte = self.src[self.pos];
            match byte {
                b'{' => {
                    brace_depth += 1;
                    self.pos += 1;
                }
                b'}' => {
                    brace_depth -= 1;
                    self.pos += 1;
                    if brace_depth == 0 && (!track_parens || paren_depth == 0) {
                        self.emit(kind, start, self.pos);
                        return true;
                    }
                }
                b'(' if track_parens => {
                    paren_depth += 1;
                    self.pos += 1;
                }
                b')' if track_parens => {
                    paren_depth -= 1;
                    self.pos += 1;
                    if brace_depth == 0 && paren_depth == 0 {
                        self.emit(kind, start, self.pos);
                        return true;
                    }
                }
                b'"' | b'\'' => {
                    self.pos += 1;
                    skip_quoted_string(self.src, &mut self.pos, self.len, byte);
                }
                b'`' => {
                    self.pos += 1;
                    skip_template_literal(self.src, &mut self.pos, self.len);
                }
                b'/' => {
                    if self.pos + 1 < self.len {
                        let n = self.src[self.pos + 1];
                        if n == b'/' {
                            self.pos += 2;
                            while self.pos < self.len {
                                let ch = self.src[self.pos];
                                if ch == b'\n' || ch == b'\r' {
                                    self.pos += 1;
                                    if ch == b'\r'
                                        && self.pos < self.len
                                        && self.src[self.pos] == b'\n'
                                    {
                                        self.pos += 1;
                                    }
                                    break;
                                }
                                self.pos += 1;
                            }
                            continue;
                        }
                        if n == b'*' {
                            self.pos += 2;
                            skip_block_comment(self.src, &mut self.pos, self.len);
                            continue;
                        }
                    }
                    self.pos += 1;
                }
                b'@' if abort_on_directive => {
                    self.pos = original;
                    return false;
                }
                b'\n' | b'\r' if abort_on_newline => {
                    self.pos = original;
                    return false;
                }
                _ => self.pos += 1,
            }
        }
    }

    fn scan_rawtext(&mut self) {
        let start = self.pos;
        let tag_name = self.rawtext_tag_name.clone();
        let tag_len = tag_name.len();
        while self.pos < self.len {
            if self.next_ignore_range_start() == Some(self.pos) {
                if start < self.pos {
                    let kind = if self.php_block {
                        TokenType::PhpBlock
                    } else {
                        TokenType::Text
                    };
                    self.emit(kind, start, self.pos);
                }
                return;
            }
            let byte = self.src[self.pos];
            if self.php_block {
                if byte == b'\'' || byte == b'"' {
                    self.pos += 1;
                    skip_quoted_string(self.src, &mut self.pos, self.len, byte);
                    continue;
                }
                if byte == b'`' {
                    self.pos += 1;
                    skip_backtick_string(self.src, &mut self.pos, self.len);
                    continue;
                }
                if byte == b'/' && self.pos + 1 < self.len {
                    let next = self.src[self.pos + 1];
                    if next == b'/' {
                        self.pos += 2;
                        self.skip_to_line_end_inclusive();
                        continue;
                    }
                    if next == b'*' {
                        self.pos += 2;
                        skip_block_comment(self.src, &mut self.pos, self.len);
                        continue;
                    }
                }
                if byte == b'#' {
                    self.pos += 1;
                    self.skip_to_line_end_inclusive();
                    continue;
                }
                if byte == b'<' && self.is_rawtext_closing_tag_at(self.pos, &tag_name) {
                    self.recover_unclosed_rawtext_php_block(start);
                    return;
                }
                if byte == b'<'
                    && self.pos + 2 < self.len
                    && self.src[self.pos + 1] == b'<'
                    && self.src[self.pos + 2] == b'<'
                {
                    self.pos += 3;
                    skip_heredoc(self.src, &mut self.pos, self.len);
                    continue;
                }
                if byte == b'@' && self.is_endphp_at(self.pos) {
                    if start < self.pos {
                        self.emit(TokenType::PhpBlock, start, self.pos);
                    }
                    self.return_state = State::RawText;
                    self.scan_directive();
                    return;
                }
                self.pos += 1;
                continue;
            }
            if byte == b'{' && !self.verbatim {
                let next1 = self.peek_ahead(1);
                if next1 == Some(b'{') {
                    let next2 = self.peek_ahead(2);
                    let next3 = self.peek_ahead(3);
                    if start < self.pos {
                        self.emit(TokenType::Text, start, self.pos);
                    }
                    self.return_state = State::RawText;
                    if next2 == Some(b'-') && next3 == Some(b'-') {
                        self.begin_blade_comment_capture(State::RawText, None);
                        self.scan_blade_comment_start();
                        return;
                    }
                    self.scan_echo();
                    return;
                } else if next1 == Some(b'!')
                    && self.pos + 2 < self.len
                    && self.src[self.pos + 2] == b'!'
                {
                    if start < self.pos {
                        self.emit(TokenType::Text, start, self.pos);
                    }
                    self.return_state = State::RawText;
                    self.scan_raw_echo();
                    return;
                }
                self.pos += 1;
                continue;
            }
            if byte == b'@' && !self.verbatim && !self.php_block {
                if can_start_directive_at(self.src, self.pos, 0) {
                    let next_pos = self.pos + 1;
                    let mut escaped = false;
                    if next_pos < self.len {
                        let nb = self.src[next_pos];
                        if nb == b'@' {
                            escaped = true;
                        } else if nb == b'{' && next_pos + 1 < self.len {
                            let ab = self.src[next_pos + 1];
                            if ab == b'{' || ab == b'!' {
                                escaped = true;
                            }
                        }
                    }
                    if start < self.pos {
                        self.emit(TokenType::Text, start, self.pos);
                    }
                    if escaped {
                        self.emit(TokenType::AtSign, self.pos, self.pos + 1);
                        self.pos += 1;
                        return;
                    }
                    self.return_state = State::RawText;
                    self.scan_directive();
                    return;
                }
                self.pos += 1;
                continue;
            }
            if byte == b'<' {
                if self.pos + 2 + tag_len <= self.len && self.src[self.pos + 1] == b'/' {
                    let potential = &self.text[self.pos + 2..self.pos + 2 + tag_len];
                    if potential.eq_ignore_ascii_case(&tag_name) {
                        let potential = potential.to_string();
                        let after = self.pos + 2 + tag_len;
                        if after < self.len {
                            let ac = self.src[after];
                            if ac == b'>' || is_space(ac) {
                                if start < self.pos {
                                    self.emit(TokenType::Text, start, self.pos);
                                }
                                self.rawtext_tag_name.clear();
                                self.emit(TokenType::LessThan, self.pos, self.pos + 1);
                                self.pos += 1;
                                self.emit(TokenType::Slash, self.pos, self.pos + 1);
                                self.pos += 1;
                                self.emit(TokenType::TagName, self.pos, self.pos + tag_len);
                                self.current_tag_name = potential;
                                self.is_closing_tag = true;
                                self.pos += tag_len;
                                let mut peek = self.pos;
                                while peek < self.len && is_space(self.src[peek]) {
                                    peek += 1;
                                }
                                if peek < self.len && self.src[peek] == b'>' {
                                    if self.pos < peek {
                                        self.emit(TokenType::Whitespace, self.pos, peek);
                                        self.pos = peek;
                                    }
                                    self.emit(TokenType::GreaterThan, self.pos, self.pos + 1);
                                    self.pos += 1;
                                } else {
                                    self.emit(TokenType::SyntheticClose, self.pos, self.pos);
                                }
                                self.state = State::Data;
                                return;
                            }
                        } else if after == self.len {
                            if start < self.pos {
                                self.emit(TokenType::Text, start, self.pos);
                            }
                            self.rawtext_tag_name.clear();
                            self.emit(TokenType::LessThan, self.pos, self.pos + 1);
                            self.pos += 1;
                            self.emit(TokenType::Slash, self.pos, self.pos + 1);
                            self.pos += 1;
                            self.emit(TokenType::TagName, self.pos, self.pos + tag_len);
                            self.pos += tag_len;
                            self.state = State::Data;
                            return;
                        }
                    }
                }
                self.pos += 1;
                continue;
            }
            self.pos += 1;
        }
        if start < self.pos {
            let kind = if self.php_block {
                TokenType::PhpBlock
            } else {
                TokenType::Text
            };
            self.emit(kind, start, self.pos);
        }
        self.rawtext_tag_name.clear();
    }

    fn is_rawtext_closing_tag_at(&self, pos: usize, tag_name: &str) -> bool {
        let tag_len = tag_name.len();
        if pos + 2 + tag_len > self.len || self.src[pos] != b'<' || self.src[pos + 1] != b'/' {
            return false;
        }
        if !self.src[pos + 2..pos + 2 + tag_len].eq_ignore_ascii_case(tag_name.as_bytes()) {
            return false;
        }
        let after = pos + 2 + tag_len;
        if after == self.len {
            return true;
        }
        let c = self.src[after];
        c == b'>' || is_space(c)
    }

    fn recover_unclosed_rawtext_php_block(&mut self, content_start: usize) {
        if let Some(i) = self.php_block_start_token_index {
            self.tokens[i].kind = TokenType::Text;
        }
        if content_start < self.pos {
            self.emit(TokenType::Text, content_start, self.pos);
        }
        self.php_block = false;
        self.php_block_start_token_index = None;
    }

    fn recover_unclosed_rawtext_verbatim_block(&mut self, content_start: usize) {
        if let Some(i) = self.verbatim_start_token_index {
            self.tokens[i].kind = TokenType::Text;
        }
        if content_start < self.pos {
            self.emit(TokenType::Text, content_start, self.pos);
        }
        self.verbatim = false;
        self.verbatim_return_state = None;
        self.verbatim_start_token_index = None;
        self.state = State::RawText;
        self.return_state = State::Data;
    }
}

/// `@verbatim` plus the default `statamic` syntax plugin's `@antlers`.
fn is_verbatim_start(name: &str) -> bool {
    name == "verbatim" || name == "antlers"
}

fn is_verbatim_end(name: &str) -> bool {
    name == "endverbatim" || name == "endantlers"
}

fn utf8_len(first: u8) -> usize {
    match first {
        0..0x80 => 1,
        0xC0..0xE0 => 2,
        0xE0..0xF0 => 3,
        0xF0.. => 4,
        _ => 1,
    }
}

#[allow(dead_code, reason = "kept for parity with the upstream helpers")]
fn unused(_: bool) {
    let _ = is_digit;
}
