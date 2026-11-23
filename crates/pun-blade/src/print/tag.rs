//! `print/tag.ts`, `attribute-name.ts`, `bound-attribute.ts` and `doctype-utils.ts`.

use std::collections::HashMap;

use super::Printer;
use super::utils::{
    NamePart, ValueKind, directive_arg_spacing, is_blade_component_tag_name, is_html_tag, normalize_line_endings_to_lf,
    preserve_inline_intent_attributes, preserve_inline_intent_element, preserve_inline_intent_namespace, replace_eol,
    replace_eol_literal, unescape_quote_entities,
};
use crate::ast::Id;
use crate::doc::{Doc, concat, hardline, indent, join, line, softline};
use crate::lexer::TokenType;
use crate::tree::{NodeKind, VOID_ELEMENTS, has_swallowed_tag_syntax, js_trim, js_trim_end};

impl Printer<'_> {
    fn can_borrow_parent_tag_markers(&self, id: Id) -> bool {
        (self.kind(id) == NodeKind::Element && !self.is_malformed_opening_recovery_element(id))
            || self.kind(id) == NodeKind::ConditionalComment
    }

    pub(crate) fn is_malformed_opening_recovery_element(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind != NodeKind::Element || n.has_closing_tag || n.is_self_closing {
            return false;
        }
        if n.open_tag_end_offset <= n.start {
            return true;
        }
        has_swallowed_tag_syntax(&self.source()[n.start + 1..n.open_tag_end_offset])
    }

    fn should_trim_malformed_eof_tag_name(&self, id: Id) -> bool {
        let n = self.n(id);
        n.kind == NodeKind::Element && n.end == self.source().len() && n.open_tag_end_offset <= n.start && !n.has_closing_tag
    }

    fn display_tag_name(&self, id: Id) -> String {
        let n = self.n(id);
        let trim = |s: &str| -> String {
            if self.should_trim_malformed_eof_tag_name(id) { js_trim_end(s).to_owned() } else { s.to_owned() }
        };
        if n.namespace == "svg" {
            return trim(&n.raw_tag_name);
        }
        if n.kind == NodeKind::Element && self.ast.flat[n.flat].generic_offset > 0 {
            return trim(&n.raw_tag_name);
        }
        let lower = n.raw_tag_name.to_lowercase();
        if is_html_tag(&lower) {
            return trim(&lower);
        }
        trim(&n.raw_tag_name)
    }

    fn element_generic_suffix(&self, id: Id) -> String {
        if self.kind(id) != NodeKind::Element {
            return String::new();
        }
        for t in self.tokens_of(id).iter().skip(1) {
            match t.kind {
                TokenType::TsxGenericType => return self.tok_text(t).to_owned(),
                TokenType::Whitespace | TokenType::GreaterThan | TokenType::Slash | TokenType::SyntheticClose => break,
                _ => {}
            }
        }
        String::new()
    }

    fn should_print_self_closing_slash(&self, id: Id) -> bool {
        let n = self.n(id);
        n.kind == NodeKind::Element && n.is_self_closing
    }

    fn display_closing_tag_name(&self, id: Id) -> String {
        let n = self.n(id);
        let dynamic = |s: &str| s.contains("{{") || s.contains("{!!") || s.contains("<?") || s.contains('@');
        if !n.raw_closing_tag_name.is_empty() && (dynamic(&n.raw_tag_name) || dynamic(&n.raw_closing_tag_name)) {
            return n.raw_closing_tag_name.clone();
        }
        self.display_tag_name(id)
    }

    fn is_borrowable_marker_node(&self, id: Id) -> bool {
        if self.is_malformed_opening_recovery_element(id) {
            return false;
        }
        matches!(self.kind(id), NodeKind::Element | NodeKind::ConditionalComment | NodeKind::Comment | NodeKind::BogusComment)
    }

    fn can_borrow_prev_closing_tag_end_marker(&self, id: Id) -> bool {
        matches!(
            self.kind(id),
            NodeKind::Element
                | NodeKind::ConditionalComment
                | NodeKind::Text
                | NodeKind::Comment
                | NodeKind::BogusComment
                | NodeKind::Echo
                | NodeKind::RawEcho
                | NodeKind::TripleEcho
                | NodeKind::Doctype
                | NodeKind::Cdata
                | NodeKind::Decl
                | NodeKind::ProcessingInstruction
        )
    }

    fn unpaired_closing_tag_index(&self) -> &HashMap<String, Vec<usize>> {
        self.unpaired_index.get_or_init(|| {
            let mut index: HashMap<String, Vec<usize>> = HashMap::new();
            let mut stack = vec![crate::ast::ROOT];
            while let Some(id) = stack.pop() {
                if let Some(name) = self.unpaired_closing_tag_name(id) {
                    index.entry(name).or_default().push(self.n(id).start);
                }
                for &c in self.n(id).children.iter().rev() {
                    stack.push(c);
                }
            }
            for v in index.values_mut() {
                v.sort_unstable();
            }
            index
        })
    }

    /// `^<\s*\/\s*([^\s>/]+)\s*>?$`
    fn unpaired_closing_tag_name(&self, id: Id) -> Option<String> {
        if self.kind(id) != NodeKind::UnpairedClosingTag {
            return None;
        }
        let raw = js_trim(self.full_text(id));
        let rest = crate::tree::js_trim_start(raw.strip_prefix('<')?);
        let rest = crate::tree::js_trim_start(rest.strip_prefix('/')?);
        let name_len = rest.find(|c: char| crate::tree::is_js_whitespace(c) || c == '>' || c == '/').unwrap_or(rest.len());
        if name_len == 0 {
            return None;
        }
        let name = &rest[..name_len];
        let tail = crate::tree::js_trim_start(&rest[name_len..]);
        (tail.is_empty() || tail == ">").then(|| name.to_lowercase())
    }

    fn has_future_matching_unpaired_closing_tag(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind != NodeKind::Element || n.tag_name.is_empty() {
            return false;
        }
        let index = self.unpaired_closing_tag_index();
        let open = n.tag_name.to_lowercase();
        let mut candidates = vec![open.clone()];
        if open.starts_with("x-slot:") || open.starts_with("x-slot[") {
            candidates.push("x-slot".to_owned());
        }
        candidates.iter().any(|c| index.get(c).is_some_and(|p| p.last().is_some_and(|&last| last > n.end)))
    }

    pub(crate) fn needs_to_borrow_prev_closing_tag_end_marker(&self, id: Id) -> bool {
        let n = self.n(id);
        self.can_borrow_prev_closing_tag_end_marker(id)
            && n.prev.is_some_and(|p| {
                self.is_borrowable_marker_node(p) && self.kind(p) != NodeKind::Doctype && !self.is_text_like(p)
            })
            && n.kind != NodeKind::DirectiveBlock
            && n.is_leading_space_sensitive
            && !n.has_leading_spaces
    }

    pub(crate) fn needs_to_borrow_last_child_closing_tag_end_marker(&self, id: Id) -> bool {
        if !self.can_borrow_parent_tag_markers(id) {
            return false;
        }
        let Some(last) = self.last_child(id) else { return false };
        let l = self.n(last);
        self.is_borrowable_marker_node(last)
            && l.is_trailing_space_sensitive
            && !l.has_trailing_spaces
            && !self.is_text_like(self.last_descendant(last))
            && !self.is_pre_like(id)
    }

    pub(crate) fn needs_to_borrow_parent_closing_tag_start_marker(&self, id: Id) -> bool {
        let n = self.n(id);
        !self.is_malformed_opening_recovery_element(id)
            && n.next.is_none()
            && !n.has_trailing_spaces
            && n.is_trailing_space_sensitive
            && self.is_text_like(self.last_descendant(id))
    }

    pub(crate) fn needs_to_borrow_next_opening_tag_start_marker(&self, id: Id) -> bool {
        let n = self.n(id);
        n.next.is_some_and(|x| self.is_borrowable_marker_node(x) && !self.is_text_like(x))
            && self.is_text_like(id)
            && n.is_trailing_space_sensitive
            && !n.has_trailing_spaces
    }

    pub(crate) fn needs_to_borrow_parent_opening_tag_end_marker(&self, id: Id) -> bool {
        let n = self.n(id);
        let can_borrow = matches!(
            n.kind,
            NodeKind::Text
                | NodeKind::Comment
                | NodeKind::BogusComment
                | NodeKind::Echo
                | NodeKind::RawEcho
                | NodeKind::TripleEcho
                | NodeKind::Doctype
                | NodeKind::Cdata
                | NodeKind::Decl
                | NodeKind::ProcessingInstruction
                | NodeKind::Element
                | NodeKind::ConditionalComment
        );
        if let Some(p) = n.parent
            && self.kind(p) == NodeKind::Element
            && self.n(p).raw_tag_name.contains('.')
        {
            return false;
        }
        can_borrow && n.prev.is_none() && n.is_leading_space_sensitive && !n.has_leading_spaces
    }

    fn has_closing_tag_like_text_in_body(&self, id: Id) -> bool {
        let n = self.n(id);
        n.kind == NodeKind::Element
            && !n.is_self_closing
            && !n.has_closing_tag
            && !n.raw_tag_name.is_empty()
            && n.open_tag_end_offset > 0
            && self.source()[n.open_tag_end_offset.min(n.end)..n.end]
                .to_lowercase()
                .contains(&format!("</{}", n.raw_tag_name.to_lowercase()))
    }

    fn should_not_print_closing_tag(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind == NodeKind::Element && !n.is_self_closing && !n.has_closing_tag {
            // `bladeInsertOptionalClosingTags` is off.
            return true;
        }
        let has_unpaired_child =
            n.kind == NodeKind::Element && n.children.iter().any(|&c| self.kind(c) == NodeKind::UnpairedClosingTag);
        !n.is_self_closing
            && !n.has_closing_tag
            && (self.has_closing_tag_like_text_in_body(id)
                || self.has_prettier_ignore(id)
                || n.parent.is_some_and(|p| self.should_preserve_content(p))
                || has_unpaired_child)
    }

    pub(crate) fn opening_tag_start_marker(&self, id: Id) -> String {
        let n = self.n(id);
        match n.kind {
            NodeKind::ConditionalComment => format!("<!--[if {}", n.condition),
            NodeKind::Comment => {
                if n.is_ie_conditional_start_comment {
                    format!("<!--[if {}", n.ie_conditional_start_condition)
                } else {
                    "<!--".to_owned()
                }
            }
            NodeKind::BogusComment => "<!--".to_owned(),
            NodeKind::Echo => "{{".to_owned(),
            NodeKind::RawEcho => "{!!".to_owned(),
            NodeKind::TripleEcho => "{{{".to_owned(),
            NodeKind::Doctype => doctype_start_marker(&n.raw_text),
            NodeKind::Element => {
                if n.condition.is_empty() {
                    format!("<{}{}", self.display_tag_name(id), self.element_generic_suffix(id))
                } else {
                    format!("<!--[if {}]><!--><{}{}", n.condition, self.display_tag_name(id), self.element_generic_suffix(id))
                }
            }
            _ => format!("<{}", self.display_tag_name(id)),
        }
    }

    pub(crate) fn opening_tag_end_marker(&self, id: Id) -> String {
        let n = self.n(id);
        match n.kind {
            NodeKind::ConditionalComment => {
                if n.conditional_start_is_revealed { "]><!-->" } else { "]>" }.to_owned()
            }
            NodeKind::Element => {
                let future = self.has_future_matching_unpaired_closing_tag(id);
                if n.open_tag_end_offset <= n.start && !n.has_closing_tag && !future {
                    return String::new();
                }
                if !n.condition.is_empty() {
                    return "><!--<![endif]-->".to_owned();
                }
                ">".to_owned()
            }
            _ => ">".to_owned(),
        }
    }

    pub(crate) fn closing_tag_start_marker(&self, id: Id) -> String {
        if self.should_not_print_closing_tag(id) {
            return String::new();
        }
        let n = self.n(id);
        match n.kind {
            NodeKind::ConditionalComment => if n.conditional_end_is_hidden { "<!--<!" } else { "<!" }.to_owned(),
            NodeKind::Element => {
                if n.has_htm_component_closing_tag {
                    "<//".to_owned()
                } else {
                    format!("</{}", self.display_closing_tag_name(id))
                }
            }
            _ => format!("</{}", self.display_tag_name(id)),
        }
    }

    pub(crate) fn closing_tag_end_marker(&self, id: Id) -> String {
        if self.should_not_print_closing_tag(id) {
            return String::new();
        }
        let n = self.n(id);
        match n.kind {
            NodeKind::ConditionalComment => "[endif]-->".to_owned(),
            NodeKind::Comment => if n.is_ie_conditional_start_comment { "]><!-->" } else { ">" }.to_owned(),
            NodeKind::Echo => "}}".to_owned(),
            NodeKind::RawEcho => "!!}".to_owned(),
            NodeKind::TripleEcho => "}}}".to_owned(),
            NodeKind::Element => if self.should_print_self_closing_slash(id) { "/>" } else { ">" }.to_owned(),
            _ => ">".to_owned(),
        }
    }

    pub(crate) fn opening_tag_prefix(&self, id: Id) -> Doc {
        let n = self.n(id);
        if self.needs_to_borrow_parent_opening_tag_end_marker(id) {
            return match n.parent {
                Some(p) if self.can_borrow_parent_tag_markers(p) => Doc::from(self.opening_tag_end_marker(p)),
                _ => Doc::default(),
            };
        }
        if self.needs_to_borrow_prev_closing_tag_end_marker(id)
            && let Some(p) = n.prev
        {
            return Doc::from(self.closing_tag_end_marker(p));
        }
        Doc::default()
    }

    pub(crate) fn opening_tag_start(&self, id: Id) -> Doc {
        if self.n(id).prev.is_some_and(|p| self.needs_to_borrow_next_opening_tag_start_marker(p)) {
            return Doc::default();
        }
        concat(vec![self.opening_tag_prefix(id), Doc::from(self.opening_tag_start_marker(id))])
    }

    fn opening_tag_end(&self, id: Id) -> Doc {
        if self.first_child(id).is_some_and(|c| self.needs_to_borrow_parent_opening_tag_end_marker(c)) {
            return Doc::default();
        }
        Doc::from(self.opening_tag_end_marker(id))
    }

    pub(crate) fn closing_tag_suffix(&self, id: Id) -> Doc {
        let n = self.n(id);
        if self.needs_to_borrow_parent_closing_tag_start_marker(id) {
            if let Some(p) = n.parent
                && self.can_borrow_parent_tag_markers(p)
                && (self.kind(p) != NodeKind::Element || self.n(p).has_closing_tag)
            {
                return Doc::from(self.closing_tag_start_marker(p));
            }
            return Doc::default();
        }
        if self.needs_to_borrow_next_opening_tag_start_marker(id)
            && let Some(x) = n.next
        {
            return Doc::from(self.opening_tag_start_marker(x));
        }
        Doc::default()
    }

    fn closing_tag_prefix(&self, id: Id) -> Doc {
        if self.needs_to_borrow_last_child_closing_tag_end_marker(id)
            && let Some(last) = self.last_child(id)
        {
            return Doc::from(self.closing_tag_end_marker(last));
        }
        Doc::default()
    }

    fn closing_tag_start(&self, id: Id) -> Doc {
        if self.last_child(id).is_some_and(|c| self.needs_to_borrow_parent_closing_tag_start_marker(c)) {
            return Doc::default();
        }
        concat(vec![self.closing_tag_prefix(id), Doc::from(self.closing_tag_start_marker(id))])
    }

    pub(crate) fn closing_tag_end(&self, id: Id) -> Doc {
        let n = self.n(id);
        let borrowed = match n.next {
            Some(x) => self.needs_to_borrow_prev_closing_tag_end_marker(x),
            None => n.parent.is_some_and(|p| self.needs_to_borrow_last_child_closing_tag_end_marker(p)),
        };
        if borrowed {
            return Doc::default();
        }
        concat(vec![Doc::from(self.closing_tag_end_marker(id)), self.closing_tag_suffix(id)])
    }

    pub(crate) fn print_opening_tag(&self, id: Id) -> Doc {
        let n = self.n(id);
        concat(vec![
            self.opening_tag_start(id),
            self.print_attributes(id),
            if n.is_self_closing { Doc::default() } else { self.opening_tag_end(id) },
        ])
    }

    pub(crate) fn print_closing_tag(&self, id: Id) -> Vec<Doc> {
        let n = self.n(id);
        vec![if n.is_self_closing { Doc::default() } else { self.closing_tag_start(id) }, self.closing_tag_end(id)]
    }

    fn print_attributes(&self, id: Id) -> Doc {
        let n = self.n(id);
        let slash = self.should_print_self_closing_slash(id);
        if n.attrs.is_empty() {
            return Doc::from(if slash { " " } else { "" });
        }
        let ignore_data = n
            .prev
            .filter(|&p| self.kind(p) == NodeKind::Comment)
            .and_then(|p| prettier_ignore_attribute_data(&self.n(p).raw_text));
        let printed: Vec<Doc> = n
            .attrs
            .iter()
            .map(|&a| {
                let ignored = match &ignore_data {
                    Some(IgnoreAttr::All) => true,
                    Some(IgnoreAttr::Names(names)) => {
                        matches!(self.kind(a), NodeKind::Attribute | NodeKind::JsxAttribute)
                            && names.contains(&self.attribute_name_raw(a))
                    }
                    None => false,
                };
                if ignored {
                    crate::doc::replace_end_of_line(&Doc::from(&self.source()[self.n(a).start..self.n(a).end]))
                } else {
                    self.print(a)
                }
            })
            .collect();

        let force_not_to_break = n.kind == NodeKind::Element
            && n.full_name.to_lowercase() == "script"
            && n.attrs.len() == 1
            && self.static_attribute_name_lower(n.attrs[0]).as_deref() == Some("src")
            && n.children.is_empty();
        let inline_intent = self.should_keep_inline_intent_attribute_layout(id);
        let multiline_svg = self.should_force_multiline_svg_attribute_layout(id);

        let first_sep = if force_not_to_break || inline_intent {
            Doc::from(" ")
        } else if multiline_svg {
            hardline()
        } else {
            line()
        };
        let sep = if inline_intent {
            Doc::from(" ")
        } else if multiline_svg {
            hardline()
        } else {
            line()
        };
        let mut parts = vec![indent(concat(vec![first_sep, join(&sep, printed)]))];
        let force_flat = self.first_child(id).is_some_and(|c| self.needs_to_borrow_parent_opening_tag_end_marker(c))
            || (n.is_self_closing && n.parent.is_some_and(|p| self.needs_to_borrow_last_child_closing_tag_end_marker(p)))
            || inline_intent
            || force_not_to_break;
        if multiline_svg {
            parts.push(hardline());
        } else if force_flat {
            parts.push(Doc::from(if slash { " " } else { "" }));
        } else {
            parts.push(if slash { line() } else { softline() });
        }
        concat(parts)
    }

    fn should_keep_inline_intent_attribute_layout(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind != NodeKind::Element || n.attrs.is_empty() || n.open_tag_end_offset <= n.start {
            return false;
        }
        if !preserve_inline_intent_attributes(&n.tag_name, &n.namespace) {
            return false;
        }
        let opening = &self.source()[n.start..n.open_tag_end_offset];
        if opening.contains("{{") || opening.contains("{!!") {
            return false;
        }
        !super::utils::has_line_break(opening)
    }

    fn should_force_multiline_svg_attribute_layout(&self, id: Id) -> bool {
        let n = self.n(id);
        n.kind == NodeKind::Element
            && n.namespace == "svg"
            && !n.attrs.is_empty()
            && !(preserve_inline_intent_element(&n.tag_name, &n.namespace) || preserve_inline_intent_namespace(&n.namespace))
    }

    // -- attribute values --------------------------------------------------------------------

    fn format_dynamic_attribute_value(&self, id: Id, original: &str) -> String {
        let Some((start, end)) = self.attribute_value_token_range(id) else { return original.to_owned() };
        if start >= end {
            return original.to_owned();
        }
        let tokens = self.tokens_of(id);
        let mut out = String::new();
        let mut i = start;
        while i < end {
            let t = tokens[i];
            let info = match t.kind {
                TokenType::EchoStart => Some(("{{", "}}", TokenType::EchoEnd)),
                TokenType::RawEchoStart => Some(("{!!", "!!}", TokenType::RawEchoEnd)),
                TokenType::TripleEchoStart => Some(("{{{", "}}}", TokenType::TripleEchoEnd)),
                _ => None,
            };
            let Some((open, close, end_type)) = info else {
                out.push_str(self.tok_text(&t));
                i += 1;
                continue;
            };
            let seg_start = t.start;
            let mut seg_end = t.end;
            i += 1;
            while i < end {
                seg_end = tokens[i].end;
                if tokens[i].kind == end_type {
                    break;
                }
                i += 1;
            }
            out.push_str(&format_echo_construct(&self.source()[seg_start..seg_end], open, close));
            i += 1;
        }
        out
    }

    pub(crate) fn print_attribute(&self, id: Id) -> Doc {
        let mut bound = false;
        let mut shorthand = false;
        let mut escaped = false;
        for t in self.tokens_of(id) {
            match t.kind {
                TokenType::BoundAttribute => bound = true,
                TokenType::ShorthandAttribute => shorthand = true,
                TokenType::EscapedAttribute => escaped = true,
                _ => {}
            }
        }
        let raw_name = self.format_attribute_name_for_print(id);
        let name = if bound || shorthand || escaped || !self.is_static_attribute_name(id) {
            raw_name
        } else {
            self.normalize_attribute_name(&raw_name, self.n(id).parent)
        };
        let Some(value) = self.attribute_raw_value(id) else { return Doc::from(name) };
        if self.attribute_value_kind(id) != ValueKind::StaticText {
            let formatted = self.format_dynamic_attribute_value(id, &value);
            return match self.attribute_value_quote(id) {
                Some(q) if !q.is_empty() => concat(vec![
                    Doc::from(name),
                    Doc::from("="),
                    Doc::from(q.clone()),
                    replace_eol_literal(&formatted),
                    Doc::from(q),
                ]),
                _ => concat(vec![Doc::from(name), Doc::from("="), replace_eol_literal(&formatted)]),
            };
        }
        let unescaped = unescape_quote_entities(&value);
        let quote = preferred_quote(&unescaped);
        let indent_array = bound && is_multiline_bracket_array_value(&unescaped);
        let escaped_value =
            if quote == '"' { unescaped.replace('"', "&quot;") } else { unescaped.replace('\'', "&apos;") };
        let rendered =
            if indent_array { print_multiline_bound_array_value(&escaped_value) } else { replace_eol_literal(&escaped_value) };
        concat(vec![Doc::from(name), Doc::from("="), Doc::from(quote.to_string()), rendered, Doc::from(quote.to_string())])
    }

    pub(crate) fn is_component_attribute_name(&self, id: Id) -> bool {
        self.n(id)
            .parent
            .is_some_and(|p| self.kind(p) == NodeKind::Element && is_blade_component_tag_name(&self.n(p).full_name))
    }

    pub(crate) fn format_attribute_name_for_print(&self, id: Id) -> String {
        if self.is_static_attribute_name(id) {
            return self.attribute_name_raw(id);
        }
        let parts = self.attribute_name_parts(id);
        if parts.is_empty() {
            return self.attribute_name_raw(id);
        }
        parts
            .iter()
            .map(|(kind, text)| match kind {
                NamePart::Echo => format_echo_construct(text, "{{", "}}"),
                NamePart::RawEcho => format_echo_construct(text, "{!!", "!!}"),
                NamePart::TripleEcho => format_echo_construct(text, "{{{", "}}}"),
                NamePart::Directive => self.format_directive_part(text, id),
                _ => text.clone(),
            })
            .collect()
    }

    fn format_directive_part(&self, text: &str, id: Id) -> String {
        if !text.starts_with('@') {
            return text.to_owned();
        }
        let Some(args_start) = text.find('(') else { return text.to_owned() };
        let before = &text[..args_start];
        let name = js_trim_end(before);
        let args = &text[args_start..];
        let spacing = directive_arg_spacing(name, self.is_component_attribute_name(id));
        format!("{name}{spacing}{}", crate::tree::js_trim_start(args))
    }
}

