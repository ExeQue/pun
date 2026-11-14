use mago_syntax::cst::Node;
use mago_syntax::token::TokenKind;
use pun_problem::{Edit, Span};

use super::rector_batch5::doc_comment;
use super::rector_stmts::{previous_token_end, tokens};
use crate::Context;
use crate::syntax::node_span;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Open,
    Close,
    Eol,
    Ws,
    Other,
    End,
}

pub(crate) struct Doc<'s> {
    pub span: Span,
    text: &'s str,
    tokens: Vec<(Kind, usize, usize)>,
    children: Vec<(usize, usize)>,
}

impl<'s> Doc<'s> {
    pub(crate) fn of(ctx: &Context<'s>, ancestors: &[Node<'_, '_>], start: usize) -> Option<Self> {
        let source = ctx.source;
        let outer = ancestors
            .iter()
            .rev()
            .map(|a| node_span(a).start)
            .find(|&s| s < start)
            .unwrap_or(0);
        Self::at(
            source,
            doc_comment(source, previous_token_end(ctx, outer, start), start)?,
        )
    }

    pub(crate) fn last_of(
        ctx: &Context<'s>,
        ancestors: &[Node<'_, '_>],
        start: usize,
    ) -> Option<Self> {
        let source = ctx.source;
        let outer = ancestors
            .iter()
            .rev()
            .map(|a| node_span(a).start)
            .find(|&s| s < start)
            .unwrap_or(0);
        let doc = tokens(source, previous_token_end(ctx, outer, start), start)
            .into_iter()
            .rfind(|t| t.kind == TokenKind::DocBlockComment)?;
        Self::at(source, Span::new(doc.start, doc.end))
    }

    pub(crate) fn parse(text: &'s str) -> Option<Self> {
        Self::at(text, Span::new(0, text.len()))
    }

    fn at(source: &'s str, span: Span) -> Option<Self> {
        let text = &source[span.start..span.end];
        if !text
            .as_bytes()
            .get(3)
            .is_some_and(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        {
            return None;
        }
        let tokens = lex(text);
        let children = children(text, &tokens);
        Some(Doc {
            span,
            text,
            tokens,
            children,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.children.len()
    }

    pub(crate) fn words(&self, i: usize) -> Vec<&'s str> {
        let (from, to) = self.children[i];
        self.tokens[from..to]
            .iter()
            .filter(|t| t.0 == Kind::Other)
            .map(|t| &self.text[t.1..t.2])
            .collect()
    }

    pub(crate) fn remove(&self, removed: &[bool], node_start: usize) -> Edit {
        let printed = self.print(removed);
        if printed.is_empty() {
            return Edit::new(Span::new(self.span.start, node_start), "");
        }
        Edit::new(self.span, printed)
    }

    pub(crate) fn remove_last(&self, source: &str, removed: &[bool]) -> Edit {
        let after = &source[self.span.end..];
        self.remove(
            removed,
            self.span.end + after.len() - after.trim_start().len(),
        )
    }

    pub(crate) fn text_all(&self) -> &'s str {
        self.text
    }

    pub(crate) fn text(&self, i: usize) -> &'s str {
        let (from, to) = self.children[i];
        if from >= to {
            return "";
        }
        &self.text[self.tokens[from].1..self.tokens[to - 1].2]
    }

    pub(crate) fn print(&self, removed: &[bool]) -> String {
        self.print_appending(removed, &[])
    }

