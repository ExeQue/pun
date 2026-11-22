use crate::problem::{Edit, Span};
use mago_syntax::cst::*;

use crate::rules::support::class_members::method_without_params;
use crate::rules::support::docblocks::{doc_comment, doc_without_params};
use crate::rules::support::statement_edits::{
    Frame, List, body_list, enclosing, first_comment, holder_list, inserted_statements,
    line_indent, moved_statements, remove,
};
use crate::syntax::node_span;

pub(crate) fn replace_with<'a>(
    ctx: &crate::Context<'_>,
    statement: &'a Statement<'a>,
    ancestors: &[Node<'a, 'a>],
    stmts: Option<&List<'a>>,
    changes: impl Fn(&Statement<'_>) -> bool,
) -> Vec<Edit> {
    let source = ctx.source;
    let (list, index) = enclosing(source, statement, ancestors);
    let alone = list
        .stmts
        .iter()
        .enumerate()
        .all(|(i, s)| i == index || !changes(s));
    let Some(stmts) = stmts.filter(|s| !s.is_empty()) else {
        return remove(ctx, &list, &[index]);
    };
    let span = node_span(list.stmts[index]);
    let indent = line_indent(source, span.start);
    if list.frame == Frame::Bare && stmts.len() > 1 {
        let from = source[..span.start].trim_end().len();
        let outer = line_indent(source, from);
        let inner = format!("{outer}    ");
        let text = format!(
            " {{\n{inner}{}\n{outer}}}",
            moved_statements(ctx, stmts, &inner)
        );
        return vec![Edit::new(Span::new(from, span.end), text)];
    }
    let text = inserted_statements(ctx, stmts, indent);
    if alone && stmts.len() > 1 {
        if index > 0 {
            let from = list.before(ctx, index);
            return vec![Edit::new(
                Span::new(from, span.end),
                format!("\n{indent}{text}"),
            )];
        }
        if index + 1 < list.stmts.len() {
            let from = list.with_comments(ctx, index).start;
            let to = list.with_comments(ctx, index + 1).start;
            return vec![Edit::new(Span::new(from, to), format!("{text}\n{indent}"))];
        }
    }
    vec![Edit::new(span, text)]
}

pub(crate) fn list_removal(items: &[Span], removed: &[bool]) -> Option<Vec<Edit>> {
    let first_kept = removed.iter().position(|r| !r)?;
    let mut edits = Vec::new();
    if first_kept > 0 {
        edits.push(Edit::new(
            Span::new(items[0].start, items[first_kept].start),
            "",
        ));
    }
    for i in first_kept + 1..items.len() {
        if removed[i] {
            edits.push(Edit::new(Span::new(items[i - 1].end, items[i].end), ""));
        }
    }
    Some(merge(edits))
}

pub(crate) fn merge(mut edits: Vec<Edit>) -> Vec<Edit> {
    edits.sort_by_key(|e| (e.span.start, e.span.end));
    let mut out: Vec<Edit> = Vec::new();
    for edit in edits {
        match out.last_mut() {
            Some(last) if last.span.end == edit.span.start => {
                let text = format!("{}{}", last.text, edit.text);
                *last = Edit::new(Span::new(last.span.start, edit.span.end), text);
            }
            _ => out.push(edit),
        }
    }
    out
}

pub(crate) fn remove_params(
    ctx: &crate::Context<'_>,
    method: &Method<'_>,
    from: usize,
    removed: &[bool],
) -> Vec<Edit> {
    let source = ctx.source;
    let params: Vec<&FunctionLikeParameter> = method.parameter_list.parameters.iter().collect();
    let span = node_span(method);
    let mut edits = Vec::new();
    let names: Vec<&[u8]> = params
        .iter()
        .zip(removed)
        .filter(|(_, r)| **r)
        .map(|(p, _)| p.variable.name)
        .collect();
    if let Some(doc) = doc_comment(source, from, span.start)
        && let Some(text) = doc_without_params(&source[doc.start..doc.end], &names)
    {
        edits.push(if text.is_empty() {
            Edit::new(Span::new(doc.start, span.start), "")
        } else {
            Edit::new(doc, text)
        });
    }
    let items: Vec<Span> = params.iter().map(|p| node_span(*p)).collect();
    match list_removal(&items, removed) {
        Some(list) => edits.extend(list),
        None => edits.push(Edit::new(span, method_without_params(ctx, method))),
    }
    merge(edits)
}

pub(crate) fn remove_items(
    source: &str,
    spans: &[Span],
    froms: &[usize],
    removed: &[bool],
) -> Option<Vec<Edit>> {
    let first_kept = removed.iter().position(|r| !r)?;
    let comment_start =
        |i: usize| first_comment(source, froms[i], spans[i].start).unwrap_or(spans[i].start);
    let mut edits = Vec::new();
    if first_kept > 0 {
        edits.push(Edit::new(
            Span::new(comment_start(0), comment_start(first_kept)),
            "",
        ));
    }
    for i in first_kept + 1..spans.len() {
        if removed[i] {
            edits.push(Edit::new(Span::new(froms[i], spans[i].end), ""));
        }
    }
    Some(merge(edits))
}

pub(crate) fn remove_statements(
    ctx: &crate::Context<'_>,
    list: &List<'_>,
    removed: &[bool],
) -> Vec<Edit> {
    let source = ctx.source;
    let mut spans: Vec<Span> = list.stmts.iter().map(|s| node_span(*s)).collect();
    let mut froms: Vec<usize> = (0..list.stmts.len()).map(|i| list.before(ctx, i)).collect();
    let mut flags = removed.to_vec();
    if let Some(nop) = list.nop_span(source) {
        froms.push(spans.last().map_or(list.inner.start, |s| s.end));
        spans.push(nop);
        flags.push(false);
    }
    match remove_items(source, &spans, &froms, &flags) {
        Some(edits) => edits,
        None => {
            let all: Vec<usize> = (0..list.stmts.len()).collect();
            remove(ctx, list, &all)
        }
    }
}

pub(crate) fn replace_run(
    ctx: &crate::Context<'_>,
    list: &List<'_>,
    start: usize,
    end: usize,
    code: &str,
) -> Option<Vec<Edit>> {
    let source = ctx.source;
    let first = node_span(list.stmts[start]);
    let indent = line_indent(source, first.start);
    let last = node_span(list.stmts[end]);
    if start == end {
        let from = list.with_comments(ctx, start).start;
        return Some(vec![Edit::new(Span::new(from, last.end), code.to_string())]);
    }
    if start > 0 {
        let from = list.before(ctx, start);
        return Some(vec![Edit::new(
            Span::new(from, last.end),
            format!("\n{indent}{code}"),
        )]);
    }
    if end + 1 < list.stmts.len() || list.nop {
        return None;
    }
    let Frame::Braces { open, close } = list.frame else {
        return None;
    };
    let outer = line_indent(source, open - 1);
    Some(vec![Edit::new(
        Span::new(open, close),
        format!("\n{outer}    {code}\n{outer}"),
    )])
}

pub(crate) fn foreach_list<'a>(source: &str, foreach: &'a Foreach<'a>) -> Option<List<'a>> {
    match &foreach.body {
        ForeachBody::Statement(statement) => Some(body_list(source, statement)),
        ForeachBody::ColonDelimited(body) => {
            holder_list(source, Node::ForeachColonDelimitedBody(body), &[])
        }
    }
}
