//! `print/directive.ts` and `directive-spacing-context.ts`.

use super::utils::{directive_arg_spacing, has_blank_line_between, has_line_break, is_blade_component_tag_name, replace_eol_literal, trim_trailing_horizontal_whitespace};
use super::{Between, Printer};
use crate::ast::{Id, is_echo_like};
use crate::directives::Role;
use crate::doc::{Doc, concat, hardline, indent};
use crate::lexer::TokenType;
use crate::tree::{NodeKind, js_trim, js_trim_end};

fn is_branch(kind: NodeKind) -> bool {
    matches!(kind, NodeKind::Directive | NodeKind::PhpTag)
}

impl Printer<'_> {
    pub(crate) fn directive_name(&self, id: Id) -> Option<String> {
        let t = self.tokens_of(id).iter().find(|t| t.kind == TokenType::Directive)?;
        let raw = self.tok_text(t);
        Some(raw.strip_prefix('@').unwrap_or(raw).to_lowercase())
    }

    pub(crate) fn directive_attribute_context_element(&self, id: Id) -> Option<Id> {
        let mut cur = id;
        while let Some(p) = self.n(cur).parent
            && self.kind(p) == NodeKind::DirectiveBlock
        {
            cur = p;
        }
        let parent = self.n(cur).parent?;
        (self.kind(parent) == NodeKind::Element && self.n(cur).end <= self.n(parent).open_tag_end_offset).then_some(parent)
    }

    fn arg_spacing(&self, id: Id) -> &'static str {
        let in_component = self
            .directive_attribute_context_element(id)
            .is_some_and(|e| is_blade_component_tag_name(&self.n(e).full_name));
        directive_arg_spacing(&self.directive_name(id).unwrap_or_default(), in_component)
    }

    pub(crate) fn render_directive_tokens(&self, id: Id) -> String {
        let tokens = self.tokens_of(id);
        let spacing = self.arg_spacing(id);
        let mut result = String::new();
        let mut saw_directive = false;
        let mut saw_args = false;
        for (i, t) in tokens.iter().enumerate() {
            let prev = (i > 0).then(|| tokens[i - 1]);
            let next = tokens.get(i + 1);
            if t.kind == TokenType::Directive {
                result.push_str(self.tok_text(t));
                saw_directive = true;
                if next.is_some_and(|x| x.kind == TokenType::DirectiveArgs) {
                    result.push_str(spacing);
                }
                continue;
            }
            if !saw_directive {
                continue;
            }
            if t.kind == TokenType::Whitespace
                && prev.is_some_and(|p| p.kind == TokenType::Directive)
                && next.is_some_and(|x| x.kind == TokenType::DirectiveArgs)
            {
                result.push_str(spacing);
                continue;
            }
            if t.kind == TokenType::DirectiveArgs && !saw_args {
                result.push_str(&trim_trailing_horizontal_whitespace(self.tok_text(t)));
                saw_args = true;
                continue;
            }
            break;
        }
        if !saw_directive {
            if tokens.len() == 1 && tokens[0].kind == TokenType::PhpBlockEnd {
                return self.full_text(id).to_owned();
            }
            return String::new();
        }
        if self.has_unterminated_directive_args(id) {
            return js_trim_end(&result).to_owned();
        }
        result
    }

    fn has_unterminated_directive_args(&self, id: Id) -> bool {
        for t in self.tokens_of(id) {
            if t.kind != TokenType::DirectiveArgs {
                continue;
            }
            let text = self.tok_text(t);
            if !text.starts_with('(') {
                return false;
            }
            return !js_trim_end(text).ends_with(')');
        }
        false
    }

    pub(crate) fn print_directive_block(&self, id: Id) -> Doc {
        let n = self.n(id);
        if n.children.iter().any(|&c| !is_branch(self.kind(c))) || self.should_fallback_unterminated_root_block(id) {
            return Doc::from(js_trim_end(self.full_text(id)));
        }
        if n.children.is_empty() {
            return Doc::default();
        }
        if self.is_inline_directive_block(id) {
            return self.print_directive_block_inline(id);
        }
        self.print_directive_block_multiline(id)
    }

    fn should_fallback_unterminated_root_block(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.parent.is_none_or(|p| self.kind(p) != NodeKind::Root) {
            return false;
        }
        if self.source()[n.end..].chars().any(|c| !crate::tree::is_js_whitespace(c)) {
            return false;
        }
        let branches: Vec<Id> = n.children.iter().copied().filter(|&c| is_branch(self.kind(c))).collect();
        let Some(&last) = branches.last() else { return false };
        if self.n(last).children.is_empty() {
            return false;
        }
        if branches.iter().any(|&b| self.kind(b) == NodeKind::PhpTag) {
            return true;
        }
        !self.is_simple_unterminated_body(last)
    }

    fn is_simple_unterminated_body(&self, branch: Id) -> bool {
        let meaningful: Vec<Id> = self
            .n(branch)
            .children
            .iter()
            .copied()
            .filter(|&c| !(self.kind(c) == NodeKind::Text && js_trim(&self.n(c).raw_text).is_empty()))
            .collect();
        meaningful.len() == 1 && self.is_simple_leaf_body_node(meaningful[0])
    }

    fn is_simple_leaf_body_node(&self, id: Id) -> bool {
        let n = self.n(id);
        if matches!(n.kind, NodeKind::Text) || is_echo_like(n.kind) {
            return true;
        }
        if n.kind != NodeKind::Element || !n.attrs.is_empty() {
            return false;
        }
        let mut stack: Vec<Id> = n.children.clone();
        while let Some(c) = stack.pop() {
            let cn = self.n(c);
            if matches!(cn.kind, NodeKind::Attribute | NodeKind::JsxAttribute) {
                return false;
            }
            if cn.kind == NodeKind::Element {
                return false;
            }
            stack.extend(cn.children.iter().copied());
        }
        true
    }

    fn print_directive_body(&self, branch: Id) -> Vec<Doc> {
        let children = self.n(branch).children.clone();
        let rendered: Vec<(Doc, Vec<Doc>)> = children
            .iter()
            .map(|&c| {
                if is_branch(self.kind(c)) && !self.n(c).children.is_empty() {
                    let branch_doc = if self.kind(c) == NodeKind::Directive {
                        Doc::from(self.render_directive_tokens(c))
                    } else {
                        self.print(c)
                    };
                    (branch_doc, self.print_directive_body(c))
                } else {
                    (self.print_directive_body_child(c), Vec::new())
                }
            })
            .collect();
        let segments: Vec<usize> =
            (0..children.len()).filter(|&i| self.kind(children[i]) != NodeKind::AttributeWhitespace).collect();
        let mut docs = Vec::new();
        for (si, &ci) in segments.iter().enumerate() {
            let child = children[ci];
            if !docs.is_empty() && si > 0 {
                docs.push(self.directive_between_segments(children[segments[si - 1]], child).doc());
            }
            let (branch_doc, nested) = &rendered[ci];
            docs.push(branch_doc.clone());
            if is_branch(self.kind(child)) && !self.n(child).children.is_empty() && !nested.is_empty() {
                docs.push(indent(concat(vec![hardline(), concat(nested.clone())])));
            }
        }
        docs
    }

    fn print_directive_body_inline(&self, branch: Id) -> Vec<Doc> {
        let children = self.n(branch).children.clone();
        let printed: Vec<Doc> = children.iter().map(|&c| self.print_directive_body_child(c)).collect();
        let mut docs = Vec::new();
        for (i, &c) in children.iter().enumerate() {
            if i > 0 {
                let prev = children[i - 1];
                let ignore = self.kind(prev) == NodeKind::IgnoreRange || self.kind(c) == NodeKind::IgnoreRange;
                if ignore {
                    let between = self.segment_source_between(prev, c);
                    if !between.is_empty() {
                        docs.push(if has_blank_line_between(between) {
                            concat(vec![hardline(), hardline()])
                        } else if has_line_break(between) {
                            hardline()
                        } else {
                            Doc::from(between)
                        });
                    }
                } else if self.node_source_between(prev, c).chars().any(crate::tree::is_js_whitespace) {
                    docs.push(Doc::from(" "));
                }
            }
            if !printed[i].is_empty_str() {
                docs.push(printed[i].clone());
            }
        }
        docs
    }

    fn print_directive_block_inline(&self, id: Id) -> Doc {
        let mut segments = Vec::new();
        let mut previous: Option<Id> = None;
        for &c in &self.n(id).children.clone() {
            if !is_branch(self.kind(c)) {
                segments.push(self.print(c));
                continue;
            }
            if !segments.is_empty() {
                let sep = self.inline_directive_separator(previous, c);
                if !sep.is_empty() {
                    segments.push(Doc::from(sep));
                }
            }
            segments.push(self.print(c));
            if !self.n(c).children.is_empty() {
                let body = self.print_directive_body_inline(c);
                if !body.is_empty() {
                    segments.push(Doc::from(" "));
                    segments.push(concat(body));
                }
            }
            previous = Some(c);
        }
        concat(segments)
    }

    fn print_directive_block_multiline(&self, id: Id) -> Doc {
        let children = self.n(id).children.clone();
        let branches: Vec<Id> = children.iter().copied().filter(|&c| is_branch(self.kind(c))).collect();
        if branches.is_empty() {
            return concat(children.iter().map(|&c| self.print(c)).collect());
        }
        let mut segments = Vec::new();
        let mut separators = Vec::new();
        let mut index = 0;
        for &c in &children {
            if !is_branch(self.kind(c)) {
                continue;
            }
            let mut segment = vec![self.print(c)];
            if !self.n(c).children.is_empty() {
                let body = self.print_directive_body(c);
                if !body.is_empty() {
                    segment.push(indent(concat(vec![hardline(), concat(body)])));
                }
            }
            segments.push(concat(segment));
            let Some(&next) = branches.get(index + 1) else { continue };
            separators.push(self.directive_separator(c, next));
            index += 1;
        }
        if segments.len() == 1 {
            return segments.pop().unwrap_or_default();
        }
        let mut out = vec![segments[0].clone()];
        for (i, sep) in separators.into_iter().enumerate() {
            out.push(sep);
            out.push(segments[i + 1].clone());
        }
        concat(out)
    }

    fn print_directive_body_child(&self, id: Id) -> Doc {
        if self.has_prettier_ignore(id) {
            return self.print_ignored_child(id);
        }
        let n = self.n(id);
        if n.kind == NodeKind::Text
            && (n.prev.is_some_and(|p| is_echo_like(self.kind(p))) || n.next.is_some_and(|x| is_echo_like(self.kind(x))))
        {
            return replace_eol_literal(&n.raw_text);
        }
        self.print(id)
    }

    fn directive_separator(&self, prev: Id, next: Id) -> Doc {
        if self.should_suppress_directive_separator(prev, next) {
            return Doc::default();
        }
        let prev_end = self.last_child(prev).map_or(self.n(prev).end, |c| self.n(c).end);
        let start = self.n(next).start;
        let between = if prev_end <= start { &self.source()[prev_end..start] } else { "" };
        if has_blank_line_between(between) { concat(vec![hardline(), hardline()]) } else { hardline() }
    }

    fn inline_directive_separator(&self, prev: Option<Id>, next: Id) -> &'static str {
        let Some(prev) = prev else { return "" };
        if self.should_suppress_directive_separator(prev, next) {
            return "";
        }
        let between = self.node_source_between(prev, next);
        if between.is_empty() {
            return "";
        }
        if between.ends_with(crate::tree::is_js_whitespace) { " " } else { "" }
    }

    fn php_tag_keyword(&self, id: Id) -> Option<String> {
        if self.kind(id) != NodeKind::PhpTag {
            return None;
        }
        let text = js_trim(self.full_text(id));
        let rest = text.strip_prefix("<?")?;
        let ident = |s: &str| -> Option<String> {
            let s = crate::tree::js_trim_start(s);
            let b = s.as_bytes();
            if b.is_empty() || !(b[0].is_ascii_alphabetic() || b[0] == b'_') {
                return None;
            }
            let len = b.iter().take_while(|c| c.is_ascii_alphanumeric() || **c == b'_').count();
            Some(s[..len].to_lowercase())
        };
        if rest.len() >= 3
            && rest.as_bytes()[..3].eq_ignore_ascii_case(b"php")
            && let Some(k) = ident(&rest[3..])
        {
            return Some(k);
        }
        ident(rest)
    }

    fn should_suppress_directive_separator(&self, prev: Id, next: Id) -> bool {
        self.n(prev).children.is_empty()
            && self.php_tag_keyword(prev).as_deref() == Some("switch")
            && matches!(self.php_tag_keyword(next).as_deref(), Some("case" | "default"))
    }

    fn is_inline_directive_block(&self, id: Id) -> bool {
        let directives: Vec<Id> = self.n(id).children.iter().copied().filter(|&c| is_branch(self.kind(c))).collect();
        if directives.is_empty() {
            return false;
        }
        for (i, &d) in directives.iter().enumerate() {
            let dn = self.n(d);
            if !dn.children.is_empty() {
                if self.n(dn.children[0]).start_line > dn.end_line {
                    return false;
                }
                let in_open_tag = self.directive_attribute_context_element(d).is_some();
                for &c in &dn.children {
                    let cn = self.n(c);
                    if cn.end_line > cn.start_line {
                        return false;
                    }
                    if !in_open_tag && cn.kind == NodeKind::Element {
                        return false;
                    }
                }
                for j in 1..dn.children.len() {
                    if self.n(dn.children[j]).start_line > self.n(dn.children[j - 1]).end_line {
                        return false;
                    }
                }
            }
            let branch_end_line = dn.children.last().map_or(dn.end_line, |&c| self.n(c).end_line);
            if let Some(&next) = directives.get(i + 1)
                && self.n(next).start_line > branch_end_line
            {
                return false;
            }
        }
        true
    }

    fn node_source_between(&self, prev: Id, next: Id) -> &str {
        let (s, e) = (self.n(prev).end, self.n(next).start);
        if s <= e { &self.source()[s..e] } else { "" }
    }

    fn segment_source_between(&self, prev: Id, next: Id) -> &str {
        let (s, e) = (self.printable_subtree_end(prev), self.n(next).start);
        if s <= e { &self.source()[s..e] } else { "" }
    }

    fn directive_between_segments(&self, prev: Id, next: Id) -> Between {
        if self.kind(prev) == NodeKind::IgnoreRange || self.kind(next) == NodeKind::IgnoreRange {
            let between = self.segment_source_between(prev, next);
            if between.is_empty() {
                return Between::Empty;
            }
            if !has_line_break(between) {
                return Between::Text(between.to_owned());
            }
            if has_blank_line_between(between) && self.should_preserve_body_blank_line(prev, next) {
                return Between::DoubleHardline;
            }
            return Between::Hardline;
        }
        self.directive_between_line(prev, next)
    }

    fn directive_between_line(&self, prev: Id, next: Id) -> Between {
        let p = self.n(prev);
        let x = self.n(next);
        if p.kind == NodeKind::NonOutput || x.kind == NodeKind::NonOutput {
            return Between::Empty;
        }
        let between = self.node_source_between(prev, next);
        let line_break = has_line_break(between) || x.start_line > p.end_line;
        if p.kind == NodeKind::IgnoreRange || x.kind == NodeKind::IgnoreRange {
            if between.is_empty() {
                return Between::Empty;
            }
            if has_blank_line_between(between) {
                return Between::DoubleHardline;
            }
            if line_break {
                return Between::Hardline;
            }
            return Between::Text(between.to_owned());
        }
        if has_blank_line_between(between) && self.should_preserve_body_blank_line(prev, next) {
            return Between::DoubleHardline;
        }
        let echo = is_echo_like(p.kind) || is_echo_like(x.kind);
        if self.is_text_like(prev) && self.is_text_like(next) {
            if line_break && echo {
                return Between::Hardline;
            }
            if p.is_trailing_space_sensitive {
                if p.has_trailing_spaces {
                    if echo {
                        return if line_break { Between::Hardline } else { Between::Text(" ".to_owned()) };
                    }
                    return if self.prefer_hardline_as_leading_spaces(next) { Between::Hardline } else { Between::Line };
                }
                return Between::Empty;
            }
            if echo {
                return if line_break { Between::Hardline } else { Between::Empty };
            }
            return if self.prefer_hardline_as_leading_spaces(next) { Between::Hardline } else { Between::Softline };
        }
        if !x.is_leading_space_sensitive || self.prefer_hardline_as_leading_spaces(next) {
            return Between::Hardline;
        }
        if x.has_leading_spaces {
            return Between::Line;
        }
        Between::Softline
    }

    fn should_preserve_body_blank_line(&self, prev: Id, next: Id) -> bool {
        if self.is_branch_boundary_directive(prev) || self.is_branch_boundary_directive(next) {
            return false;
        }
        self.is_meaningful_body_sibling(prev) && self.is_meaningful_body_sibling(next)
    }

    fn is_branch_boundary_directive(&self, id: Id) -> bool {
        if self.kind(id) != NodeKind::Directive {
            return false;
        }
        let Some(name) = self.directive_name(id) else { return false };
        let Some(d) = self.ast.directives.get(&name) else { return false };
        matches!(d.role, Role::Mixed | Role::Closing) || d.is_switch_branch || d.is_switch_terminator || d.is_conditional_close
    }

    fn is_meaningful_body_sibling(&self, id: Id) -> bool {
        let n = self.n(id);
        if n.kind == NodeKind::NonOutput {
            return false;
        }
        !(n.kind == NodeKind::Text && js_trim(self.full_text(id)).is_empty())
    }
}
