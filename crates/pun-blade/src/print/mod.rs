//! The Blade printer: prettier-plugin-blade's `print/*` on top of the ported doc printer.

mod directive;
mod embed;
mod oxc;
mod raw_content;
mod srcset;
mod tag;
mod text;
pub(crate) mod utils;

use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, HashSet};

use crate::ast::{Ast, Id, ROOT, is_echo_like};
use crate::doc::{
    Doc, GroupId, Ids, break_parent, concat, dedent_to_root, group, group_with, hardline, if_break_group, indent,
    indent_if_break, line, softline,
};
use crate::lexer::TokenType;
use crate::tree::{NodeKind, has_swallowed_tag_syntax, js_trim_end};
use utils::{has_blank_line_strict, has_line_break, replace_eol_literal};

pub(crate) struct Printer<'a> {
    pub(crate) ast: &'a Ast,
    pub(crate) ids: Ids,
    cache: RefCell<HashMap<Id, Doc>>,
    pub(crate) ignored: RefCell<HashMap<Id, HashSet<Id>>>,
    pub(crate) unpaired_index: OnceCell<HashMap<String, Vec<usize>>>,
    pub(crate) embeds: RefCell<HashMap<Id, Doc>>,
    pub(crate) inline_intent_delegated: bool,
}

/// What goes between two printed siblings (`printBetweenLine`), kept symbolic because the
/// callers compare it against `hardline`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Between {
    Empty,
    Hardline,
    DoubleHardline,
    Line,
    Softline,
    Text(String),
}

impl Between {
    pub(crate) fn is_empty(&self) -> bool {
        matches!(self, Between::Empty)
    }

    pub(crate) fn doc(&self) -> Doc {
        match self {
            Between::Empty => Doc::default(),
            Between::Hardline => hardline(),
            Between::DoubleHardline => concat(vec![hardline(), hardline()]),
            Between::Line => line(),
            Between::Softline => softline(),
            Between::Text(s) => Doc::from(s.as_str()),
        }
    }
}

impl<'a> Printer<'a> {
    pub(crate) fn new(ast: &'a Ast) -> Self {
        Printer {
            ast,
            ids: Ids::default(),
            cache: RefCell::default(),
            ignored: RefCell::default(),
            unpaired_index: OnceCell::new(),
            embeds: RefCell::default(),
            inline_intent_delegated: false,
        }
    }

    /// `mainPrint`: prints a node once and hands out the same doc afterwards.
    pub(crate) fn print(&self, id: Id) -> Doc {
        if let Some(d) = self.cache.borrow().get(&id) {
            return d.clone();
        }
        let embedded = self.embeds.borrow().get(&id).cloned();
        let d = match embedded {
            Some(d) => d,
            None => self.generic_print(id),
        };
        self.cache.borrow_mut().insert(id, d.clone());
        d
    }

    pub(crate) fn print_root(&self) -> Doc {
        self.compute_embeds(ROOT);
        self.print(ROOT)
    }

