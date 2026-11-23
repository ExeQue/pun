//! The printable tree: prettier-plugin-blade's `parser.ts` (`wrapTree`) and `preprocess.ts`.
//!
//! Nodes live in an arena (`Ast::nodes`) and refer to each other by index, standing in for the
//! plugin's mutable `WrappedNode` object graph.

use crate::directives::{Directives, TreeDirectives};
use crate::frontmatter::{FrontMatter, parse_front_matter};
use crate::lexer::{Token, TokenType, collect_ignore_ranges, tokenize};
use crate::tree::{FlatNode, NONE, NodeKind, TreeBuilder, VOID_ELEMENTS, js_trim};

pub(crate) type Id = usize;

#[derive(Debug, Clone, Default)]
pub(crate) struct Node {
    pub(crate) kind: NodeKind,
    /// Index into `Ast::flat`.
    pub(crate) flat: usize,
    pub(crate) parent: Option<Id>,
    pub(crate) children: Vec<Id>,
    pub(crate) attrs: Vec<Id>,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) raw_text: String,
    pub(crate) tag_name: String,
    pub(crate) raw_tag_name: String,
    pub(crate) raw_closing_tag_name: String,
    pub(crate) full_name: String,
    pub(crate) name: String,
    pub(crate) namespace: String,
    pub(crate) has_closing_tag: bool,
    pub(crate) open_tag_end_offset: usize,
    pub(crate) closing_tag_start_offset: usize,
    pub(crate) start_line: usize,
    pub(crate) end_line: usize,
    pub(crate) prev: Option<Id>,
    pub(crate) next: Option<Id>,
    pub(crate) is_self_closing: bool,
    pub(crate) has_htm_component_closing_tag: bool,
    pub(crate) condition: String,
    pub(crate) complete: bool,
    pub(crate) conditional_start_is_revealed: bool,
    pub(crate) conditional_end_is_hidden: bool,
    pub(crate) is_ie_conditional_start_comment: bool,
    pub(crate) ie_conditional_start_condition: String,
    pub(crate) css_display: String,
    pub(crate) is_whitespace_sensitive: bool,
    pub(crate) is_indentation_sensitive: bool,
    pub(crate) is_leading_space_sensitive: bool,
    pub(crate) is_trailing_space_sensitive: bool,
    pub(crate) is_dangling_space_sensitive: bool,
    pub(crate) has_leading_spaces: bool,
    pub(crate) has_trailing_spaces: bool,
    pub(crate) has_dangling_spaces: bool,
}

pub(crate) struct Ast {
    pub(crate) nodes: Vec<Node>,
    /// The source with any front matter blanked out (what every offset points into).
    pub(crate) source: String,
    pub(crate) tokens: Vec<Token>,
    pub(crate) flat: Vec<FlatNode>,
    pub(crate) directives: TreeDirectives,
    pub(crate) front_matter: Option<FrontMatter>,
    pub(crate) line_offsets: Vec<usize>,
}

pub(crate) const ROOT: Id = 0;

pub(crate) fn build_line_offsets(source: &str) -> Vec<usize> {
    let b = source.as_bytes();
    let mut offsets = vec![0];
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\n' {
            offsets.push(i + 1);
        } else if b[i] == b'\r' {
            if i + 1 < b.len() && b[i + 1] == b'\n' {
                i += 1;
            }
            offsets.push(i + 1);
        }
        i += 1;
    }
    offsets
}

pub(crate) fn get_line(offset: usize, line_offsets: &[usize]) -> usize {
    line_offsets
        .partition_point(|&o| o <= offset)
        .saturating_sub(1)
}

fn is_internal_kind(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::ElementName
            | NodeKind::ClosingElementName
            | NodeKind::Attribute
            | NodeKind::JsxAttribute
            | NodeKind::AttributeName
            | NodeKind::AttributeValue
            | NodeKind::AttributeWhitespace
    )
}

fn is_opening_tag_internal_kind(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::ElementName
            | NodeKind::Attribute
            | NodeKind::JsxAttribute
            | NodeKind::AttributeWhitespace
            | NodeKind::Directive
            | NodeKind::DirectiveBlock
            | NodeKind::Echo
            | NodeKind::RawEcho
            | NodeKind::TripleEcho
            | NodeKind::PhpTag
            | NodeKind::PhpBlock
    )
}

fn should_materialize_raw_text(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Text
            | NodeKind::Comment
            | NodeKind::BogusComment
            | NodeKind::BladeComment
            | NodeKind::IgnoreRange
            | NodeKind::Doctype
            | NodeKind::ElementName
            | NodeKind::ClosingElementName
    )
}