enum IgnoreAttr {
    All,
    Names(Vec<String>),
}

/// `getPrettierIgnoreAttributeCommentData` on a comment's raw text.
fn prettier_ignore_attribute_data(raw: &str) -> Option<IgnoreAttr> {
    // `.replace(/^<!--\s*/, "").replace(/\s*-->$/, "")`
    let mut v = raw;
    if let Some(rest) = v.strip_prefix("<!--") {
        v = crate::tree::js_trim_start(rest);
    }
    if let Some(rest) = v.strip_suffix("-->") {
        v = js_trim_end(rest);
    }
    let v = js_trim(v);
    let rest = v.strip_prefix("prettier-ignore-attribute")?;
    if rest.is_empty() {
        return Some(IgnoreAttr::All);
    }
    if !rest.starts_with(crate::tree::is_js_whitespace) {
        return None;
    }
    let names = crate::tree::js_trim_start(rest);
    if names.is_empty() {
        return Some(IgnoreAttr::All);
    }
    Some(IgnoreAttr::Names(names.split(crate::tree::is_js_whitespace).map(str::to_owned).collect()))
}

/// `formatEchoConstruct` with `bladeEchoSpacing: "space"`.
pub(crate) fn format_echo_construct(raw: &str, open: &str, close: &str) -> String {
    if !raw.starts_with(open) || !raw.ends_with(close) || raw.len() < open.len() + close.len() {
        return raw.to_owned();
    }
    let content = &raw[open.len()..raw.len() - close.len()];
    let trimmed = js_trim(content);
    if trimmed.is_empty() {
        return format!("{open} {close}");
    }
    if trimmed.contains('\n') {
        return format!("{open}\n{trimmed}\n{close}");
    }
    format!("{open} {trimmed} {close}")
}

