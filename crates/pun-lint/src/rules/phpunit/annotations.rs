use mago_syntax::cst::{
    AttributeList, LiteralStringKind, Namespace, NamespaceBody, Node, Property, Sequence,
    Statement, UseItem, UseItems,
};
use pun_problem::{Edit, Span};

use crate::Context;
use crate::rules::lint_b_doc::Doc;
use crate::rules::prefer_absolute_include::print_string;
use crate::rules::rector_stmts::{line_indent, previous_token_end, tokens};
use crate::rules::types::facts::facts;
use crate::syntax::node_span;

pub(crate) struct Annotated<'s> {
    pub start: usize,
    pub last_attribute_end: Option<usize>,
    pub doc: Option<Doc<'s>>,
    other_comments: bool,
    comments_from: usize,
}

impl<'s> Annotated<'s> {
    pub(crate) fn of(
        ctx: &Context<'s>,
        node: Node<'_, '_>,
        ancestors: &[Node<'_, '_>],
    ) -> Option<Self> {
        let lists: &Sequence<'_, AttributeList<'_>> = match node {
            Node::Class(class) => &class.attribute_lists,
            Node::Method(method) => &method.attribute_lists,
            Node::Function(function) => &function.attribute_lists,
            Node::Property(property) => return Some(Self::property(ctx, property, ancestors)),
            Node::Interface(interface) => &interface.attribute_lists,
            Node::FunctionLikeParameter(parameter) => &parameter.attribute_lists,
            _ => return None,
        };
        Some(Self::from_attributes(
            ctx,
            node_span(&node).start,
            lists,
            ancestors,
        ))
    }

    pub(crate) fn property(
        ctx: &Context<'s>,
        property: &Property<'_>,
        ancestors: &[Node<'_, '_>],
    ) -> Self {
        let lists = match property {
            Property::Plain(property) => &property.attribute_lists,
            Property::Hooked(property) => &property.attribute_lists,
        };
        Self::from_attributes(
            ctx,
            node_span(&Node::Property(property)).start,
            lists,
            ancestors,
        )
    }

    fn from_attributes(
        ctx: &Context<'s>,
        start: usize,
        lists: &Sequence<'_, AttributeList<'_>>,
        ancestors: &[Node<'_, '_>],
    ) -> Self {
        let source = ctx.source;
        let doc = Doc::of(ctx, ancestors, start);
        let outer = ancestors
            .iter()
            .rev()
            .map(|a| node_span(a).start)
            .find(|&s| s < start)
            .unwrap_or(0);
        let comments_from = previous_token_end(ctx, outer, start);
        let other_comments = doc
            .as_ref()
            .is_some_and(|d| !source[comments_from..d.span.start].trim().is_empty());
        Annotated {
            start,
            last_attribute_end: lists.last().map(|l| node_span(l).end),
            doc,
            other_comments,
            comments_from,
        }
    }

    pub(crate) fn of_last(
        ctx: &Context<'s>,
        node: Node<'_, '_>,
        ancestors: &[Node<'_, '_>],
    ) -> Option<Self> {
        let mut annotated = Self::of(ctx, node, ancestors)?;
        if annotated.doc.is_none() {
            annotated.doc = Doc::last_of(ctx, ancestors, annotated.start);
            annotated.other_comments = annotated.doc.is_some();
        }
        Some(annotated)
    }

    pub(crate) fn edits(
        &self,
        source: &str,
        new_doc: Option<&str>,
        groups: &[String],
    ) -> Option<Vec<Edit>> {
        let indent = line_indent(source, self.start);
        if indent.contains('\t') {
            return None;
        }
        self.edits_at(source, new_doc, groups, &format!("\n{indent}"))
    }