/// `/\s+/g` → `" "` after trimming (JS `\s`).
pub(crate) fn collapse_js_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in js_trim(s).chars() {
        if crate::tree::is_js_whitespace(c) {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

/// `/\s+/g` → `" "` without trimming.
pub(crate) fn collapse_js_whitespace_keep_ends(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if crate::tree::is_js_whitespace(c) {
            if !in_ws {
                out.push(' ');
            }
            in_ws = true;
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

/// `<!--([\s\S]*?)-->` value of a comment's raw text.
fn comment_value(raw: &str) -> Option<&str> {
    raw.strip_prefix("<!--")?.strip_suffix("-->")
}

/// `^\[if([^\]]*)\]><!$` on a comment value.
fn ie_conditional_start_condition(raw: &str) -> Option<String> {
    let value = comment_value(raw)?;
    let rest = value.strip_prefix("[if")?;
    let close = rest.find(']')?;
    if &rest[close..] != "]><!" {
        return None;
    }
    Some(collapse_js_whitespace(&rest[..close]))
}

/// `^<!\s*\[endif\]$` on a comment value.
fn is_ie_conditional_end_comment(node: &Node) -> bool {
    if node.kind != NodeKind::Comment {
        return false;
    }
    let Some(value) = comment_value(&node.raw_text) else {
        return false;
    };
    let Some(rest) = value.strip_prefix("<!") else {
        return false;
    };
    js_trim_start_ws(rest) == "[endif]"
}

fn js_trim_start_ws(s: &str) -> &str {
    crate::tree::js_trim_start(s)
}

pub(crate) fn parse(text: &str) -> Ast {
    let (front_matter, content) = parse_front_matter(text);
    let ignore_ranges = collect_ignore_ranges(&content);
    let tokens = tokenize(&content, &Directives::accept_all(), &ignore_ranges);
    let mut directives = TreeDirectives::with_defaults();
    let names: Vec<String> = tokens
        .iter()
        .filter(|t| t.kind == TokenType::Directive)
        .map(|t| {
            let name = content[t.start..t.end].to_lowercase();
            match name.strip_prefix('@') {
                Some(n) => n.to_owned(),
                None => name,
            }
        })
        .collect();
    directives.train(&names);
    let flat = TreeBuilder::new(&tokens, &content, &directives).build();
    let line_offsets = build_line_offsets(&content);
    let mut ast = Ast {
        nodes: Vec::new(),
        source: content,
        tokens,
        flat,
        directives,
        front_matter,
        line_offsets,
    };
    wrap_tree(&mut ast);
    if ast.front_matter.is_some() {
        prepend_front_matter(&mut ast);
    }
    ast
}

fn token_lines(tokens: &[Token], line_offsets: &[usize]) -> (Vec<usize>, Vec<usize>) {
    let mut starts = Vec::with_capacity(tokens.len());
    let mut ends = Vec::with_capacity(tokens.len());
    let mut line = 0;
    for t in tokens {
        while line + 1 < line_offsets.len() && line_offsets[line + 1] <= t.start {
            line += 1;
        }
        starts.push(line);
        let end = if t.end > t.start { t.end - 1 } else { t.end };
        while line + 1 < line_offsets.len() && line_offsets[line + 1] <= end {
            line += 1;
        }
        ends.push(line);
    }
    (starts, ends)
}

fn wrap_tree(ast: &mut Ast) {
    let (start_lines, end_lines) = token_lines(&ast.tokens, &ast.line_offsets);
    let tokens = &ast.tokens;
    let source = &ast.source;
    let mut nodes: Vec<Node> = Vec::with_capacity(ast.flat.len());
    for (i, f) in ast.flat.iter().enumerate() {
        let (start, end, start_line, end_line) = if f.token_count > 0 {
            let first = f.token_start;
            let last = f.token_start + f.token_count - 1;
            (
                tokens[first].start,
                tokens[last].end,
                start_lines[first],
                end_lines[last],
            )
        } else {
            (0, 0, 0, 0)
        };
        nodes.push(Node {
            kind: f.kind,
            flat: i,
            start,
            end,
            raw_text: if should_materialize_raw_text(f.kind) {
                source[start..end].to_owned()
            } else {
                String::new()
            },
            complete: true,
            css_display: "inline".to_owned(),
            start_line,
            end_line,
            ..Node::default()
        });
    }

    for i in 0..ast.flat.len() {
        let f = &ast.flat[i];
        let mut child_idx = f.first_child;
        while child_idx != NONE {
            nodes[child_idx].parent = Some(i);
            let child_kind = nodes[child_idx].kind;
            if is_internal_kind(child_kind) {
                if matches!(child_kind, NodeKind::Attribute | NodeKind::JsxAttribute) {
                    if nodes[i].kind == NodeKind::Element {
                        nodes[i].attrs.push(child_idx);
                    } else {
                        nodes[i].children.push(child_idx);
                    }
                } else if child_kind == NodeKind::ElementName {
                    let raw = nodes[child_idx].raw_text.clone();
                    let parent = &mut nodes[i];
                    parent.raw_tag_name.clone_from(&raw);
                    parent.full_name.clone_from(&raw);
                    if let Some(colon) = raw.find(':') {
                        parent.namespace = raw[..colon].to_owned();
                        parent.name = raw[colon + 1..].to_owned();
                        parent.tag_name = raw[colon + 1..].to_lowercase();
                    } else {
                        parent.namespace = String::new();
                        parent.tag_name = raw.to_lowercase();
                        parent.name = raw;
                    }
                } else if child_kind == NodeKind::ClosingElementName {
                    let raw = nodes[child_idx].raw_text.clone();
                    nodes[i].has_closing_tag = true;
                    nodes[i].raw_closing_tag_name = raw;
                }
            } else {
                nodes[i].children.push(child_idx);
            }
            child_idx = ast.flat[child_idx].next_sibling;
        }

        if f.kind == NodeKind::Element && f.token_count > 0 {
            let token_end = f.token_start + f.token_count;
            let mut opening_end = token_end;
            let mut c = f.first_child;
            while c != NONE {
                if !is_opening_tag_internal_kind(nodes[c].kind) {
                    opening_end = opening_end.min(ast.flat[c].token_start);
                    break;
                }
                c = ast.flat[c].next_sibling;
            }
            if let Some(t) = tokens[f.token_start..opening_end]
                .iter()
                .find(|t| t.kind == TokenType::GreaterThan)
            {
                nodes[i].open_tag_end_offset = t.end;
            }
            if nodes[i].has_closing_tag {
                let mut j = token_end - 1;
                while j > f.token_start {
                    if tokens[j].kind == TokenType::Slash
                        && tokens[j - 1].kind == TokenType::LessThan
                    {
                        nodes[i].closing_tag_start_offset = tokens[j - 1].start;
                        break;
                    }
                    j -= 1;
                }
            }
        }

        if f.kind == NodeKind::ConditionalComment && f.token_count > 0 {
            let first = tokens[f.token_start];
            if first.kind == TokenType::ConditionalCommentStart {
                let text = &source[first.start..first.end];
                nodes[i].conditional_start_is_revealed = text.ends_with("]><!-->");
                // ^<!--\[if\s*([^\]]*)\]>
                if let Some(rest) = text.strip_prefix("<!--[if")
                    && let Some(close) = rest.find(']')
                    && rest[close..].starts_with("]>")
                {
                    nodes[i].condition = collapse_js_whitespace(&rest[..close]);
                }
                nodes[i].open_tag_end_offset = first.end;
            }
            let last = tokens[f.token_start + f.token_count - 1];
            let complete = last.kind == TokenType::ConditionalCommentEnd;
            nodes[i].complete = complete;
            if complete {
                nodes[i].conditional_end_is_hidden =
                    source[last.start..last.end].starts_with("<!--<![");
                nodes[i].has_closing_tag = true;
                nodes[i].closing_tag_start_offset = last.start;
            }
        }

        if f.kind == NodeKind::Comment
            && let Some(cond) = ie_conditional_start_condition(&nodes[i].raw_text)
        {
            nodes[i].is_ie_conditional_start_comment = true;
            nodes[i].ie_conditional_start_condition = cond;
        }
    }
    ast.nodes = nodes;
}

fn prepend_front_matter(ast: &mut Ast) {
    let Some(fm) = &ast.front_matter else { return };
    let end_line = get_line(fm.end.saturating_sub(1), &ast.line_offsets);
    let node = Node {
        kind: NodeKind::FrontMatter,
        flat: ROOT,
        parent: Some(ROOT),
        start: 0,
        end: fm.end,
        raw_text: fm.raw.clone(),
        end_line,
        is_self_closing: true,
        complete: true,
        css_display: "inline".to_owned(),
        ..Node::default()
    };
    let id = ast.nodes.len();
    ast.nodes.push(node);
    ast.nodes[ROOT].children.insert(0, id);
}

// ---------------------------------------------------------------------------------------------
// preprocess.ts

const CSS_DISPLAY_TAGS: &[(&str, &str)] = &[
    ("area", "none"),
    ("base", "none"),
    ("basefont", "none"),
    ("datalist", "none"),
    ("head", "none"),
    ("link", "none"),
    ("meta", "none"),
    ("noembed", "none"),
    ("noframes", "none"),
    ("param", "block"),
    ("rp", "none"),
    ("title", "none"),
    ("script", "block"),
    ("style", "block"),
    ("template", "inline"),
    ("address", "block"),
    ("article", "block"),
    ("aside", "block"),
    ("blockquote", "block"),
    ("body", "block"),
    ("center", "block"),
    ("dd", "block"),
    ("details", "block"),
    ("dialog", "block"),
    ("dir", "block"),
    ("div", "block"),
    ("dl", "block"),
    ("dt", "block"),
    ("fieldset", "block"),
    ("figcaption", "block"),
    ("figure", "block"),
    ("footer", "block"),
    ("form", "block"),
    ("h1", "block"),
    ("h2", "block"),
    ("h3", "block"),
    ("h4", "block"),
    ("h5", "block"),
    ("h6", "block"),
    ("header", "block"),
    ("hgroup", "block"),
    ("hr", "block"),
    ("html", "block"),
    ("legend", "block"),
    ("li", "block"),
    ("listing", "block"),
    ("main", "block"),
    ("marquee", "inline-block"),
    ("menu", "block"),
    ("nav", "block"),
    ("ol", "block"),
    ("optgroup", "block"),
    ("option", "block"),
    ("p", "block"),
    ("plaintext", "block"),
    ("pre", "block"),
    ("search", "block"),
    ("section", "block"),
    ("source", "block"),
    ("summary", "block"),
    ("table", "table"),
    ("track", "block"),
    ("ul", "block"),
    ("xmp", "block"),
    ("caption", "table-caption"),
    ("tr", "table-row"),
    ("td", "table-cell"),
    ("th", "table-cell"),
    ("thead", "table-header-group"),
    ("tbody", "table-row-group"),
    ("tfoot", "table-footer-group"),
    ("col", "table-column"),
    ("colgroup", "table-column-group"),
    ("button", "inline-block"),
    ("meter", "inline-block"),
    ("progress", "inline-block"),
    ("select", "inline-block"),
    ("textarea", "inline-block"),
    ("video", "inline-block"),
    ("audio", "inline-block"),
    ("object", "inline-block"),
    ("ruby", "ruby"),
    ("rt", "ruby-text"),
    ("svg", "inline-block"),
];

const CSS_WHITE_SPACE_TAGS: &[(&str, &str)] = &[
    ("listing", "pre"),
    ("plaintext", "pre"),
    ("pre", "pre"),
    ("xmp", "pre"),
    ("nobr", "nowrap"),
    ("table", "initial"),
    ("textarea", "pre-wrap"),
    ("td", "normal"),
    ("th", "normal"),
];

fn lookup(table: &'static [(&'static str, &'static str)], key: &str) -> Option<&'static str> {
    table.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

pub(crate) fn is_block_like_css_display(display: &str) -> bool {
    display == "block" || display == "list-item" || display.starts_with("table")
}

fn is_html_ws(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{c}' | '\r' | ' ')
}

pub(crate) fn html_trim(s: &str) -> &str {
    s.trim_matches(is_html_ws)
}

pub(crate) fn html_trim_start(s: &str) -> &str {
    s.trim_start_matches(is_html_ws)
}

pub(crate) fn html_trim_end(s: &str) -> &str {
    s.trim_end_matches(is_html_ws)
}

pub(crate) fn has_html_whitespace(s: &str) -> bool {
    s.chars().any(is_html_ws)
}

const IGNORE_FIRST_LF_TAGS: &[&str] = &["pre", "textarea", "listing"];

impl Ast {
    pub(crate) fn node(&self, id: Id) -> &Node {
        &self.nodes[id]
    }

    pub(crate) fn has_parent(&self, id: Id, pred: impl Fn(&Node) -> bool) -> bool {
        let mut cur = Some(id);
        while let Some(c) = cur {
            if pred(&self.nodes[c]) {
                return true;
            }
            cur = self.nodes[c].parent;
        }
        false
    }

    pub(crate) fn is_unknown_namespace(&self, id: Id) -> bool {
        let n = &self.nodes[id];
        let ns = n.namespace.to_lowercase();
        n.kind == NodeKind::Element
            && !n.full_name.contains(':')
            && !ns.is_empty()
            && ns != "html"
            && ns != "svg"
    }

    pub(crate) fn is_script_like_tag(&self, id: Id) -> bool {
        let n = &self.nodes[id];
        if n.kind != NodeKind::Element {
            return false;
        }
        let full = n.full_name.to_lowercase();
        let script_or_style = n.tag_name == "script" || n.tag_name == "style";
        full == "script"
            || full == "style"
            || full == "svg:style"
            || full == "svg:script"
            || ((n.namespace == "svg"
                || n.parent.is_some_and(|p| self.nodes[p].namespace == "svg"))
                && script_or_style)
            || (self.is_unknown_namespace(id) && script_or_style)
    }

    fn in_svg_foreign_object(&self, id: Id) -> bool {
        self.has_parent(id, |p| {
            p.kind == NodeKind::Element && p.namespace == "svg" && p.tag_name == "foreignobject"
        })
    }

    fn prev_non_whitespace_sibling(&self, id: Id) -> Option<Id> {
        let mut s = self.nodes[id].prev;
        while let Some(i) = s {
            let n = &self.nodes[i];
            let ws_only = n.kind == NodeKind::Text
                && n.raw_text
                    .bytes()
                    .all(|b| matches!(b, b'\t' | b'\n' | 0x0c | b'\r' | b' '));
            if !ws_only {
                return Some(i);
            }
            s = n.prev;
        }
        None
    }

    fn css_display(&self, id: Id) -> String {
        let n = &self.nodes[id];
        if n.kind == NodeKind::Element && is_blade_slot_name(&n.full_name) {
            return "block".to_owned();
        }
        if let Some(prev) = self.prev_non_whitespace_sibling(id)
            && self.nodes[prev].kind == NodeKind::Comment
            && let Some(display) = display_hint(&self.nodes[prev].raw_text)
        {
            return display;
        }
        if n.kind == NodeKind::Element && n.namespace == "svg" && !self.in_svg_foreign_object(id) {
            return if n.tag_name == "svg" {
                "inline-block"
            } else {
                "block"
            }
            .to_owned();
        }
        if n.kind == NodeKind::Element
            && (n.namespace.is_empty()
                || self.in_svg_foreign_object(id)
                || self.is_unknown_namespace(id))
            && let Some(d) = lookup(CSS_DISPLAY_TAGS, &n.tag_name)
        {
            return d.to_owned();
        }
        if matches!(n.kind, NodeKind::DirectiveBlock | NodeKind::Root) {
            return "block".to_owned();
        }
        "inline".to_owned()
    }

    pub(crate) fn css_white_space(&self, id: Id) -> &'static str {
        let n = &self.nodes[id];
        if n.kind == NodeKind::Element
            && (n.namespace.is_empty() || self.is_unknown_namespace(id))
            && let Some(ws) = lookup(CSS_WHITE_SPACE_TAGS, &n.tag_name)
        {
            return ws;
        }
        "normal"
    }

    pub(crate) fn is_pre_like(&self, id: Id) -> bool {
        self.css_white_space(id).starts_with("pre")
    }

    fn walk(&self, id: Id, out: &mut Vec<Id>) {
        out.push(id);
        for &c in &self.nodes[id].children {
            self.walk(c, out);
        }
    }

    /// Pre-order list of the nodes reachable through `children`.
    fn preorder(&self) -> Vec<Id> {
        let mut out = Vec::with_capacity(self.nodes.len());
        self.walk(ROOT, &mut out);
        out
    }

    fn remove_ignorable_first_lf(&mut self, id: Id) {
        let n = &self.nodes[id];
        if n.kind == NodeKind::Element
            && IGNORE_FIRST_LF_TAGS.contains(&n.full_name.to_lowercase().as_str())
            && let Some(&first) = n.children.first()
            && self.nodes[first].kind == NodeKind::Text
            && self.nodes[first].raw_text.starts_with('\n')
        {
            if self.nodes[first].raw_text.len() == 1 {
                self.nodes[id].children.remove(0);
            } else {
                let t = &mut self.nodes[first];
                t.raw_text.remove(0);
                t.start += 1;
            }
        }
        for c in self.nodes[id].children.clone() {
            self.remove_ignorable_first_lf(c);
        }
    }

    fn merge_ie_conditional_comments(&mut self, id: Id) {
        if self.nodes[id].children.is_empty() {
            return;
        }
        let mut i = 0;
        while i < self.nodes[id].children.len() {
            let child = self.nodes[id].children[i];
            if self.nodes[child].kind != NodeKind::Element {
                self.merge_ie_conditional_comments(child);
                i += 1;
                continue;
            }
            let prev = (i > 0).then(|| self.nodes[id].children[i - 1]);
            let first = self.nodes[child].children.first().copied();
            let condition = prev
                .filter(|&p| self.nodes[p].kind == NodeKind::Comment)
                .and_then(|p| ie_conditional_start_condition(&self.nodes[p].raw_text))
                .filter(|c| !c.is_empty());
            let can_merge = match (condition.as_ref(), prev, first) {
                (Some(_), Some(p), Some(f)) => {
                    self.nodes[p].end == self.nodes[child].start
                        && self.nodes[f].kind == NodeKind::Comment
                        && self.nodes[f].start == self.nodes[child].open_tag_end_offset
                        && is_ie_conditional_end_comment(&self.nodes[f])
                }
                _ => false,
            };
            if !can_merge {
                self.merge_ie_conditional_comments(child);
                i += 1;
                continue;
            }
            let (p, f) = (prev.unwrap_or(child), first.unwrap_or(child));
            let (p_start, p_line, f_end) = (
                self.nodes[p].start,
                self.nodes[p].start_line,
                self.nodes[f].end,
            );
            let c = &mut self.nodes[child];
            c.condition = condition.unwrap_or_default();
            c.start = p_start;
            c.start_line = p_line;
            c.open_tag_end_offset = f_end;
            c.children.remove(0);
            self.nodes[id].children.remove(i - 1);
            self.merge_ie_conditional_comments(child);
            // `i--` then `i++`: the merged element now sits at `i - 1`; continue after it.
        }
    }

    fn add_implicit_namespaces(&mut self, id: Id, inherited: &str) {
        let mut child_ns = inherited.to_owned();
        if self.nodes[id].kind == NodeKind::Element {
            if !self.nodes[id].full_name.contains(':') {
                let parent_tag = self.nodes[id]
                    .parent
                    .filter(|&p| self.nodes[p].kind == NodeKind::Element)
                    .map(|p| self.nodes[p].tag_name.clone());
                if self.nodes[id].tag_name == "svg"
                    || (inherited == "svg"
                        && parent_tag.as_deref().is_some_and(|t| t != "foreignobject"))
                {
                    "svg".clone_into(&mut self.nodes[id].namespace);
                } else if inherited == "svg" && parent_tag.as_deref() == Some("foreignobject") {
                    self.nodes[id].namespace = String::new();
                }
            }
            child_ns.clone_from(&self.nodes[id].namespace);
            if self.nodes[id].namespace == "svg" && self.nodes[id].tag_name == "foreignobject" {
                child_ns = String::new();
            }
        }
        for c in self.nodes[id].children.clone() {
            self.add_implicit_namespaces(c, &child_ns);
        }
    }

    fn add_siblings(&mut self) {
        for id in self.preorder() {
            let children = self.nodes[id].children.clone();
            for (i, &c) in children.iter().enumerate() {
                self.nodes[c].prev = (i > 0).then(|| children[i - 1]);
                self.nodes[c].next = children.get(i + 1).copied();
            }
        }
    }

    fn extract_whitespace(&mut self, id: Id) {
        let children = &self.nodes[id].children;
        if children.is_empty() {
            return;
        }
        if children.len() == 1
            && self.nodes[children[0]].kind == NodeKind::Text
            && html_trim(&self.nodes[children[0]].raw_text).is_empty()
        {
            let dangling = !self.nodes[children[0]].raw_text.is_empty();
            self.nodes[id].has_dangling_spaces = dangling;
            self.nodes[id].children.clear();
            return;
        }
        if self.nodes[id].is_whitespace_sensitive {
            return;
        }
        let mut children = self.nodes[id].children.clone();
        let mut i = children.len();
        while i > 0 {
            i -= 1;
            let child = children[i];
            if self.nodes[child].kind != NodeKind::Text {
                continue;
            }
            let text = self.nodes[child].raw_text.clone();
            let trimmed = html_trim(&text);
            let prev = (i > 0).then(|| children[i - 1]);
            if trimmed.is_empty() {
                let next = children.get(i + 1).copied();
                if let Some(p) = prev {
                    self.nodes[p].has_trailing_spaces = true;
                }
                if let Some(n) = next {
                    self.nodes[n].has_leading_spaces = true;
                }
                if prev.is_none() && next.is_none() {
                    self.nodes[id].has_dangling_spaces = true;
                }
                children.remove(i);
                continue;
            }
            let leading = text.len() - html_trim_start(&text).len();
            let trailing = text.len() - html_trim_end(&text).len();
            if leading > 0 {
                self.nodes[child].has_leading_spaces = true;
                if let Some(p) = prev {
                    self.nodes[p].has_trailing_spaces = true;
                }
            }
            if trailing > 0 {
                self.nodes[child].has_trailing_spaces = true;
                if let Some(&n) = children.get(i + 1) {
                    self.nodes[n].has_leading_spaces = true;
                }
            }
            let trimmed = trimmed.to_owned();
            let c = &mut self.nodes[child];
            c.raw_text = trimmed;
            let content_start = c.start + leading;
            let content_end = c.end - trailing;
            c.start_line = get_line(content_start, &self.line_offsets);
            c.end_line = get_line(content_end.saturating_sub(1), &self.line_offsets);
        }
        self.nodes[id].children.clone_from(&children);
        for c in children {
            self.extract_whitespace(c);
        }
    }

    fn is_dangling_space_sensitive(&self, id: Id) -> bool {
        let d = &self.nodes[id].css_display;
        !is_block_like_css_display(d) && d != "inline-block" && !self.is_script_like_tag(id)
    }

    fn is_ws_text_like(&self, id: Id) -> bool {
        matches!(
            self.nodes[id].kind,
            NodeKind::Text | NodeKind::Echo | NodeKind::RawEcho | NodeKind::TripleEcho
        )
    }

    fn leading_space_sensitive(&self, id: Id) -> bool {
        let n = &self.nodes[id];
        let result = self.leading_space_sensitive_inner(id);
        if result
            && n.prev.is_none()
            && let Some(p) = n.parent
            && IGNORE_FIRST_LF_TAGS.contains(&self.nodes[p].full_name.to_lowercase().as_str())
        {
            return is_echo_like(n.kind);
        }
        result
    }

    fn leading_space_sensitive_inner(&self, id: Id) -> bool {
        let n = &self.nodes[id];
        if matches!(n.kind, NodeKind::FrontMatter | NodeKind::DirectiveBlock) {
            return false;
        }
        if self.is_ws_text_like(id) && n.prev.is_some_and(|p| self.is_ws_text_like(p)) {
            return true;
        }
        let Some(parent) = n.parent else { return false };
        if self.nodes[parent].css_display == "none" {
            return false;
        }
        if self.is_pre_like(parent) {
            return true;
        }
        let Some(prev) = n.prev else {
            let pd = &self.nodes[parent].css_display;
            return !(self.nodes[parent].kind == NodeKind::Root
                || self.is_pre_like(id)
                || self.is_script_like_tag(parent)
                || is_block_like_css_display(pd)
                || pd == "inline-block");
        };
        !is_block_like_css_display(&self.nodes[prev].css_display)
    }

    fn trailing_space_sensitive(&self, id: Id) -> bool {
        let n = &self.nodes[id];
        if matches!(n.kind, NodeKind::FrontMatter | NodeKind::DirectiveBlock) {
            return false;
        }
        if self.is_ws_text_like(id) && n.next.is_some_and(|p| self.is_ws_text_like(p)) {
            return true;
        }
        let Some(parent) = n.parent else { return false };
        if self.nodes[parent].css_display == "none" {
            return false;
        }
        if self.is_pre_like(parent) {
            return true;
        }
        let Some(next) = n.next else {
            let pd = &self.nodes[parent].css_display;
            return !(self.nodes[parent].kind == NodeKind::Root
                || self.is_pre_like(id)
                || self.is_script_like_tag(parent)
                || is_block_like_css_display(pd)
                || pd == "inline-block");
        };
        !is_block_like_css_display(&self.nodes[next].css_display)
    }

    fn add_space_sensitivity(&mut self, id: Id) {
        let children = self.nodes[id].children.clone();
        if children.is_empty() {
            self.nodes[id].is_dangling_space_sensitive = self.is_dangling_space_sensitive(id);
            return;
        }
        for &c in &children {
            self.nodes[c].is_leading_space_sensitive = self.leading_space_sensitive(c);
            self.nodes[c].is_trailing_space_sensitive = self.trailing_space_sensitive(c);
        }
        for i in 0..children.len() {
            let c = children[i];
            if i > 0 {
                let v = self.nodes[children[i - 1]].is_trailing_space_sensitive
                    && self.nodes[c].is_leading_space_sensitive;
                self.nodes[c].is_leading_space_sensitive = v;
            }
            if i + 1 < children.len() {
                let v = self.nodes[children[i + 1]].is_leading_space_sensitive
                    && self.nodes[c].is_trailing_space_sensitive;
                self.nodes[c].is_trailing_space_sensitive = v;
            }
        }
        for c in children {
            self.add_space_sensitivity(c);
        }
    }

    fn merge_simple_element_into_text(&mut self, id: Id) {
        let mut i = 0;
        while i < self.nodes[id].children.len() {
            let children = &self.nodes[id].children;
            let child = children[i];
            if self.nodes[child].kind != NodeKind::Element {
                self.merge_simple_element_into_text(child);
                i += 1;
                continue;
            }
            let prev = (i > 0).then(|| children[i - 1]);
            let next = children.get(i + 1).copied();
            let c = &self.nodes[child];
            let simple = c.attrs.is_empty()
                && c.children.len() == 1
                && {
                    let f = &self.nodes[c.children[0]];
                    f.kind == NodeKind::Text
                        && !has_html_whitespace(&f.raw_text)
                        && !f.has_leading_spaces
                        && !f.has_trailing_spaces
                }
                && c.is_leading_space_sensitive
                && !c.has_leading_spaces
                && c.is_trailing_space_sensitive
                && !c.has_trailing_spaces
                && prev.is_some_and(|p| self.nodes[p].kind == NodeKind::Text)
                && next.is_some_and(|n| self.nodes[n].kind == NodeKind::Text);
            let (Some(p), Some(n), true) = (prev, next, simple) else {
                self.merge_simple_element_into_text(child);
                i += 1;
                continue;
            };
            let first = self.nodes[child].children[0];
            let tag = self.nodes[child].raw_tag_name.clone();
            let merged = format!(
                "<{tag}>{}</{tag}>{}",
                self.nodes[first].raw_text, self.nodes[n].raw_text
            );
            let (n_end, n_end_line, n_tss, n_hts) = {
                let nn = &self.nodes[n];
                (
                    nn.end,
                    nn.end_line,
                    nn.is_trailing_space_sensitive,
                    nn.has_trailing_spaces,
                )
            };
            let pn = &mut self.nodes[p];
            pn.raw_text.push_str(&merged);
            pn.end = n_end;
            pn.end_line = n_end_line;
            pn.is_trailing_space_sensitive = n_tss;
            pn.has_trailing_spaces = n_hts;
            self.nodes[id].children.drain(i..i + 2);
            // `i--` then `i++`: revisit the slot the removed element occupied.
        }
    }

    fn promote_opening_tag_children(&mut self, id: Id) {
        for c in self.nodes[id].children.clone() {
            self.promote_opening_tag_children(c);
        }
        let n = &self.nodes[id];
        if n.kind != NodeKind::Element || n.children.is_empty() {
            return;
        }
        let open_end = n.open_tag_end_offset;
        let start = n.start;
        let (moved, retained): (Vec<Id>, Vec<Id>) = n.children.iter().partition(|&&c| {
            let cn = &self.nodes[c];
            open_end > 0 && cn.start >= start && cn.start < open_end && cn.end <= open_end
        });
        if moved.is_empty() {
            return;
        }
        for &m in &moved {
            let c = &mut self.nodes[m];
            c.prev = None;
            c.next = None;
            c.is_leading_space_sensitive = false;
            c.is_trailing_space_sensitive = false;
            c.has_leading_spaces = false;
            c.has_trailing_spaces = false;
        }
        let mut attrs = self.nodes[id].attrs.clone();
        attrs.extend(moved);
        attrs.sort_by_key(|&a| (self.nodes[a].start, self.nodes[a].end));
        self.nodes[id].attrs = attrs;
        self.nodes[id].children = retained;
    }

    pub(crate) fn preprocess(&mut self) {
        self.remove_ignorable_first_lf(ROOT);
        self.merge_ie_conditional_comments(ROOT);
        self.add_implicit_namespaces(ROOT, "");
        self.add_siblings();
        for id in self.preorder() {
            self.nodes[id].css_display = self.css_display(id);
        }
        for id in self.preorder() {
            let n = &self.nodes[id];
            let closing = if n.kind == NodeKind::Element {
                VOID_ELEMENTS.contains(&n.full_name.to_lowercase().as_str())
                    || self.flat[n.flat].data == 1
            } else {
                n.children.is_empty()
            };
            self.nodes[id].is_self_closing = closing;
        }
        for id in self.preorder() {
            let n = &self.nodes[id];
            if n.kind == NodeKind::Element && n.has_closing_tag {
                let end = n.end.min(self.source.len());
                // `lastIndexOf("</", end)` may match a "</" that starts at `end`.
                let mut hay_end = (end + 2).min(self.source.len());
                while !self.source.is_char_boundary(hay_end) {
                    hay_end -= 1;
                }
                if let Some(cs) = self.source[..hay_end].rfind("</") {
                    let closing = if cs <= n.end {
                        &self.source[cs..n.end.max(cs)]
                    } else {
                        ""
                    };
                    let htm = is_htm_closing(closing);
                    self.nodes[id].has_htm_component_closing_tag = htm;
                }
            }
        }
        for id in self.preorder() {
            let ws = self.is_script_like_tag(id)
                || is_echo_like(self.nodes[id].kind)
                || self.is_pre_like(id);
            let pre = self.is_pre_like(id);
            let n = &mut self.nodes[id];
            n.is_whitespace_sensitive = ws;
            n.is_indentation_sensitive = pre;
        }
        self.extract_whitespace(ROOT);
        self.add_siblings();
        self.add_space_sensitivity(ROOT);
        self.merge_simple_element_into_text(ROOT);
        self.add_siblings();
        self.promote_opening_tag_children(ROOT);
        self.add_siblings();
        self.add_space_sensitivity(ROOT);
    }
}

/// `^<\s*\/\s*\/\s*>$`
fn is_htm_closing(s: &str) -> bool {
    let rest = s.strip_prefix('<').map(js_trim_start_ws);
    let rest = rest.and_then(|r| r.strip_prefix('/')).map(js_trim_start_ws);
    let rest = rest.and_then(|r| r.strip_prefix('/')).map(js_trim_start_ws);
    rest == Some(">")
}

pub(crate) fn is_echo_like(kind: NodeKind) -> bool {
    matches!(
        kind,
        NodeKind::Echo | NodeKind::RawEcho | NodeKind::TripleEcho
    )
}

/// `/^x-slot(?::|\[|$)/iu`
fn is_blade_slot_name(full_name: &str) -> bool {
    let b = full_name.as_bytes();
    b.len() >= 6
        && b[..6].eq_ignore_ascii_case(b"x-slot")
        && (b.len() == 6 || b[6] == b':' || b[6] == b'[')
}

/// `/^<!--\s*display:\s*([a-z]+)\s*-->$/`
fn display_hint(raw: &str) -> Option<String> {
    let inner = raw.strip_prefix("<!--")?.strip_suffix("-->")?;
    let inner = crate::tree::js_trim_start(inner).strip_prefix("display:")?;
    let inner = crate::tree::js_trim(inner);
    (!inner.is_empty() && inner.bytes().all(|b| b.is_ascii_lowercase())).then(|| inner.to_owned())
}
