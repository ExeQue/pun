use mago_syntax::cst::{ExpressionStatement, Node, NodeKind};
use pun_problem::{Fix, Safety};

use crate::rules::support::expressions::{args, call_to, operand, statements_edit, text};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferArrayAppend;

impl Rule for PreferArrayAppend {
    fn id(&self) -> &'static str {
        "lint/arrays/prefer-array-append"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ExpressionStatement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if let Node::ExpressionStatement(statement) = node {
            check(statement, ancestors, ctx, out);
        }
    }
}

fn check(
    statement: &ExpressionStatement<'_>,
    ancestors: &[Node<'_, '_>],
    ctx: &Context<'_>,
    out: &mut Vec<Report>,
) {
    let Some(call) = call_to(statement.expression, "array_push") else {
        return;
    };
    let args = args(call);
    if args.iter().any(|a| a.is_unpacked()) || args.len() < 2 {
        return;
    }
    let array = text(ctx, args[0].value());
    let parts: Vec<String> = args[1..]
        .iter()
        .map(|a| format!("{array}[] = {}", operand(ctx, a.value(), 160, None)))
        .collect();
    let Some(edit) = statements_edit(ctx, ancestors, statement, &parts) else {
        return;
    };
    out.push(Report {
        span: node_span(statement),
        message: "Use `$array[] =` instead of `array_push()`.".to_string(),
        help: None,
        fix: Some(Fix {
            safety: Safety::Unsafe,
            edits: vec![edit],
        }),
    });
}
