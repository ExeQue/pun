use mago_syntax::cst::{Expression, ForBody, ForeachBody, Node, NodeKind, Statement, WhileBody};

use super::lint_b_early_exit::{EarlyExitPattern, check_early_exit, single_if, single_if_in};
use crate::{Context, Report, Rule};

pub struct PreferEarlyContinue;

const PATTERN: EarlyExitPattern = EarlyExitPattern {
    keyword: "continue",
    title: "Consider using early continue pattern to reduce nesting.",
    help: "Invert the condition and use `continue` to exit early, then place the main logic outside the if block.",
    is_early_exit_statement,
};

impl Rule for PreferEarlyContinue {
    fn id(&self) -> &'static str {
        "lint/style/prefer-early-continue"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::For,
            NodeKind::Foreach,
            NodeKind::While,
            NodeKind::DoWhile,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let r#if = match node {
            Node::For(l) => match &l.body {
                ForBody::Statement(s) => single_if(s),
                ForBody::ColonDelimited(b) => single_if_in(b.statements.as_slice()),
            },
            Node::Foreach(l) => match &l.body {
                ForeachBody::Statement(s) => single_if(s),
                ForeachBody::ColonDelimited(b) => single_if_in(b.statements.as_slice()),
            },
            Node::While(l) => match &l.body {
                WhileBody::Statement(s) => single_if(s),
                WhileBody::ColonDelimited(b) => single_if_in(b.statements.as_slice()),
            },
            _ => {
                let Node::DoWhile(l) = node else { return };
                single_if(l.statement)
            }
        };
        let Some(r#if) = r#if else { return };
        check_early_exit(ctx.source, r#if, 0, &PATTERN, out);
    }
}

fn is_early_exit_statement(stmt: &Statement<'_>) -> bool {
    match stmt {
        Statement::Continue(_) | Statement::Break(_) | Statement::Return(_) => true,
        Statement::Expression(e) => matches!(e.expression, Expression::Throw(_)),
        Statement::Block(block) => {
            block.statements.len() == 1 && is_early_exit_statement(&block.statements.nodes[0])
        }
        _ => false,
    }
}
