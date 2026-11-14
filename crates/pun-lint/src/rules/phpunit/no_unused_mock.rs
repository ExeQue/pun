use mago_syntax::cst::{Call, Expression, MethodBody, Node, NodeKind, Statement};
use pun_problem::Safety;

use super::{AnyCall, is_in_test_class};
use crate::rules::support::rewrites::remove_statements;
use crate::rules::support::statement_edits::block_list;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoUnusedMock;

impl Rule for NoUnusedMock {
    fn id(&self) -> &'static str {
        "lint/phpunit/no-unused-mock"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Method(method) = node else { return };
        let MethodBody::Concrete(block) = &method.body else {
            return;
        };
        if !is_in_test_class(ctx, ancestors, Some(method)) {
            return;
        }
        let list = block_list(ctx.source, block);
        let removed: Vec<bool> = list
            .stmts
            .iter()
            .map(|statement| {
                let Statement::Expression(statement) = statement else {
                    return false;
                };
                let Expression::Call(Call::Method(first)) = unparenthesize(statement.expression)
                else {
                    return false;
                };
                let mut call = first;
                while let Expression::Call(Call::Method(inner)) = unparenthesize(call.object) {
                    call = inner;
                }
                AnyCall::of(Node::MethodCall(call)).is_some_and(|c| c.is_name(&["createMock"]))
            })
            .collect();
        for (at, statement) in list.stmts.iter().enumerate() {
            if removed[at] {
                let mut only = vec![false; removed.len()];
                only[at] = true;
                out.push(Report::fixed(
                    node_span(*statement),
                    "Remove the mock: nothing uses it.",
                    Safety::Safe,
                    remove_statements(ctx, &list, &only),
                ));
            }
        }
    }
}
