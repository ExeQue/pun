use crate::problem::{Edit, Span};
use mago_span::HasSpan;
use mago_syntax::cst::{Expression, ForeachTarget, Node, Statement};

use crate::rules::complexity::no_redundant_null_check::reprint_function_like;
use crate::rules::support::conditions::{comments_between, indentation, print_comments, reprint};
use crate::rules::support::statements::{
    Frame, List, Token, enclosing, previous_token_end, remove,
};
use crate::syntax::node_span;

pub(crate) struct NewStatement {
    pub comments: Vec<Span>,
    pub code: Code,
}

#[derive(Clone)]
pub(crate) enum Code {
    Moved(Span),
    Text(String),
    Nop,
}

impl NewStatement {
    pub(crate) fn moved(source: &str, tokens: &[Token], span: Span) -> Self {
        let before = previous_token_end(source, tokens, span.start);
        NewStatement {
            comments: comments_between(tokens, before, span.start),
            code: Code::Moved(span),
        }
    }

    pub(crate) fn all(source: &str, tokens: &[Token], list: &List<'_, '_>) -> Vec<Self> {
        let mut out: Vec<Self> = list
            .stmts
            .iter()
            .map(|s| Self::moved(source, tokens, node_span(*s)))
            .collect();
        if list.nop {
            let after = list
                .stmts
                .last()
                .map_or(list.inner.start, |s| node_span(*s).end);
            out.push(NewStatement {
                comments: comments_between(tokens, after, list.inner.end),
                code: Code::Nop,
            });
        }
        out
    }

    pub(crate) fn print_in_list(
        &self,
        source: &str,
        tokens: &[Token],
        level: usize,
        nl: &str,
    ) -> String {
        let mut out = String::new();
        if !self.comments.is_empty() {
            out.push_str(nl);
            out.push_str(&print_comments(source, &self.comments, nl));
        }
        if !matches!(self.code, Code::Nop) {
            out.push_str(nl);
            out.push_str(
                &Self {
                    comments: Vec::new(),
                    code: self.code.clone(),
                }
                .print(source, tokens, level, nl),
            );
        }
        out
    }

    fn print(&self, source: &str, tokens: &[Token], level: usize, nl: &str) -> String {
        let mut out = String::new();
        if !self.comments.is_empty() {
            out.push_str(&print_comments(source, &self.comments, nl));
            out.push_str(nl);
        }
        match &self.code {
            Code::Moved(span) => out.push_str(&reprint(
                source,
                tokens,
                *span,
                level as isize - indentation(source, span.start) as isize,
            )),
            Code::Text(text) => out.push_str(text),
            Code::Nop => {}
        }
        out
    }
}

fn comment_start(source: &str, tokens: &[Token], list: &List<'_, '_>, index: usize) -> usize {
    let span = node_span(list.stmts[index]);
    let before = previous_token_end(source, tokens, span.start).max(list.inner.start);
    comments_between(tokens, before, span.start)
        .first()
        .map_or(span.start, |c| c.start)
}

pub(crate) fn splice(
    source: &str,
    tokens: &[Token],
    list: &List<'_, '_>,
    index: usize,
    count: usize,
    new: &[NewStatement],
) -> Option<Vec<Edit>> {
    let has_nop = list.nop || new.iter().any(|n| matches!(n.code, Code::Nop));
    let last = node_span(list.stmts[index + count - 1]);
    if new.is_empty() {
        let indices: Vec<usize> = (index..index + count).collect();
        return Some(remove(source, tokens, list, &indices));
    }
    if new.len() == count {
        return Some(
            new.iter()
                .enumerate()
                .map(|(j, one)| {
                    let old = node_span(list.stmts[index + j]);
                    let start = comment_start(source, tokens, list, index + j);
                    let level = indentation(source, old.start);
                    let nl = format!("\n{}", " ".repeat(level));
                    let text = one.print(source, tokens, level, &nl);
                    strip_nop_blanks(source, Span::new(start, old.end), text, has_nop)
                })
                .collect(),
        );
    }
    if list.frame == Frame::Bare {
        return None;
    }
    let start = comment_start(source, tokens, list, index);
    let (span, text) = if index > 0 {
        let previous = node_span(list.stmts[index - 1]);
        let level = indentation(source, previous.start);
        let nl = format!("\n{}", " ".repeat(level));
        let before = previous_token_end(source, tokens, start).max(list.inner.start);
        let text: String = new
            .iter()
            .map(|n| format!("{nl}{}", n.print(source, tokens, level, &nl)))
            .collect();
        (Span::new(before, last.end), text)
    } else {
        let next = if index + count < list.stmts.len() {
            comment_start(source, tokens, list, index + count)
        } else if list.nop {
            comments_between(tokens, last.end, list.inner.end)
                .first()?
                .start
        } else {
            return None;
        };
        let level = indentation(source, next);
        let nl = format!("\n{}", " ".repeat(level));
        let text: String = new
            .iter()
            .map(|n| format!("{}{nl}", n.print(source, tokens, level, &nl)))
            .collect();
        (Span::new(start, next), text)
    };
    Some(vec![strip_nop_blanks(source, span, text, has_nop)])
}