    fn generic_print(&self, id: Id) -> Doc {
        let n = self.n(id);
        match n.kind {
            NodeKind::FrontMatter => Doc::from(self.full_text(id)),
            NodeKind::Root => {
                let children = self.print_children(id);
                if children.is_empty() {
                    return Doc::default();
                }
                let last_is_eof_ignore = n
                    .children
                    .last()
                    .is_some_and(|&c| self.kind(c) == NodeKind::IgnoreRange && self.n(c).end == self.source().len());
                if last_is_eof_ignore {
                    group(concat(children))
                } else {
                    concat(vec![group(concat(children)), hardline()])
                }
            }
            NodeKind::Element | NodeKind::ConditionalComment => self.print_element(id),
            NodeKind::Text => self.print_text(id),
            NodeKind::IgnoreRange => Doc::from(self.full_text(id)),
            NodeKind::Echo | NodeKind::RawEcho | NodeKind::TripleEcho => {
                concat(vec![self.opening_tag_prefix(id), self.print_echo(id), self.closing_tag_suffix(id)])
            }
            NodeKind::Directive => Doc::from(self.render_directive_tokens(id)),
            NodeKind::DirectiveBlock => self.print_directive_block(id),
            NodeKind::Comment | NodeKind::BogusComment => self.print_comment(id),
            NodeKind::BladeComment => Doc::from(self.full_text(id)),
            NodeKind::Attribute | NodeKind::JsxAttribute => self.print_attribute(id),
            NodeKind::Doctype => concat(vec![
                self.opening_tag_prefix(id),
                Doc::from(tag::format_doctype(&n.raw_text)),
                self.closing_tag_suffix(id),
            ]),
            NodeKind::Verbatim | NodeKind::PhpBlock | NodeKind::PhpTag => Doc::from(self.trim_unterminated_at_eof(id)),
            NodeKind::Cdata | NodeKind::Decl | NodeKind::ProcessingInstruction => concat(vec![
                self.opening_tag_prefix(id),
                Doc::from(self.trim_unterminated_at_eof(id)),
                self.closing_tag_suffix(id),
            ]),
            NodeKind::Fragment => concat(self.print_children(id)),
            NodeKind::NonOutput | NodeKind::ElementName | NodeKind::AttributeWhitespace | NodeKind::AttributeName | NodeKind::AttributeValue => {
                Doc::from(self.full_text(id))
            }
            NodeKind::UnpairedClosingTag | NodeKind::ClosingElementName => {
                let text = self.full_text(id);
                if n.end == self.source().len() { Doc::from(js_trim_end(text)) } else { Doc::from(text) }
            }
        }
    }

    fn trim_unterminated_at_eof(&self, id: Id) -> String {
        let text = self.full_text(id);
        let n = self.n(id);
        if n.end != self.source().len() {
            return text.to_owned();
        }
        let closing = match n.kind {
            NodeKind::Verbatim => TokenType::VerbatimEnd,
            NodeKind::PhpBlock => TokenType::PhpBlockEnd,
            NodeKind::PhpTag => TokenType::PhpTagEnd,
            NodeKind::Cdata => TokenType::CdataEnd,
            NodeKind::Decl => TokenType::DeclEnd,
            NodeKind::ProcessingInstruction => TokenType::PIEnd,
            _ => return text.to_owned(),
        };
        if self.tokens_of(id).iter().any(|t| t.kind == closing) {
            return text.to_owned();
        }
        js_trim_end(text).to_owned()
    }

    fn ancestor_count(&self, id: Id) -> usize {
        let mut d = 0;
        let mut cur = self.n(id).parent;
        while let Some(p) = cur {
            d += 1;
            cur = self.n(p).parent;
        }
        d
    }

    // -- children.ts --------------------------------------------------------------------------

    /// `printChild`
    fn print_child(&self, id: Id) -> Doc {
        if self.has_prettier_ignore(id) {
            return self.print_ignored_child(id);
        }
        self.print(id)
    }

    pub(crate) fn print_ignored_child(&self, id: Id) -> Doc {
        let n = self.n(id);
        let end = self.printable_subtree_end(id);
        let start = n.start
            + if n.prev.is_some_and(|p| self.needs_to_borrow_next_opening_tag_start_marker(p)) {
                self.opening_tag_start_marker(id).len()
            } else {
                0
            };
        let end = end
            - if n.next.is_some_and(|x| self.needs_to_borrow_prev_closing_tag_end_marker(x)) {
                self.closing_tag_end_marker(id).len()
            } else {
                0
            };
        let mut preserved = crate::ast::html_trim_end(&self.source()[start..end.max(start)]);
        if n.kind == NodeKind::Text {
            preserved = crate::ast::html_trim_start(preserved);
        }
        concat(vec![self.opening_tag_prefix(id), replace_eol_literal(preserved), self.closing_tag_suffix(id)])
    }

    fn source_between_segments(&self, prev: Id, next: Id) -> &str {
        let s = self.printable_subtree_end(prev);
        let e = self.n(next).start;
        if s <= e { &self.source()[s..e] } else { "" }
    }

    fn has_empty_line_between_segments(&self, prev: Id, next: Id) -> bool {
        if self.kind(prev) != NodeKind::IgnoreRange && self.kind(next) != NodeKind::IgnoreRange {
            return self.force_next_empty_line(prev);
        }
        has_blank_line_strict(self.source_between_segments(prev, next))
    }

