use mago_syntax::cst::{MethodCall, Node, NodeKind};
use pun_problem::{Edit, Safety, Span};

use super::{identifier, root_func_call};
use crate::rules::support::expressions::named;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoFocusedTest;

impl Rule for NoFocusedTest {
    fn id(&self) -> &'static str {
        "lint/pest/no-focused-test"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::MethodCall(call) = node else {
            return;
        };
        if identifier(&call.method) != Some(b"only") || !is_pest_test_chain(call) {
            return;
        }
        out.push(Report::fixed(
            node_span(call),
            "Remove `->only()`, which runs this test alone.",
            Safety::Unsafe,
            vec![Edit::new(
                Span::new(
                    node_span(unparenthesize(call.object)).end,
                    node_span(call).end,
                ),
                "",
            )],
        ));
    }
}

const PEST_TEST_FUNCTIONS: [&str; 8] = [
    "test",
    "it",
    "describe",
    "todo",
    "beforeEach",
    "afterEach",
    "beforeAll",
    "afterAll",
];

fn is_pest_test_chain(call: &MethodCall<'_>) -> bool {
    root_func_call(call).is_some_and(|f| PEST_TEST_FUNCTIONS.iter().any(|name| named(f, name)))
}