fn strip_nop_blanks(source: &str, span: Span, text: String, has_nop: bool) -> Edit {
    if !has_nop {
        return Edit::new(span, text);
    }
    let line_start = source[..span.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = source[span.end..]
        .find('\n')
        .map_or(source.len(), |i| span.end + i);
    let whole = format!(
        "{}{text}{}",
        &source[line_start..span.start],
        &source[span.end..line_end]
    );
    let cleaned: Vec<&str> = whole
        .split('\n')
        .map(|line| if line.trim().is_empty() { "" } else { line })
        .collect();
    Edit::new(Span::new(line_start, line_end), cleaned.join("\n"))
}

pub(crate) enum Change {
    Edits(Vec<Edit>),
    Remove,
    Replace(Vec<NewStatement>),
}

pub(crate) fn run_edits<'ast, 'arena>(
    source: &str,
    tokens: &[Token],
    ancestors: &[Node<'ast, 'arena>],
    change: &dyn Fn(&[Node<'ast, 'arena>]) -> Option<Change>,
) -> Option<Vec<Edit>> {
    let n = ancestors.len();
    let at = |statement: &'ast Statement<'arena>| {
        let mut chain = ancestors.to_vec();
        chain[n - 1] = Node::Statement(statement);
        change(&chain)
    };
    let replaced = |c: &Option<Change>| matches!(c, Some(Change::Remove | Change::Replace(_)));
    let own = change(ancestors)?;
    if let Change::Edits(edits) = own {
        return Some(edits);
    }
    let (list, index) = enclosing(source, tokens, ancestors);
    if index > 0 && replaced(&at(list.stmts[index - 1])) {
        return None;
    }
    let mut new = Vec::new();
    let mut count = 0;
    let mut next = Some(own);
    while let Some(Change::Remove | Change::Replace(_)) = &next {
        if let Some(Change::Replace(statements)) = next {
            new.extend(statements);
        }
        count += 1;
        next = list.stmts.get(index + count).and_then(|s| at(s));
    }
    match splice(source, tokens, &list, index, count, &new) {
        Some(edits) => Some(edits),
        None => holder_reprint(source, tokens, ancestors, &new),
    }
}

fn holder_reprint(
    source: &str,
    tokens: &[Token],
    ancestors: &[Node<'_, '_>],
    new: &[NewStatement],
) -> Option<Vec<Edit>> {
    let n = ancestors.len();
    let parent = ancestors.get(n.checked_sub(2)?)?;
    let body = |level: usize| -> String {
        let nl = format!("\n{}", " ".repeat(level));
        new.iter()
            .map(|s| s.print_in_list(source, tokens, level, &nl))
            .collect()
    };
    let text = |s: mago_span::Span| &source[s.start.offset as usize..s.end.offset as usize];
    let case = |span: mago_span::Span, head: String| {
        let span = node_span(&span);
        let level = indentation(source, span.start);
        Some(vec![Edit::new(span, format!("{head}:{}", body(level + 4)))])
    };
    match parent {
        Node::SwitchExpressionCase(c) => {
            return case(c.span(), format!("case {}", text(c.expression.span())));
        }
        Node::SwitchDefaultCase(c) => return case(c.span(), "default".to_string()),
        Node::Block(_) => {}
        _ => return None,
    }
    let owner = ancestors[..n - 2].iter().rev().find(|a| {
        !matches!(
            a,
            Node::Statement(_) | Node::MethodBody(_) | Node::ForeachBody(_) | Node::WhileBody(_)
        )
    })?;
    let braced = |start: usize, end: usize, head: String| {
        let level = indentation(source, start);
        Edit::new(
            Span::new(start, end),
            format!("{head} {{{}\n{}}}", body(level + 4), " ".repeat(level)),
        )
    };
    match owner {
        Node::IfStatementBody(if_body) => {
            let r#if = ancestors[..n - 2].iter().rev().find_map(|a| match a {
                Node::If(r#if) => Some(*r#if),
                _ => None,
            })?;
            let start = node_span(r#if).start;
            let end = node_span(if_body.statement).end;
            let mut edits = vec![braced(
                start,
                end,
                format!("if ({})", text(r#if.condition.span())),
            )];
            let mut previous = end;
            let clauses = (if_body.else_if_clauses.iter().map(mago_span::HasSpan::span))
                .chain(if_body.else_clause.iter().map(mago_span::HasSpan::span));
            for clause in clauses {
                let clause = node_span(&clause);
                edits.push(Edit::new(
                    Span::new(previous, clause.start),
                    " ".to_string(),
                ));
                previous = clause.end;
            }
            Some(edits)
        }
        Node::IfStatementBodyElseIfClause(clause) => {
            let span = node_span(*clause);
            Some(vec![braced(
                span.start,
                span.end,
                format!("elseif ({})", text(clause.condition.span())),
            )])
        }
        Node::IfStatementBodyElseClause(clause) => {
            let span = node_span(*clause);
            Some(vec![braced(span.start, span.end, "else".to_string())])
        }
        Node::Foreach(foreach) => {
            let text = |e: &Expression<'_>| {
                let s = node_span(e);
                &source[s.start..s.end]
            };
            let target = match &foreach.target {
                ForeachTarget::Value(v) => text(v.value).to_string(),
                ForeachTarget::KeyValue(kv) => format!("{} => {}", text(kv.key), text(kv.value)),
            };
            let span = node_span(*foreach);
            let level = indentation(source, span.start);
            Some(vec![Edit::new(
                span,
                format!(
                    "foreach ({} as {target}) {{{}\n{}}}",
                    text(foreach.expression),
                    body(level + 4),
                    " ".repeat(level)
                ),
            )])
        }
        Node::While(r#while) => {
            let span = node_span(*r#while);
            let level = indentation(source, span.start);
            let condition = node_span(r#while.condition);
            Some(vec![Edit::new(
                span,
                format!(
                    "while ({}) {{{}\n{}}}",
                    &source[condition.start..condition.end],
                    body(level + 4),
                    " ".repeat(level)
                ),
            )])
        }
        holder => reprint_function_like(source, tokens, *holder, &body).map(|e| vec![e]),
    }
}