    fn print_between_segments(&self, prev: Id, next: Id) -> Between {
        if self.kind(prev) == NodeKind::IgnoreRange || self.kind(next) == NodeKind::IgnoreRange {
            let between = self.source_between_segments(prev, next);
            if between.is_empty() {
                return Between::Empty;
            }
            if has_blank_line_strict(between) {
                return Between::DoubleHardline;
            }
            if has_line_break(between) {
                return Between::Hardline;
            }
            return Between::Text(between.to_owned());
        }
        self.print_between_line(prev, next)
    }

    fn print_between_line(&self, prev: Id, next: Id) -> Between {
        let p = self.n(prev);
        let x = self.n(next);
        if p.kind == NodeKind::NonOutput || x.kind == NodeKind::NonOutput {
            return Between::Empty;
        }
        if self.is_text_like(prev) && self.is_text_like(next) {
            if p.is_trailing_space_sensitive {
                if p.has_trailing_spaces {
                    if is_echo_like(p.kind) || is_echo_like(x.kind) {
                        return Between::Text(" ".to_owned());
                    }
                    return if self.prefer_hardline_as_leading_spaces(next) { Between::Hardline } else { Between::Line };
                }
                return Between::Empty;
            }
            if is_echo_like(p.kind) || is_echo_like(x.kind) {
                return Between::Empty;
            }
            return if self.prefer_hardline_as_leading_spaces(next) { Between::Hardline } else { Between::Softline };
        }
        if (self.needs_to_borrow_next_opening_tag_start_marker(prev)
            && (self.has_prettier_ignore(next)
                || !x.children.is_empty()
                || x.is_self_closing
                || (x.kind == NodeKind::Element && !x.attrs.is_empty())))
            || (p.kind == NodeKind::Element && p.is_self_closing && self.needs_to_borrow_prev_closing_tag_end_marker(next))
        {
            return Between::Empty;
        }
        let nested_borrow = self.needs_to_borrow_prev_closing_tag_end_marker(next)
            && self.last_child(prev).is_some_and(|lc| {
                self.needs_to_borrow_parent_closing_tag_start_marker(lc)
                    && self.last_child(lc).is_some_and(|llc| self.needs_to_borrow_parent_closing_tag_start_marker(llc))
            });
        if !x.is_leading_space_sensitive || self.prefer_hardline_as_leading_spaces(next) || nested_borrow {
            return Between::Hardline;
        }
        if x.has_leading_spaces {
            return Between::Line;
        }
        Between::Softline
    }

    pub(crate) fn print_children(&self, id: Id) -> Vec<Doc> {
        let children = self.n(id).children.clone();
        let printed: Vec<Doc> = children.iter().map(|&c| self.print_child(c)).collect();

        if self.force_break_children(id) {
            let mut out = vec![break_parent()];
            for (i, &c) in children.iter().enumerate() {
                let between = if i > 0 { self.print_between_segments(children[i - 1], c) } else { Between::Empty };
                let lead = if between.is_empty() {
                    Doc::default()
                } else {
                    concat(vec![
                        between.doc(),
                        if self.has_empty_line_between_segments(children[i - 1], c) { hardline() } else { Doc::default() },
                    ])
                };
                out.push(concat(vec![lead, printed[i].clone()]));
            }
            return out;
        }

        let needs_ids = children.iter().any(|&c| !self.is_text_like(c));
        let ids: Vec<Option<GroupId>> = children.iter().map(|_| needs_ids.then(|| self.ids.next())).collect();

        let mut out = Vec::with_capacity(children.len());
        for (i, &c) in children.iter().enumerate() {
            let seg = printed[i].clone();
            let prev = (i > 0).then(|| children[i - 1]);
            if self.is_text_like(c) {
                if let Some(p) = prev
                    && self.is_text_like(p)
                {
                    let between = self.print_between_segments(p, c);
                    if !between.is_empty() {
                        if self.has_empty_line_between_segments(p, c) {
                            out.push(concat(vec![hardline(), hardline(), seg]));
                        } else {
                            out.push(concat(vec![between.doc(), seg]));
                        }
                        continue;
                    }
                }
                out.push(seg);
                continue;
            }

            let mut prev_parts = Vec::new();
            let mut leading = Vec::new();
            let mut trailing = Vec::new();
            let mut next_parts = Vec::new();
            let next = children.get(i + 1).copied();
            let prev_between = prev.map_or(Between::Empty, |p| self.print_between_segments(p, c));
            let next_between = next.map_or(Between::Empty, |x| self.print_between_segments(c, x));

            if let Some(p) = prev
                && !prev_between.is_empty()
            {
                if self.has_empty_line_between_segments(p, c) {
                    prev_parts.push(hardline());
                    prev_parts.push(hardline());
                } else if prev_between == Between::Hardline {
                    prev_parts.push(hardline());
                } else if self.is_text_like(p) {
                    leading.push(prev_between.doc());
                } else {
                    leading.push(if_break_group(Doc::default(), softline(), ids[i - 1]));
                }
            }
            if let Some(x) = next
                && !next_between.is_empty()
            {
                if self.has_empty_line_between_segments(c, x) {
                    if self.is_text_like(x) {
                        next_parts.push(hardline());
                        next_parts.push(hardline());
                    }
                } else if next_between == Between::Hardline {
                    if self.is_text_like(x) {
                        next_parts.push(hardline());
                    }
                } else {
                    trailing.push(next_between.doc());
                }
            }
            let mut inner = vec![seg];
            inner.extend(trailing);
            leading.push(group_with(concat(inner), false, ids[i]));
            let mut parts = prev_parts;
            parts.push(group(concat(leading)));
            parts.extend(next_parts);
            out.push(concat(parts));
        }
        out
    }

