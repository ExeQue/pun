use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{Namespace, NamespaceBody, Node, NodeKind, Sequence, Statement};

use super::is_in_test_class;
use crate::rules::support::statement_edits::contains_html;
use crate::rules::support::statement_edits::{open_list, printed, tokens, trailing_comment};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const DECLARE: &str = "declare(strict_types=1);";

pub struct RequireStrictTypesInTests;

impl Rule for RequireStrictTypesInTests {
    fn id(&self) -> &'static str {
        "lint/phpunit/require-strict-types-in-tests"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Program])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Program(program) = node else { return };
        if has_strict_types(&program.statements)
            || contains_html(ctx, node, ancestors)
            || !first_class_is_test(ctx, node)
        {
            return;
        }
        let Some((at, edit)) = declare_strict_types(ctx, &program.statements) else {
            return;
        };
        out.push(Report::fixed(
            at,
            "Declare strict types in a test file.",
            Safety::Unsafe,
            vec![edit],
        ));
    }
}

pub(crate) fn has_strict_types(statements: &Sequence<'_, Statement<'_>>) -> bool {
    statements.iter().any(|statement| {
        matches!(statement, Statement::Declare(d) if d.items.iter().any(|i| i.name.value == b"strict_types"))
    })
}

pub(crate) fn declare_strict_types(
    ctx: &crate::Context<'_>,
    statements: &Sequence<'_, Statement<'_>>,
) -> Option<(Span, Edit)> {
    let source = ctx.source;
    let list = open_list(source, statements, 0, source.len());
    if list.stmts.is_empty() {
        return None;
    }
    let first = list.with_comments(ctx, 0);
    let last_end = list.stmts.last().map_or(0, |s| node_span(*s).end);
    let in_namespace = matches!(
        list.stmts.last(),
        Some(Statement::Namespace(Namespace {
            body: NamespaceBody::Implicit(_),
            ..
        }))
    );
    let nop = list.nop || closing_nop(source, statements, last_end).is_some();
    let edit = if list.stmts.len() + usize::from(nop && !in_namespace) == 1 {
        let mut text = format!("<?php\n\n{DECLARE}\n");
        let start = node_span(list.stmts[0]).start;
        for comment in tokens(source, list.before(ctx, 0), start)
            .into_iter()
            .filter(|t| t.kind.is_comment())
        {
            text.push('\n');
            text.push_str(&source[comment.start..comment.end]);
        }
        let end = match list.stmts[0] {
            Statement::Namespace(Namespace {
                body: NamespaceBody::Implicit(body),
                ..
            }) => {
                let terminator = node_span(&body.terminator).end;
                let inner = open_list(source, &body.statements, terminator, source.len());
                let end = printed(ctx, &inner).end.max(terminator);
                closing_nop(source, &body.statements, end).unwrap_or(end)
            }
            statement => node_span(statement).end,
        };
        text.push('\n');
        text.push_str(&source[start..end]);
        text.push('\n');
        Edit::new(Span::new(0, source.len()), text)
    } else {
        Edit::new(
            Span::new(first.start, first.start),
            format!("{DECLARE}\n\n"),
        )
    };
    Some((Span::new(first.start, first.start), edit))
}

fn closing_nop(
    source: &str,
    statements: &Sequence<'_, Statement<'_>>,
    after: usize,
) -> Option<usize> {
    statements
        .iter()
        .find_map(|s| match s {
            Statement::ClosingTag(tag) if node_span(tag).start >= after => Some(node_span(tag)),
            _ => None,
        })
        .filter(|tag| trailing_comment(source, after, tag.start))
        .map(|tag| tag.end)
}

fn first_class_is_test(ctx: &Context<'_>, program: Node<'_, '_>) -> bool {
    let mut stack = vec![(program, 0usize)];
    let mut path: Vec<Node> = Vec::new();
    while let Some((node, depth)) = stack.pop() {
        path.truncate(depth);
        if matches!(node, Node::Class(_)) {
            path.push(node);
            return is_in_test_class(ctx, &path, None);
        }
        path.push(node);
        let mut children = Vec::new();
        node.visit_children(|child| children.push(child));
        stack.extend(children.into_iter().rev().map(|c| (c, depth + 1)));
    }
    false
}
