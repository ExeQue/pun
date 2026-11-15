use mago_syntax::cst::{
    DoWhile, Expression, ForBody, ForeachBody, ForeachTarget, Literal, Node, NodeKind, Sequence,
    Statement, TokenSeparatedSequence, WhileBody,
};
use pun_problem::{Edit, Fix, Safety};

use crate::rules::support::statements::{body_list, line_indent, open_list, remove, tokens};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoUselessContinue;

impl Rule for NoUselessContinue {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-useless-continue"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::DoWhile,
            NodeKind::For,
            NodeKind::Foreach,
            NodeKind::While,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let open = |s, colon: mago_span::Span, end: mago_span::Span| {
            Body::Open(s, colon.end.offset as usize, end.start.offset as usize)
        };
        let body = match node {
            Node::DoWhile(DoWhile { statement, .. }) => Body::One(statement),
            Node::For(f) => match &f.body {
                ForBody::Statement(s) => Body::One(s),
                ForBody::ColonDelimited(b) => open(&b.statements, b.colon, b.end_for.span),
            },
            Node::Foreach(f) => match &f.body {
                ForeachBody::Statement(s) => Body::One(s),
                ForeachBody::ColonDelimited(b) => open(&b.statements, b.colon, b.end_foreach.span),
            },
            _ => {
                let Node::While(w) = node else { return };
                match &w.body {
                    WhileBody::Statement(s) => Body::One(s),
                    WhileBody::ColonDelimited(b) => open(&b.statements, b.colon, b.end_while.span),
                }
            }
        };
        let candidate = match body {
            Body::One(Statement::Block(b)) => ends_with_continue(b.statements.as_slice()),
            Body::One(s) => is_removable(s),
            Body::Open(s, ..) => ends_with_continue(s.as_slice()),
        };
        if !candidate {
            return;
        }
        let tokens = tokens(ctx);
        let list = match body {
            Body::One(s) => body_list(tokens, s),
            Body::Open(s, from, to) => open_list(tokens, s, from, to),
        };
        if list.nop {
            return;
        }
        let removable: Vec<usize> = (0..list.stmts.len())
            .rev()
            .take_while(|&i| is_removable(list.stmts[i]))
            .collect();
        let first = *removable.last().expect("the last statement is a continue");
        let mut indices = removable;
        indices.reverse();
        let edits = if matches!(body, Body::Open(..)) && indices.len() == list.len() {
            let span = node_span(&node);
            let indent = line_indent(ctx.source, span.start);
            vec![Edit::new(
                span,
                format!("{} {{\n{indent}}}", header(ctx.source, node)),
            )]
        } else {
            remove(ctx.source, tokens, &list, &indices)
        };
        out.push(Report {
            span: node_span(list.stmts[first]),
            message: "This `continue` is the last statement of the loop; remove it.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}

fn header(source: &str, node: Node<'_, '_>) -> String {
    let text = |e: &Expression| {
        let s = node_span(unparenthesize(e));
        &source[s.start..s.end]
    };
    let list = |es: &TokenSeparatedSequence<'_, &Expression<'_>>| {
        es.iter().map(|e| text(e)).collect::<Vec<_>>().join(", ")
    };
    let gap =
        |es: &TokenSeparatedSequence<'_, &Expression<'_>>| if es.is_empty() { "" } else { " " };
    match node {
        Node::For(f) => format!(
            "for ({};{}{};{}{})",
            list(&f.initializations),
            gap(&f.conditions),
            list(&f.conditions),
            gap(&f.increments),
            list(&f.increments)
        ),
        Node::Foreach(f) => {
            let target = match &f.target {
                ForeachTarget::Value(t) => text(t.value).to_string(),
                ForeachTarget::KeyValue(t) => format!("{} => {}", text(t.key), text(t.value)),
            };
            format!("foreach ({} as {target})", text(f.expression))
        }
        _ => {
            let mut condition = "";
            if let Node::While(w) = node {
                condition = text(w.condition);
            }
            format!("while ({condition})")
        }
    }
}

#[derive(Clone, Copy)]
enum Body<'ast, 'arena> {
    One(&'ast Statement<'arena>),
    Open(&'ast Sequence<'arena, Statement<'arena>>, usize, usize),
}

fn ends_with_continue(statements: &[Statement<'_>]) -> bool {
    statements
        .iter()
        .rfind(|s| {
            !matches!(
                s,
                Statement::Noop(_) | Statement::OpeningTag(_) | Statement::ClosingTag(_)
            )
        })
        .is_some_and(is_removable)
}

fn is_removable(statement: &Statement<'_>) -> bool {
    let Statement::Continue(c) = statement else {
        return false;
    };
    match c.level.map(unparenthesize) {
        Some(Expression::Literal(Literal::Integer(i))) => i.value.is_some_and(|v| v < 2),
        _ => true,
    }
}