    // -- element.ts ---------------------------------------------------------------------------

    fn print_element(&self, id: Id) -> Doc {
        let n = self.n(id);
        if self.should_preserve_malformed_opening_tag(id) {
            let end = self.max_descendant_end(id);
            return Doc::from(js_trim_end(&self.source()[n.start..end]));
        }
        let inline_intent = self.should_keep_inline_intent_layout(id);
        let multiline_svg = self.should_force_multiline_svg_children(id);
        let head_body_at_root = self.should_keep_html_head_body_at_root(id);

        if self.should_preserve_content(id) {
            let mut parts = vec![
                self.opening_tag_prefix(id),
                group(self.print_opening_tag(id)),
                replace_eol_literal(&self.node_content(id)),
            ];
            parts.extend(self.print_closing_tag(id));
            parts.push(self.closing_tag_suffix(id));
            return concat(parts);
        }

        let hug = n.children.len() == 1 && {
            let c = self.n(n.children[0]);
            is_echo_like(c.kind)
                && c.is_leading_space_sensitive
                && !c.has_leading_spaces
                && c.is_trailing_space_sensitive
                && !c.has_trailing_spaces
        };
        let attr_group_id = self.ids.next();
        let print_tag = |content: Doc| -> Doc {
            let mut parts = vec![group_with(self.print_opening_tag(id), false, Some(attr_group_id)), content];
            parts.push(concat(self.print_closing_tag(id)));
            group(concat(parts))
        };

        if n.children.is_empty() {
            let dangling = n.has_dangling_spaces && n.is_dangling_space_sensitive && !self.should_suppress_dangling_space_at_eof(id);
            return print_tag(if dangling { line() } else { Doc::default() });
        }

        let first = self.n(n.children[0]);
        let line_before = if inline_intent {
            Doc::default()
        } else if multiline_svg {
            hardline()
        } else if hug {
            if_break_group(softline(), Doc::default(), Some(attr_group_id))
        } else if first.has_leading_spaces && first.is_leading_space_sensitive {
            line()
        } else if first.kind == NodeKind::Text && n.is_whitespace_sensitive && n.is_indentation_sensitive {
            dedent_to_root(softline())
        } else {
            softline()
        };

        let line_after = self.line_after_children(id, inline_intent, multiline_svg, hug, attr_group_id);

        let children_doc = concat(vec![line_before, concat(self.print_children(id))]);
        let children_doc = if inline_intent {
            flatten_inline_intent_children(&children_doc)
        } else if head_body_at_root {
            children_doc
        } else if hug {
            indent_if_break(children_doc, Some(attr_group_id), false)
        } else {
            indent(children_doc)
        };

        print_tag(concat(vec![
            if self.force_break_content(id) { break_parent() } else { Doc::default() },
            children_doc,
            line_after,
        ]))
    }