    pub(crate) fn edits_at(
        &self,
        source: &str,
        new_doc: Option<&str>,
        groups: &[String],
        nl: &str,
    ) -> Option<Vec<Edit>> {
        let mut edits = Vec::new();
        let mut at_start = self.start;
        let mut lead = String::new();
        if let (Some(doc), Some(new_doc)) = (self.doc.as_ref().map(|d| d.span), new_doc) {
            if self.other_comments {
                return None;
            }
            if new_doc.is_empty() {
                let after =
                    doc.end + source[doc.end..].len() - source[doc.end..].trim_start().len();
                if after == self.start && self.last_attribute_end.is_none() && !groups.is_empty() {
                    at_start = doc.start;
                } else {
                    edits.push(Edit::new(Span::new(doc.start, after), ""));
                }
            } else {
                lead = format!("{}{nl}", reformatted(new_doc).replace('\n', nl));
                at_start = doc.start;
            }
        } else if let Some(new_doc) = new_doc.filter(|d| !d.is_empty() && self.doc.is_none()) {
            lead = format!("{}{nl}", reformatted(new_doc).replace('\n', nl));
        }
        match self.last_attribute_end {
            Some(end) if !groups.is_empty() => {
                let text: String = groups.iter().map(|g| format!("{nl}{g}")).collect();
                edits.push(Edit::new(Span::new(end, end), text));
            }
            Some(_) => {}
            None => lead.extend(groups.iter().map(|g| format!("{g}{nl}"))),
        }
        if !lead.is_empty() {
            edits.push(Edit::new(Span::new(at_start, self.start), lead));
        }
        Some(edits)
    }

    pub(crate) fn edit_with_comments(&self, source: &str, new_doc: &str) -> Option<Edit> {
        let doc = self.doc.as_ref().filter(|_| self.other_comments)?.span;
        let indent = line_indent(source, self.start);
        if indent.contains('\t') {
            return None;
        }
        let nl = format!("\n{indent}");
        let mut comments: Vec<String> = Vec::new();
        for token in tokens(source, self.comments_from, self.start) {
            if !token.kind.is_comment() {
                continue;
            }
            let text = if token.start == doc.start {
                new_doc
            } else {
                source[token.start..token.end].trim_end_matches(['\n', '\r'])
            };
            if !text.is_empty() {
                comments.push(reformatted(text).replace('\n', &nl));
            }
        }
        let start = self.comments_from + source[self.comments_from..].len()
            - source[self.comments_from..].trim_start().len();
        Some(Edit::new(
            Span::new(start, self.start),
            format!("{}{nl}", comments.join(&nl)),
        ))
    }
}

pub(crate) fn reformatted(text: &str) -> String {
    let text = &text.replace("\r\n", "\n");
    let Some((first, _)) = text.split_once('\n') else {
        return text.clone();
    };
    if starred(text) {
        let b = text.as_bytes();
        let mut out = Vec::with_capacity(b.len());
        let mut i = 0;
        while i < b.len() {
            if i == 0 || b[i - 1] == b'\n' {
                let end = i + b[i..].iter().take_while(|c| is_pcre_space(**c)).count();
                if end > i && b.get(end) == Some(&b'*') {
                    out.extend_from_slice(b" *");
                    i = end + 1;
                    continue;
                }
            }
            out.push(b[i]);
            i += 1;
        }
        return String::from_utf8(out).unwrap();
    }
    let opens_alone = first
        .strip_prefix("/*")
        .map(|r| r.strip_prefix('*').unwrap_or(r))
        .is_some_and(|r| r.bytes().all(is_pcre_space));
    let body = text.strip_suffix("*/").unwrap_or(text);
    let run = &body[body
        .trim_end_matches(|c: char| c.is_ascii() && is_pcre_space(c as u8))
        .len()..];
    if let (true, Some(at)) = (opens_alone, run.find('\n')) {
        let prefix = &run.as_bytes()[at + 1..];
        return strip_line_starts(text, |rest| {
            if rest.starts_with(prefix) {
                prefix.len()
            } else {
                0
            }
        });
    }
    let b = text.as_bytes();
    let opening = 2 + usize::from(b.get(2) == Some(&b'*'));
    let opening = opening
        + b[opening..]
            .iter()
            .take_while(|c| is_pcre_space(**c))
            .count();
    let shortest = text[first.len() + 1..]
        .split('\n')
        .map(|line| line.bytes().take_while(|c| is_pcre_space(*c)).count())
        .min()
        .unwrap();
    let remove = shortest.saturating_sub(opening);
    strip_line_starts(text, |rest| {
        if remove > 0 && rest.len() >= remove && rest[..remove].iter().all(|c| is_pcre_space(*c)) {
            remove
        } else {
            0
        }
    })
}

