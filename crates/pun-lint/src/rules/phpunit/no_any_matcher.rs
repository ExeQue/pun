use crate::problem::{Edit, Safety};
use mago_syntax::cst::{Call, Expression, Node, NodeKind};

use super::{AnyCall, is_test_case_call};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoAnyMatcher;

impl Rule for NoAnyMatcher {
    fn id(&self) -> &'static str {
        "lint/phpunit/no-any-matcher"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Assignment])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Assignment(assign) = node else {
            return;
        };
        let Expression::Call(Call::Method(method_call)) = assign.rhs else {
            return;
        };
        let call = AnyCall::of(Node::MethodCall(method_call)).unwrap();
        if !call.is_name(&["any"])
            || !call.args.arguments.is_empty()
            || !is_test_case_call(ctx, ancestors, &call)
        {
            return;
        }
        out.push(Report::fixed(
            node_span(method_call),
            "Create the `AnyInvokedCount` matcher instead of calling the deprecated `any()`.",
            Safety::Safe,
            vec![Edit::new(
                node_span(method_call),
                "new \\PHPUnit\\Framework\\MockObject\\Rule\\AnyInvokedCount()",
            )],
        ));
    }
}