    fn line_after_children(&self, id: Id, inline_intent: bool, multiline_svg: bool, hug: bool, attr_group_id: GroupId) -> Doc {
        let n = self.n(id);
        let last_id = n.children[n.children.len() - 1];
        let last = self.n(last_id);
        if inline_intent {
            return Doc::default();
        }
        if multiline_svg {
            return hardline();
        }
        if !n.has_closing_tag && !n.is_self_closing {
            return Doc::default();
        }
        if self.is_recovery_unclosed_element_with_closing_text(id) {
            return Doc::default();
        }
        let borrow = match n.next {
            Some(x) => self.needs_to_borrow_prev_closing_tag_end_marker(x),
            None => n.parent.is_some_and(|p| self.needs_to_borrow_last_child_closing_tag_end_marker(p)),
        };
        if borrow {
            return Doc::from(if last.has_trailing_spaces && last.is_trailing_space_sensitive { " " } else { "" });
        }
        if hug {
            return if_break_group(softline(), Doc::default(), Some(attr_group_id));
        }
        if last.has_trailing_spaces && last.is_trailing_space_sensitive {
            return line();
        }
        let value = if last.kind == NodeKind::Comment {
            last.raw_text.strip_prefix("<!--").and_then(|r| r.strip_suffix("-->")).unwrap_or(&last.raw_text)
        } else {
            &last.raw_text
        };
        if (last.kind == NodeKind::Comment
            || last.kind == NodeKind::BogusComment
            || (last.kind == NodeKind::Text && n.is_whitespace_sensitive && n.is_indentation_sensitive))
            && ends_with_indented_newline(value, utils::TAB_WIDTH * self.ancestor_count(id).saturating_sub(1))
        {
            return Doc::default();
        }
        softline()
    }

    fn should_preserve_malformed_opening_tag(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind != NodeKind::Element || n.has_closing_tag || n.is_self_closing {
            return false;
        }
        if n.open_tag_end_offset > n.start && !has_swallowed_tag_syntax(&self.source()[n.start + 1..n.open_tag_end_offset]) {
            return false;
        }
        if n.attrs.is_empty() && n.children.len() == 1 {
            let c = self.n(n.children[0]);
            if is_echo_like(c.kind) && c.start >= n.start && c.end == n.end {
                return false;
            }
        }
        true
    }

    fn max_descendant_end(&self, id: Id) -> usize {
        let n = self.n(id);
        n.children.iter().fold(n.end, |e, &c| e.max(self.max_descendant_end(c)))
    }

    fn is_recovery_unclosed_element_with_closing_text(&self, id: Id) -> bool {
        let n = self.n(id);
        n.kind == NodeKind::Element
            && !n.has_closing_tag
            && !n.is_self_closing
            && !n.raw_tag_name.is_empty()
            && n.open_tag_end_offset > 0
            && self.source()[n.open_tag_end_offset.min(n.end)..n.end]
                .to_lowercase()
                .contains(&format!("</{}", n.raw_tag_name.to_lowercase()))
    }

    fn should_keep_inline_intent_layout(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind != NodeKind::Element
            || n.children.is_empty()
            || !n.has_closing_tag
            || n.open_tag_end_offset <= n.start
            || n.closing_tag_start_offset <= n.open_tag_end_offset
        {
            return false;
        }
        if !utils::preserve_inline_intent_element(&n.tag_name, &n.namespace) {
            return false;
        }
        if self.contains_block_directive_descendant(id) {
            return false;
        }
        if n.tag_name == "svg" && n.children.iter().filter(|&&c| self.kind(c) == NodeKind::Element).count() > 1 {
            return false;
        }
        let src = &self.source()[n.start..n.end];
        if !has_line_break(src) {
            return true;
        }
        let first = self.n(n.children[0]);
        let last = self.n(n.children[n.children.len() - 1]);
        if has_line_break(&self.source()[n.open_tag_end_offset..first.start.max(n.open_tag_end_offset)]) {
            return false;
        }
        !has_line_break(&self.source()[last.end.min(n.closing_tag_start_offset)..n.closing_tag_start_offset])
    }