fn strip_line_starts(text: &str, matched: impl Fn(&[u8]) -> usize) -> String {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if i == 0 || b[i - 1] == b'\n' {
            let n = matched(&b[i..]);
            if n > 0 {
                i += n;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap()
}

fn is_pcre_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C)
}

fn starred(text: &str) -> bool {
    let b = text.as_bytes();
    let mut i = text.find('\n').unwrap();
    loop {
        let start = i + 1;
        let star = start + b[start..].iter().take_while(|c| is_pcre_space(**c)).count();
        if star == start || b.get(star) != Some(&b'*') {
            return false;
        }
        match b[star..].iter().position(|c| *c == b'\n') {
            Some(at) => i = star + at,
            None => return true,
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum TagValue {
    Generic(String),
    Annotation(String),
    Parenthesized(String),
}

const TYPED_TAGS: &[&str] = &[
    "param",
    "phpstan-param",
    "psalm-param",
    "phan-param",
    "param-immediately-invoked-callable",
    "phpstan-param-immediately-invoked-callable",
    "param-later-invoked-callable",
    "phpstan-param-later-invoked-callable",
    "param-closure-this",
    "phpstan-param-closure-this",
    "pure-unless-callable-is-impure",
    "phpstan-pure-unless-callable-is-impure",
    "pure-unless-parameter-passed",
    "phpstan-pure-unless-parameter-passed",
    "var",
    "phpstan-var",
    "psalm-var",
    "phan-var",
    "return",
    "phpstan-return",
    "psalm-return",
    "phan-return",
    "phan-real-return",
    "throws",
    "phpstan-throws",
    "mixin",
    "phan-mixin",
    "psalm-require-extends",
    "phpstan-require-extends",
    "psalm-require-implements",
    "phpstan-require-implements",
    "psalm-inheritors",
    "phpstan-sealed",
    "deprecated",
    "property",
    "property-read",
    "property-write",
    "phpstan-property",
    "phpstan-property-read",
    "phpstan-property-write",
    "psalm-property",
    "psalm-property-read",
    "psalm-property-write",
    "phan-property",
    "phan-property-read",
    "phan-property-write",
    "method",
    "phpstan-method",
    "psalm-method",
    "phan-method",
    "template",
    "phpstan-template",
    "psalm-template",
    "phan-template",
    "template-covariant",
    "phpstan-template-covariant",
    "psalm-template-covariant",
    "template-contravariant",
    "phpstan-template-contravariant",
    "psalm-template-contravariant",
    "extends",
    "phpstan-extends",
    "phan-extends",
    "phan-inherits",
    "template-extends",
    "implements",
    "phpstan-implements",
    "template-implements",
    "use",
    "phpstan-use",
    "template-use",
    "phpstan-type",
    "psalm-type",
    "phan-type",
    "phpstan-import-type",
    "psalm-import-type",
    "phpstan-assert",
    "phpstan-assert-if-true",
    "phpstan-assert-if-false",
    "psalm-assert",
    "psalm-assert-if-true",
    "psalm-assert-if-false",
    "phan-assert",
    "phan-assert-if-true",
    "phan-assert-if-false",
    "phpstan-this-out",
    "phpstan-self-out",
    "psalm-this-out",
    "psalm-self-out",
    "param-out",
    "phpstan-param-out",
    "psalm-param-out",
];

pub(crate) fn named_tags(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    doc: &Doc<'_>,
    name: &str,
) -> Option<Vec<(usize, TagValue)>> {
    let tokens = phpdoc_tokens(doc.text_all());
    if tokens
        .iter()
        .any(|t| !t.starts_with(['\r', '\n']) && t.contains('\n'))
        || tokens.iter().map(|t| t.len()).sum::<usize>() < doc.text_all().len()
    {
        return None;
    }
    let mut generic: Vec<(usize, &str, String)> = Vec::new();
    let mut tags = Vec::new();
    for i in 0..doc.len() {
        let text = doc.text(i);
        if text.contains("@\\") || text.starts_with("@_") {
            return None;
        }
        let Some(tag) = tag_name(text) else {
            if text.contains("})") {
                return None;
            }
            continue;
        };
        if TYPED_TAGS.contains(&tag) {
            continue;
        }
        let rest = &text[1 + tag.len()..];
        if rest.trim_start_matches([' ', '\t']).starts_with('(') {
            if tag == name {
                tags.push((i, TagValue::Parenthesized(rest.to_string())));
            }
            continue;
        }
        let inline_tag = rest.as_bytes().windows(3).any(|w| {
            matches!(w[0], b' ' | b'\t')
                && w[1] == b'@'
                && (w[2].is_ascii_alphabetic() || w[2] == b'_' || w[2] == b'\\')
        });
        if inline_tag {
            return None;
        }
        let value = generic_tag_value(text, tag);
        if tag == name {
            tags.push((i, TagValue::Generic(value.clone())));
        }
        generic.push((i, tag, value));
    }
    if generic
        .iter()
        .rev()
        .skip(1)
        .any(|(_, _, value)| !closed_content(value))
    {
        return None;
    }
    if !tags.is_empty() && is_annotation_class(ctx, ancestors, name) {
        for (_, value) in &mut tags {
            if let TagValue::Generic(v) = value {
                *value = TagValue::Annotation(std::mem::take(v));
            }
        }
    }
    Some(tags)
}

fn tag_name(text: &str) -> Option<&str> {
    let b = text.as_bytes();
    if b.first() != Some(&b'@') || !b.get(1).is_some_and(u8::is_ascii_alphabetic) {
        return None;
    }
    let part = |b: &[u8], from: usize| {
        from + b[from..]
            .iter()
            .take_while(|c| c.is_ascii_alphanumeric() || **c == b'-' || **c == b'\\')
            .count()
    };
    let end = part(b, 2);
    let end = if end > 2
        && b.get(end) == Some(&b':')
        && b.get(end + 1).is_some_and(u8::is_ascii_alphabetic)
    {
        part(b, end + 2)
    } else {
        end
    };
    Some(&text[1..end])
}

fn is_annotation_class(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], tag: &str) -> bool {
    let namespace = ancestors.iter().rev().find_map(|a| match a {
        Node::Namespace(ns) => Some(
            ns.name
                .as_ref()
                .map(|n| String::from_utf8_lossy(n.value()).into_owned()),
        ),
        _ => None,
    });
    let uses = file_uses(ancestors);
    let resolved = match namespace.flatten() {
        Some(ns) => {
            let namespaced = format!("{ns}\\{tag}");
            if facts(ctx)
                .codebase
                .get_class_like(namespaced.as_bytes())
                .is_some()
            {
                Some(namespaced)
            } else {
                uses.iter()
                    .find(|u| u.alias.as_deref() == Some(tag))
                    .map(|u| u.name.clone())
                    .or_else(|| match_name_with_uses(&uses, tag))
            }
        }
        None => match_name_with_uses(&uses, tag),
    };
    let resolved = resolved.unwrap_or_else(|| tag.to_string());
    resolved.contains('\\') || resolved == "Target"
}

struct UseImport {
    name: String,
    alias: Option<String>,
}

fn file_uses(ancestors: &[Node<'_, '_>]) -> Vec<UseImport> {
    let statements: Vec<&Statement<'_>> = ancestors
        .iter()
        .filter_map(|a| match a {
            Node::Program(program) => Some(*program),
            _ => None,
        })
        .flat_map(|program| {
            let namespaces: Vec<&Namespace<'_>> = program
                .statements
                .iter()
                .filter_map(|s| match s {
                    Statement::Namespace(ns) => Some(ns),
                    _ => None,
                })
                .collect();
            let statements: Vec<&Statement<'_>> = match namespaces.as_slice() {
                [ns] => match &ns.body {
                    NamespaceBody::Implicit(body) => body.statements.iter().collect(),
                    NamespaceBody::BraceDelimited(block) => block.statements.iter().collect(),
                },
                _ => program.statements.iter().collect(),
            };
            statements
        })
        .collect();
    let mut imports = Vec::new();
    let text = |id: &mago_syntax::cst::Identifier<'_>| {
        String::from_utf8_lossy(id.value())
            .trim_start_matches('\\')
            .to_string()
    };
    let mut add = |prefix: Option<String>, item: &UseItem<'_>| {
        let name = text(&item.name);
        imports.push(UseImport {
            name: match prefix {
                Some(prefix) => format!("{prefix}\\{name}"),
                None => name,
            },
            alias: item
                .alias
                .as_ref()
                .map(|a| String::from_utf8_lossy(a.identifier.value).into_owned()),
        });
    };
    for statement in statements {
        let Statement::Use(r#use) = statement else {
            continue;
        };
        match &r#use.items {
            UseItems::Sequence(s) => s.items.iter().for_each(|i| add(None, i)),
            UseItems::TypedSequence(s) => s.items.iter().for_each(|i| add(None, i)),
            UseItems::TypedList(l) => l
                .items
                .iter()
                .for_each(|i| add(Some(text(&l.namespace)), i)),
            UseItems::MixedList(l) => l
                .items
                .iter()
                .for_each(|i| add(Some(text(&l.namespace)), &i.item)),
        }
    }
    imports
}