fn preferred_quote(value: &str) -> char {
    let double = value.matches('"').count();
    let single = value.matches('\'').count();
    if double > single { '\'' } else { '"' }
}

pub(crate) fn print_multiline_bound_array_value(value: &str) -> Doc {
    let normalized = normalize_line_endings_to_lf(value);
    let lines: Vec<&str> = normalized.split('\n').collect();
    let body = super::utils::dedent_string(&lines[1..lines.len() - 1].join("\n"));
    concat(vec![
        Doc::from("["),
        indent(concat(vec![hardline(), concat(replace_eol(&body, &hardline()))])),
        hardline(),
        Doc::from("]"),
    ])
}

pub(crate) fn is_multiline_bracket_array_value(value: &str) -> bool {
    let normalized = normalize_line_endings_to_lf(value);
    let lines: Vec<&str> = normalized.split('\n').collect();
    lines.len() > 2
        && js_trim(lines[0]) == "["
        && lines.last().is_some_and(|l| js_trim(l) == "]")
        && !value.contains("<<<")
        && !has_multiline_quoted_value(value)
}

fn has_multiline_quoted_value(value: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for c in value.chars() {
        if c == '\n' || c == '\r' {
            if quote.is_some() {
                return true;
            }
            continue;
        }
        let Some(q) = quote else {
            if matches!(c, '\'' | '"' | '`') {
                quote = Some(c);
            }
            continue;
        };
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == q {
            quote = None;
        }
    }
    false
}

