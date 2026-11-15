use std::ops::Range;

use pun_tokens::Kind;

use crate::{Edit, Fix, LineIndex, Problem, Safety, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Lines(usize, usize),
    File,
}

#[derive(Clone, Debug)]
struct Id {
    text: String,
    span: Span,
    used: bool,
}

#[derive(Clone, Debug)]
struct Directive {
    scope: Scope,
    ids: Vec<Id>,
    comment: Span,
    own_line: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Malformed {
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unused {
    pub id: String,
    pub span: Span,
    pub fix: Fix,
}

#[derive(Clone, Debug, Default)]
pub struct Ignores {
    directives: Vec<Directive>,
    pub malformed: Vec<Malformed>,
}

const MARK: &str = "@pun-ignore";

pub fn covers(id: &str, rule: &str) -> bool {
    if id == rule {
        return true;
    }
    let mut parts = rule.splitn(3, '/');
    let (Some(command), Some(group), name) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    match id.split_once('/') {
        None => id == group,
        Some((a, b)) if a == command => b == group,
        Some((a, b)) => name.is_some_and(|n| a == group && b == n),
    }
}

impl Ignores {
    pub fn parse(source: &str) -> Ignores {
        let mut out = Ignores::default();
        if !source.contains(MARK) {
            return out;
        }
        let index = LineIndex::new(source);
        let base = source.as_ptr() as usize;
        let mut open: Vec<usize> = Vec::new();
        for token in pun_tokens::tokenize(source.as_bytes()) {
            if !matches!(token.kind, Some(Kind::T_COMMENT | Kind::T_DOC_COMMENT)) {
                continue;
            }
            let start = token.text.as_ptr() as usize - base;
            let comment = Span::new(start, start + token.text.len());
            let text = &source[comment.start..comment.end];
            let Some(at) = text.find(MARK) else {
                continue;
            };
            let rest = &text[at + MARK.len()..];
            let (kind, after) = ["-next-line", "-file", "-start", "-end", ""]
                .into_iter()
                .find_map(|k| rest.strip_prefix(k).map(|r| (k, r)))
                .expect("the empty suffix always matches");
            if after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                continue;
            }
            let word = Span::new(start + at, start + at + MARK.len() + kind.len());
            let first = index.line_col(comment.start).line;
            let last = index
                .line_col(comment.end.saturating_sub(1).max(comment.start))
                .line;
            let line = index.line_range(first).expect("the comment's line");
            let own_line = source[line.start..comment.start].trim().is_empty()
                && (index.line_range(last))
                    .is_some_and(|r| source[comment.end..r.end].trim().is_empty());
            if kind == "-end" {
                if let Some(i) = open.pop()
                    && let Scope::Lines(from, _) = out.directives[i].scope
                {
                    out.directives[i].scope = Scope::Lines(from, last);
                }
                continue;
            }
            let ids = ids(source, word.end, comment.end, text.ends_with("*/"));
            if ids.is_empty() {
                out.malformed.push(Malformed {
                    span: word,
                    message: format!("`{MARK}{kind}` needs one or more rule IDs"),
                });
                continue;
            }
            let scope = match kind {
                "-file" => Scope::File,
                "-start" => Scope::Lines(first, usize::MAX),
                "-next-line" => Scope::Lines(last + 1, last + 1),
                _ if own_line => Scope::Lines(last + 1, last + 1),
                _ => Scope::Lines(first, first),
            };
            if kind == "-start" {
                open.push(out.directives.len());
            }
            out.directives.push(Directive {
                scope,
                ids,
                comment,
                own_line,
            });
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.directives.is_empty() && self.malformed.is_empty()
    }

    pub fn retain(&mut self, problems: &mut Vec<Problem>, index: &LineIndex) {
        if self.directives.is_empty() {
            return;
        }
        problems.retain(|p| !self.ignores(&p.rule, index.line_col(p.span.start).line));
    }

    pub fn ignores(&mut self, rule: &str, line: usize) -> bool {
        let mut ignored = false;
        for d in &mut self.directives {
            let applies = match d.scope {
                Scope::File => true,
                Scope::Lines(from, to) => (from..=to).contains(&line),
            };
            if !applies {
                continue;
            }
            for id in &mut d.ids {
                if covers(&id.text, rule) {
                    id.used = true;
                    ignored = true;
                }
            }
        }
        ignored
    }

    pub fn ignores_file(&mut self, rule: &str) -> bool {
        let mut ignored = false;
        for d in self
            .directives
            .iter_mut()
            .filter(|d| d.scope == Scope::File)
        {
            for id in d.ids.iter_mut().filter(|i| covers(&i.text, rule)) {
                id.used = true;
                ignored = true;
            }
        }
        ignored
    }

    pub fn unused(&self, source: &str, ran: impl Fn(&str) -> bool) -> Vec<Unused> {
        let mut out = Vec::new();
        for d in &self.directives {
            let judged: Vec<&Id> = d.ids.iter().filter(|i| ran(&i.text)).collect();
            let dead: Vec<&&Id> = judged.iter().filter(|i| !i.used).collect();
            if dead.is_empty() {
                continue;
            }
            let all_dead = dead.len() == d.ids.len();
            for id in dead {
                let edits = if all_dead {
                    vec![Edit::new(delete_comment(source, d), "")]
                } else {
                    vec![Edit::new(delete_id(source, d, id.span), "")]
                };
                out.push(Unused {
                    id: id.text.clone(),
                    span: id.span,
                    fix: Fix {
                        safety: Safety::Safe,
                        edits,
                    },
                });
            }
        }
        out
    }
}

fn ids(source: &str, from: usize, comment_end: usize, block: bool) -> Vec<Id> {
    let end = if block { comment_end - 2 } else { comment_end };
    let mut text = &source[from..end];
    if let Some(i) = text.find('\n') {
        text = &text[..i];
    }
    if let Some(i) = text.find(" --") {
        text = &text[..i];
    }
    let mut out = Vec::new();
    let mut offset = from;
    for part in text.split(',') {
        let trimmed = part.trim_start();
        let start = offset + part.len() - trimmed.len();
        let id = trimmed.split_whitespace().next().unwrap_or("");
        let id = id.trim_end_matches('*');
        if !id.is_empty() {
            out.push(Id {
                text: id.to_string(),
                span: Span::new(start, start + id.len()),
                used: false,
            });
        }
        offset += part.len() + 1;
    }
    out
}

fn delete_comment(source: &str, d: &Directive) -> Span {
    if !d.own_line {
        let before = source[..d.comment.start]
            .trim_end_matches([' ', '\t'])
            .len();
        return Span::new(before, d.comment.end);
    }
    let start = source[..d.comment.start].rfind('\n').map_or(0, |i| i + 1);
    let rest = &source[d.comment.end..];
    let end = d.comment.end + rest.find('\n').map_or(rest.len(), |i| i + 1);
    Span::new(start, end)
}

fn delete_id(source: &str, d: &Directive, id: Span) -> Span {
    let after: Range<usize> = id.end..d.comment.end;
    let tail = &source[after];
    let skipped = tail.len() - tail.trim_start_matches([' ', '\t']).len();
    if tail[skipped..].starts_with(',') {
        let rest = &tail[skipped + 1..];
        let spaces = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        return Span::new(id.start, id.end + skipped + 1 + spaces);
    }
    let head = &source[d.comment.start..id.start];
    let comma = head.trim_end_matches([' ', '\t']);
    let comma = comma.strip_suffix(',').unwrap_or(comma);
    Span::new(d.comment.start + comma.len(), id.end)
}