fn match_name_with_uses(uses: &[UseImport], tag: &str) -> Option<String> {
    uses.iter().find_map(|u| {
        let short = u
            .alias
            .as_deref()
            .unwrap_or_else(|| u.name.rsplit('\\').next().unwrap_or_default());
        short.eq_ignore_ascii_case(tag).then(|| match u.alias {
            Some(_) => format!("{}\\", u.name),
            None => u.name.clone(),
        })
    })
}

fn closed_content(content: &str) -> bool {
    let (mut open, mut close) = (0, 0);
    for token in phpdoc_tokens(content) {
        open += usize::from(token.contains(['(', '{']));
        close += usize::from(token.contains([')', '}']));
    }
    open == close
}

fn phpdoc_tokens(s: &str) -> Vec<&str> {
    let b = s.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let Some(len) = token_len(b, i) else {
            break;
        };
        tokens.push(&s[i..i + len]);
        i += len;
    }
    tokens
}

fn token_len(b: &[u8], i: usize) -> Option<usize> {
    let at = |j: usize| b.get(j).copied();
    let start =
        |c: Option<u8>| c.is_some_and(|c| c.is_ascii_alphabetic() || c == b'_' || c >= 0x80);
    let word = |c: Option<u8>| start(c) || c.is_some_and(|c| c.is_ascii_digit());
    let run = |from: usize, f: &dyn Fn(Option<u8>) -> bool| {
        let mut j = from;
        while f(at(j)) {
            j += 1;
        }
        j
    };
    let rest = &b[i..];
    let ws = run(i, &|c| matches!(c, Some(b' ' | b'\t')));
    if ws > i {
        return Some(ws - i);
    }
    let mut j = i;
    loop {
        let k = j + usize::from(at(j) == Some(b'\\'));
        if !start(at(k)) {
            break;
        }
        j = run(k + 1, &|c| word(c) || c == Some(b'-'));
    }
    if j > i {
        return Some(j - i);
    }
    if rest.len() >= 5 && rest[..5].eq_ignore_ascii_case(b"$this") && !word(at(i + 5)) {
        return Some(5);
    }
    if at(i) == Some(b'$') && start(at(i + 1)) {
        return Some(run(i + 2, &word) - i);
    }
    if matches!(
        b[i],
        b'&' | b'|' | b'?' | b'!' | b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b','
    ) {
        return Some(1);
    }
    if rest.starts_with(b"//") {
        let eol = rest.iter().position(|c| matches!(c, b'\r' | b'\n'));
        let end = match eol {
            Some(eol) => Some(eol),
            None => rest[2..]
                .windows(2)
                .rposition(|w| w == b"*/")
                .map(|p| p + 2),
        };
        if let Some(end) = end {
            return Some(end);
        }
    }
    for op in [&b"..."[..], b"::", b"=>", b"->", b"=", b":"] {
        if rest.starts_with(op) {
            return Some(op.len());
        }
    }
    if rest.starts_with(b"/**") && at(i + 3).is_some_and(|c| c.is_ascii_whitespace() || c == 0x0b) {
        return Some(3 + usize::from(at(i + 3) == Some(b' ')));
    }
    if rest.starts_with(b"*/") {
        return Some(2);
    }
    if b[i] == b'@' {
        if let Some(name) = tag_name(std::str::from_utf8(rest).ok()?) {
            return Some(1 + name.len());
        }
        if matches!(at(i + 1), Some(b'_' | b'\\')) {
            let end = run(i + 2, &|c| {
                c.is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'\\'))
            });
            if let Some(k) = (i + 2..end)
                .rev()
                .find(|&k| b[k].is_ascii_alphabetic() || b[k] == b'_')
            {
                return Some(
                    run(k + 1, &|c| {
                        c.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_')
                    }) - i,
                );
            }
        }
    }
    let nl = i + usize::from(at(i) == Some(b'\r'));
    if at(nl) == Some(b'\n') {
        let mut j = run(nl + 1, &|c| matches!(c, Some(b' ' | b'\t')));
        if at(j) == Some(b'*') && at(j + 1) != Some(b'/') {
            j += 1 + usize::from(at(j + 1) == Some(b' '));
        }
        return Some(j - i);
    }
    if let Some(end) = number(b, i) {
        return Some(end - i);
    }
    if let quote @ (b'\'' | b'"') = b[i] {
        let mut j = i + 1;
        loop {
            match at(j) {
                Some(b'\\') if !matches!(at(j + 1), None | Some(b'\r' | b'\n')) => j += 2,
                Some(c) if c == quote => return Some(j + 1 - i),
                Some(b'\\' | b'\r' | b'\n') | None => break,
                Some(_) => j += 1,
            }
        }
    }
    if b[i] == b'"' {
        let mut j = i + 1;
        loop {
            match at(j) {
                Some(b'"') if at(j + 1) == Some(b'"') => j += 2,
                Some(b'"') => return Some(j + 1 - i),
                Some(_) => j += 1,
                None => break,
            }
        }
    }
    if b[i] == b'*' {
        return Some(1);
    }
    let mut j = i;
    while j < b.len() && !b[j].is_ascii_whitespace() && b[j] != 0x0b && !b[j..].starts_with(b"*/") {
        j += 1;
    }
    (j > i).then_some(j - i)
}

