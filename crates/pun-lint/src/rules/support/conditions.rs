use mago_syntax::cst::{BinaryOperator, Expression, If, IfBody, Node, Statement};
use mago_syntax::token::TokenKind;
use pun_problem::Span;

use crate::Context;
use crate::rules::support::expressions::{equal, is_constant};
use crate::rules::support::statement_edits::is_plain_assign;
use crate::rules::support::statements::{List, Token, body_list, line_indent, open_list};
use crate::syntax::unparenthesize;

pub(crate) fn is_binary_op(expr: &Expression<'_>) -> bool {
    match unparenthesize(expr) {
        Expression::Binary(b) => !matches!(b.operator, BinaryOperator::Instanceof(_)),
        Expression::Pipe(_) => true,
        _ => false,
    }
}

pub(crate) fn match_first_and_second<'a>(
    left: &'a Expression<'a>,
    right: &'a Expression<'a>,
    first: impl Fn(&'a Expression<'a>) -> bool,
    second: impl Fn(&'a Expression<'a>) -> bool,
) -> Option<(&'a Expression<'a>, &'a Expression<'a>)> {
    let (left, right) = (unparenthesize(left), unparenthesize(right));
    if first(left) && second(right) {
        return Some((left, right));
    }
    if first(right) && second(left) {
        return Some((right, left));
    }
    None
}

pub(crate) fn is_assign(expr: &Expression<'_>) -> bool {
    matches!(unparenthesize(expr), Expression::Assignment(a) if is_plain_assign(a))
}

pub(crate) struct IfParts<'ast, 'arena> {
    pub stmts: List<'ast, 'arena>,
    pub has_elseifs: bool,
    pub has_else: bool,
}

pub(crate) fn if_parts<'ast, 'arena>(
    tokens: &[Token],
    node: &'ast If<'arena>,
) -> IfParts<'ast, 'arena> {
    match &node.body {
        IfBody::Statement(body) => IfParts {
            stmts: body_list(tokens, body.statement),
            has_elseifs: !body.else_if_clauses.is_empty(),
            has_else: body.else_clause.is_some(),
        },
        IfBody::ColonDelimited(body) => {
            let to = body
                .else_if_clauses
                .first()
                .map(|c| mago_span::HasSpan::span(c).start.offset as usize)
                .or_else(|| {
                    body.else_clause
                        .as_ref()
                        .map(|c| mago_span::HasSpan::span(c).start.offset as usize)
                })
                .unwrap_or(body.endif.span.start.offset as usize);
            IfParts {
                stmts: open_list(tokens, &body.statements, body.colon.end.offset as usize, to),
                has_elseifs: !body.else_if_clauses.is_empty(),
                has_else: body.else_clause.is_some(),
            }
        }
    }
}

pub(crate) fn if_not_null_return_value<'a>(
    ctx: &Context<'_>,
    node: &'a If<'a>,
    stmts: &List<'_, '_>,
) -> Option<&'a Expression<'a>> {
    if stmts.len() != 1 {
        return None;
    }
    let Statement::Return(ret) = stmts.stmts[0] else {
        return None;
    };
    let Expression::Binary(cond) = unparenthesize(node.condition) else {
        return None;
    };
    if !matches!(cond.operator, BinaryOperator::NotIdentical(_)) {
        return None;
    }
    let returned = ret.value?;
    if equal(ctx, cond.lhs, returned) && is_constant(cond.rhs, "null") {
        return Some(cond.lhs);
    }
    if !equal(ctx, cond.rhs, returned) {
        return None;
    }
    is_constant(cond.lhs, "null").then_some(cond.rhs)
}

pub(crate) fn is_stmts_aware(holder: Node<'_, '_>, ancestors: &[Node<'_, '_>]) -> bool {
    !(matches!(holder, Node::Block(_))
        && matches!(ancestors.last(), Some(Node::PropertyHookConcreteBody(_))))
}

pub(crate) fn indentation(source: &str, pos: usize) -> usize {
    line_indent(source, pos)
        .bytes()
        .map(|b| if b == b'\t' { 4 } else { 1 })
        .sum()
}

pub(crate) fn reprint(source: &str, tokens: &[Token], span: Span, adjust: isize) -> String {
    let mut out = String::with_capacity(span.end - span.start);
    let mut pos = span.start;
    for token in tokens
        .iter()
        .filter(|t| t.end > span.start && t.start < span.end)
    {
        let (start, end) = (token.start.max(span.start), token.end.min(span.end));
        out.push_str(&source[pos..start]);
        let text = &source[start..end];
        if adjust == 0 || matches!(token.kind, TokenKind::LiteralString | TokenKind::StringPart) {
            out.push_str(text);
        } else if adjust < 0 {
            out.push_str(&text.replace(&format!("\n{}", " ".repeat(adjust.unsigned_abs())), "\n"));
        } else {
            out.push_str(&text.replace('\n', &format!("\n{}", " ".repeat(adjust as usize))));
        }
        pos = end;
    }
    out.push_str(&source[pos..span.end]);
    out
}

pub(crate) fn comments_between(tokens: &[Token], from: usize, to: usize) -> Vec<Span> {
    crate::rules::support::statements::within(tokens, from, to)
        .iter()
        .filter(|t| t.kind.is_comment())
        .map(|t| Span::new(t.start, t.end))
        .collect()
}

pub(crate) fn print_comments(source: &str, comments: &[Span], nl: &str) -> String {
    comments
        .iter()
        .map(|c| reformatted(&source[c.start..c.end]).replace('\n', nl))
        .collect::<Vec<_>>()
        .join(nl)
}

fn reformatted(text: &str) -> String {
    let text = text.replace("\r\n", "\n");
    if !text.contains('\n') {
        return text;
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let blanks = |l: &str| l.len() - l.trim_start().len();
    if lines[1..]
        .iter()
        .all(|l| blanks(l) > 0 && l.trim_start().starts_with('*'))
    {
        return std::iter::once(lines[0].to_string())
            .chain(lines[1..].iter().map(|l| format!(" {}", l.trim_start())))
            .collect::<Vec<_>>()
            .join("\n");
    }
    let after = text.strip_prefix("/**").unwrap_or(&text[2..]);
    let last = lines[lines.len() - 1];
    if after.trim_start_matches([' ', '\t']).starts_with('\n') && last.trim_start() == "*/" {
        let indent = &last[..last.len() - 2];
        return lines
            .iter()
            .map(|l| l.strip_prefix(indent).unwrap_or(l))
            .collect::<Vec<_>>()
            .join("\n");
    }
    let opening = text.len() - after.trim_start().len();
    let shortest = lines[1..].iter().map(|l| blanks(l)).min().unwrap_or(0);
    match shortest.checked_sub(opening) {
        Some(remove) if remove > 0 => lines
            .iter()
            .map(|l| if blanks(l) >= remove { &l[remove..] } else { l })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => text,
    }
}