    pub(crate) fn print_appending(&self, removed: &[bool], appended: &[&str]) -> String {
        let empty = |i: usize| self.children[i].0 >= self.children[i].1;
        let mut kept: Vec<usize> = (0..self.children.len()).filter(|&i| !removed[i]).collect();
        if appended.is_empty() && kept.iter().all(|&i| empty(i)) {
            return String::new();
        }
        if !appended.is_empty() && !self.tokens.iter().any(|t| t.0 == Kind::Eol) {
            let line_start = |s: &str| {
                let mut out = String::new();
                let mut rest = s;
                while let Some(at) = rest.find('\n') {
                    out.push_str(&rest[..at]);
                    rest = &rest[at + 1..];
                    out.push_str(
                        if matches!(rest.as_bytes().first(), None | Some(b'\r' | b'\n')) {
                            "\n *"
                        } else {
                            "\n * "
                        },
                    );
                }
                out.push_str(rest);
                out
            };
            let parts: Vec<String> = kept
                .iter()
                .map(|&i| line_start(self.text(i)))
                .chain(appended.iter().map(std::string::ToString::to_string))
                .map(|s| if s.is_empty() { s } else { format!(" {s}") })
                .collect();
            return format!("/**\n *{}\n */", parts.join("\n *"));
        }
        if appended.is_empty()
            && self
                .children
                .last()
                .is_some_and(|_| !empty(self.children.len() - 1))
        {
            while kept.last().is_some_and(|&i| empty(i)) {
                kept.pop();
            }
        }
        let content = |from: usize, to: usize| -> &str {
            if from >= to {
                return "";
            }
            &self.text[self.tokens[from].1..self.tokens[to].1]
        };
        enum Elem<'a> {
            Keep(usize),
            Remove(usize),
            Replace(usize, &'a str),
            Add(&'a str),
        }
        let mut diff: Vec<Elem<'_>> = (0..self.children.len())
            .map(|i| {
                if kept.contains(&i) {
                    Elem::Keep(i)
                } else {
                    Elem::Remove(i)
                }
            })
            .collect();
        let trailing = diff
            .iter()
            .rev()
            .take_while(|e| matches!(e, Elem::Remove(_)))
            .count();
        if trailing > 0 && trailing == appended.len() {
            let first = diff.len() - trailing;
            for (n, tag) in appended.iter().enumerate() {
                diff[first + n] = Elem::Replace(first + n, tag);
            }
        } else {
            diff.extend(appended.iter().map(|a| Elem::Add(a)));
        }
        let insert = (0..self.children.len())
            .rev()
            .find_map(|i| {
                let mut index = self.children[i].0.checked_sub(1)?;
                let mut line_start = String::new();
                while self.tokens[index].0 == Kind::Ws {
                    let t = self.tokens[index];
                    line_start.insert_str(0, &self.text[t.1..t.2]);
                    index = index.checked_sub(1)?;
                }
                let t = self.tokens[index];
                if t.0 != Kind::Eol {
                    return None;
                }
                line_start.insert_str(0, &self.text[t.1..t.2]);
                let line = &line_start[line_start.rfind('\n')? + 1..];
                let star = line.find('*')?;
                let (before, after) = (&line[..star], &line[star + 1..]);
                (before.bytes().all(|b| matches!(b, b' ' | b'\t'))
                    && after.bytes().all(|b| matches!(b, b' ' | b'\t')))
                .then(|| format!("\n{before}*{after}"))
            })
            .unwrap_or_else(|| "\n * ".to_string());
        let mut output = String::new();
        let mut index = 0;
        let mut delayed: Vec<&str> = Vec::new();
        let mut before_first_keep = true;
        for (position, elem) in diff.iter().enumerate() {
            match *elem {
                Elem::Keep(i) | Elem::Replace(i, _) => {
                    before_first_keep = false;
                    let (start, end) = self.children[i];
                    output.push_str(content(index, start));
                    for tag in delayed.drain(..) {
                        output.push_str(tag);
                        output.push_str(&insert);
                    }
                    match *elem {
                        Elem::Replace(_, tag) => output.push_str(tag),
                        _ => output.push_str(content(start, end.max(start))),
                    }
                    index = end;
                }
                Elem::Add(tag) if before_first_keep => delayed.push(tag),
                Elem::Add(tag) => {
                    output.push_str(&insert);
                    output.push_str(tag);
                }
                Elem::Remove(i) => {
                    let (start, end) = self.children[i];
                    if position == 0 {
                        let stop = (index..start)
                            .find(|&t| self.tokens[t].0 == Kind::Eol)
                            .unwrap_or(start);
                        output.push_str(content(index, stop));
                    }
                    index = end;
                }
            }
        }
        if !delayed.is_empty() {
            let parts: Vec<String> = delayed.iter().map(|a| format!(" {a}")).collect();
            return format!("/**\n *{}\n */", parts.join("\n *"));
        }
        output.push_str(content(index, self.tokens.len() - 1));
        output.replace(" \r\n", "\r\n").replace(" \n", "\n")
    }
}

fn lex(text: &str) -> Vec<(Kind, usize, usize)> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 3 + usize::from(bytes.get(3) == Some(&b' '));
    tokens.push((Kind::Open, 0, i));
    while i < bytes.len() {
        let start = i;
        let kind = match bytes[i] {
            b'\r' | b'\n' => {
                i += if bytes[i] == b'\r' { 2 } else { 1 };
                while matches!(bytes.get(i), Some(b' ' | b'\t')) {
                    i += 1;
                }
                if bytes.get(i) == Some(&b'*') && bytes.get(i + 1) != Some(&b'/') {
                    i += 1 + usize::from(bytes.get(i + 1) == Some(&b' '));
                }
                Kind::Eol
            }
            b' ' | b'\t' => {
                while matches!(bytes.get(i), Some(b' ' | b'\t')) {
                    i += 1;
                }
                Kind::Ws
            }
            b'*' if bytes.get(i + 1) == Some(&b'/') => {
                i += 2;
                Kind::Close
            }
            _ => {
                while i < bytes.len()
                    && !matches!(bytes[i], b' ' | b'\t' | b'\r' | b'\n')
                    && !(bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/'))
                {
                    i += 1;
                }
                Kind::Other
            }
        };
        tokens.push((kind, start, i));
    }
    tokens.push((Kind::End, bytes.len(), bytes.len()));
    tokens
}

fn children(text: &str, tokens: &[(Kind, usize, usize)]) -> Vec<(usize, usize)> {
    let is_tag = |i: usize| {
        let t = tokens[i];
        t.0 == Kind::Other && {
            let word = &text.as_bytes()[t.1..t.2];
            word[0] == b'@'
                && word
                    .get(1)
                    .is_some_and(|b| b.is_ascii_alphabetic() || *b == b'\\')
        }
    };
    let ends = |i: usize| matches!(tokens[i].0, Kind::Close | Kind::End);
    let skip = |mut i: usize| {
        while tokens[i].0 == Kind::Ws && i + 1 < tokens.len() {
            i += 1;
        }
        i
    };
    let end_of = |current: usize| {
        let mut end = current - 1;
        while tokens[end].0 == Kind::Ws && end > 0 {
            end -= 1;
        }
        end + 1
    };
    let mut children = Vec::new();
    let mut pos = skip(1);
    if tokens[pos].0 == Kind::Eol {
        pos = skip(pos + 1);
    }
    if ends(pos) {
        return children;
    }
    loop {
        let start = pos;
        let mut savepoint: Option<usize> = None;
        loop {
            let skipped = pos > 0 && tokens[pos - 1].0 == Kind::Ws;
            let from = pos;
            while !matches!(tokens[pos].0, Kind::Eol | Kind::Close | Kind::End) {
                pos += 1;
            }
            if !matches!(tokens[pos].0, Kind::Eol | Kind::Close) {
                break;
            }
            if savepoint.is_none() || skipped || pos > from {
                savepoint = Some(pos);
            }
            let next = skip(pos + 1);
            if is_tag(next) || ends(next) {
                break;
            }
            pos = next;
        }
        if let Some(savepoint) = savepoint {
            pos = savepoint;
        }
        children.push((start, end_of(pos)));
        if ends(pos) || tokens[pos].0 != Kind::Eol {
            break;
        }
        pos = skip(pos + 1);
        if ends(pos) {
            break;
        }
    }
    children
}