fn number(b: &[u8], i: usize) -> Option<usize> {
    let at = |j: usize| b.get(j).copied();
    let digits = |from: usize, digit: &dyn Fn(u8) -> bool| -> Option<usize> {
        let mut j = from;
        while at(j).is_some_and(digit) {
            j += 1;
        }
        if j == from {
            return None;
        }
        while at(j) == Some(b'_') && at(j + 1).is_some_and(digit) {
            j += 1;
            while at(j).is_some_and(digit) {
                j += 1;
            }
        }
        Some(j)
    };
    let dec = |c: u8| c.is_ascii_digit();
    let exponent = |from: usize| -> Option<usize> {
        if !matches!(at(from), Some(b'e' | b'E')) {
            return None;
        }
        let sign = from + 1 + usize::from(matches!(at(from + 1), Some(b'+' | b'-')));
        digits(sign, &dec)
    };
    let s = i + usize::from(matches!(at(i), Some(b'+' | b'-')));
    if let Some(d) = digits(s, &dec)
        && at(d) == Some(b'.')
    {
        let mut j = d + 1;
        while at(j).is_some_and(dec) {
            j += 1;
        }
        while at(j) == Some(b'_') && at(j + 1).is_some_and(dec) {
            j = digits(j + 1, &dec)?;
        }
        return Some(exponent(j).unwrap_or(j));
    }
    let mut j = s;
    while at(j).is_some_and(dec) {
        j += 1;
    }
    while at(j) == Some(b'_') && at(j + 1).is_some_and(dec) {
        j = digits(j + 1, &dec)?;
    }
    if at(j) == Some(b'.')
        && let Some(d) = digits(j + 1, &dec)
    {
        return Some(exponent(d).unwrap_or(d));
    }
    if let Some(d) = digits(s, &dec)
        && let Some(e) = exponent(d)
    {
        return Some(e);
    }
    if at(s) == Some(b'0') {
        let based = match at(s + 1) {
            Some(b'b' | b'B') => digits(s + 2, &|c| matches!(c, b'0' | b'1')),
            Some(b'o' | b'O') => digits(s + 2, &|c| (b'0'..=b'7').contains(&c)),
            Some(b'x' | b'X') => digits(s + 2, &|c| c.is_ascii_hexdigit()),
            _ => None,
        };
        if based.is_some() {
            return based;
        }
    }
    digits(s, &dec)
}