// doctype-utils.ts (the file path is a `.blade.php`, so the marker keeps its authored case)

fn doctype_start_marker(raw: &str) -> String {
    raw.chars().take(9).collect()
}

/// `^<!DOCTYPE\s+(.*?)\s*>$` (case-insensitive, dot-all): the normalized value.
fn parse_doctype(raw: &str) -> Option<String> {
    let b = raw.as_bytes();
    if b.len() < 10 || !b[..9].eq_ignore_ascii_case(b"<!doctype") || !raw.ends_with('>') {
        return None;
    }
    let inner = &raw[9..raw.len() - 1];
    if !inner.starts_with(crate::tree::is_js_whitespace) {
        return None;
    }
    let value = js_trim(inner);
    // `.replace(/^html\b/i, "html")`
    let value = if value.len() >= 4
        && value.as_bytes()[..4].eq_ignore_ascii_case(b"html")
        && value.as_bytes().get(4).is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_'))
    {
        format!("html{}", &value[4..])
    } else {
        value.to_owned()
    };
    Some(crate::ast::collapse_js_whitespace_keep_ends(&value))
}

pub(crate) fn format_doctype(raw: &str) -> String {
    match parse_doctype(raw) {
        Some(value) => format!("{} {value}>", doctype_start_marker(raw)),
        None => {
            let b = raw.as_bytes();
            let starts = b.len() >= 9
                && b[..9].eq_ignore_ascii_case(b"<!doctype")
                && b.get(9).is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_'));
            if starts && !raw.contains('>') { js_trim_end(raw).to_owned() } else { raw.to_owned() }
        }
    }
}