    fn should_suppress_dangling_space_at_eof(&self, id: Id) -> bool {
        let n = self.n(id);
        n.kind == NodeKind::Element && n.end == self.source().len() && !n.has_closing_tag && !n.is_self_closing
    }

    fn should_force_multiline_svg_children(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind != NodeKind::Element {
            return false;
        }
        let element_children = n.children.iter().filter(|&&c| self.kind(c) == NodeKind::Element).count();
        if n.tag_name == "clippath" {
            return element_children > 0;
        }
        n.tag_name == "svg" && element_children > 1
    }

    fn contains_block_directive_descendant(&self, id: Id) -> bool {
        let mut stack: Vec<Id> = self.n(id).children.clone();
        while let Some(c) = stack.pop() {
            let n = self.n(c);
            if n.kind == NodeKind::DirectiveBlock || (n.kind == NodeKind::Directive && !n.children.is_empty()) {
                return true;
            }
            stack.extend(n.children.iter().copied());
        }
        false
    }

    fn should_keep_html_head_body_at_root(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind != NodeKind::Element || !n.namespace.is_empty() || n.tag_name != "html" {
            return false;
        }
        if n.parent.is_none_or(|p| self.kind(p) != NodeKind::Root) {
            return false;
        }
        let elements: Vec<Id> = n.children.iter().copied().filter(|&c| self.kind(c) == NodeKind::Element).collect();
        !elements.is_empty()
            && elements.iter().all(|&c| {
                let cn = self.n(c);
                cn.namespace.is_empty() && (cn.tag_name == "head" || cn.tag_name == "body")
            })
    }

    /// `getNodeContent`
    fn node_content(&self, id: Id) -> String {
        let n = self.n(id);
        if !n.has_closing_tag || (n.open_tag_end_offset == 0 && n.closing_tag_start_offset == 0) {
            return String::new();
        }
        let mut start = n.open_tag_end_offset;
        if self.first_child(id).is_some_and(|c| self.needs_to_borrow_parent_opening_tag_end_marker(c)) {
            start -= self.opening_tag_end_marker(id).len();
        }
        let mut end = n.closing_tag_start_offset;
        let last = self.last_child(id);
        if last.is_some_and(|l| self.needs_to_borrow_parent_closing_tag_start_marker(l)) {
            end += self.closing_tag_start_marker(id).len();
        } else if self.needs_to_borrow_last_child_closing_tag_end_marker(id)
            && let Some(l) = last
        {
            end -= self.closing_tag_end_marker(l).len();
        }
        self.source()[start..end.max(start)].to_owned()
    }
}

fn ends_with_indented_newline(value: &str, indent_width: usize) -> bool {
    let b = value.as_bytes();
    let mut i = b.len();
    let mut ws = 0;
    while i > 0 && (b[i - 1] == b' ' || b[i - 1] == b'\t') {
        ws += 1;
        i -= 1;
    }
    ws == indent_width && i > 0 && b[i - 1] == b'\n'
}

/// `flattenInlineIntentChildren`: soft lines vanish, lines become spaces, break parents drop.
fn flatten_inline_intent_children(doc: &Doc) -> Doc {
    crate::doc::map_doc(doc, &mut |d| match &d {
        Doc::Line(l) if !l.hard && l.soft => Doc::default(),
        Doc::Line(l) if !l.hard => Doc::from(" "),
        Doc::BreakParent => Doc::default(),
        _ => d,
    })
}

/// Formats a Blade template the way Prettier with prettier-plugin-blade does under Pint's options.
pub(crate) fn format(text: &str) -> String {
    format_with(text, utils::PRINT_WIDTH, false)
}

/// `inline_intent_delegated` is the plugin's `__bladeInlineIntentDelegated`: an inline-intent
/// element formatted on its own, wider.
pub(crate) fn format_with(text: &str, print_width: usize, inline_intent_delegated: bool) -> String {
    let text = utils::normalize_line_endings_to_lf(text);
    let mut ast = crate::ast::parse(&text);
    ast.preprocess();
    let mut printer = Printer::new(&ast);
    printer.inline_intent_delegated = inline_intent_delegated;
    let doc = printer.print_root();
    crate::doc::print_doc_to_string(&doc, &crate::doc::PrintOptions { print_width, tab_width: utils::TAB_WIDTH })
}