pub(crate) fn generic_tag_value(tag: &str, name: &str) -> String {
    let rest = &tag[1 + name.len()..];
    let mut lines = rest.split('\n');
    let mut value = vec![
        lines
            .next()
            .unwrap_or_default()
            .trim_start()
            .trim_end_matches('\r')
            .to_string(),
    ];
    for line in lines {
        let line = line.trim_end_matches('\r').trim_start();
        let line = line.strip_prefix('*').unwrap_or(line);
        if !line.trim().is_empty() {
            value.push(line.strip_prefix(' ').unwrap_or(line).to_string());
        }
    }
    value.join("\n").trim_end().to_string()
}

pub(crate) fn map_item(value: &str) -> String {
    if is_class_constant_item(value) {
        return value.to_string();
    }
    if value.contains("::") && !value.starts_with('"') {
        return print_str(value);
    }
    let value = if value.starts_with('"') && value.ends_with('"') {
        value.trim_matches('"')
    } else {
        value
    };
    print_str(value)
}

pub(crate) fn is_class_constant_item(value: &str) -> bool {
    if value.starts_with('"') {
        return false;
    }
    let parts: Vec<&str> = value.split("::").collect();
    matches!(parts[..], [class, constant]
        if !class.is_empty()
            && is_class_name(class.trim_start_matches('\\'))
            && is_constant_name(constant))
}

fn print_str(value: &str) -> String {
    let kind = if value.contains('\'') && !value.contains('\n') {
        LiteralStringKind::DoubleQuoted
    } else {
        LiteralStringKind::SingleQuoted
    };
    print_string(kind, value.as_bytes()).unwrap_or_default()
}

fn is_class_name(name: &str) -> bool {
    name.split('\\').all(|part| is_name(part, 0x7f))
}

fn is_constant_name(name: &str) -> bool {
    is_name(name, 0x80)
}

fn is_name(name: &str, high: u8) -> bool {
    let b = name.as_bytes();
    let letter = |c: u8| c.is_ascii_alphabetic() || c == b'_' || c >= high;
    !b.is_empty() && letter(b[0]) && b[1..].iter().all(|&c| letter(c) || c.is_ascii_digit())
}
