//! Rector 2.8.4's `RemoveUnusedVariableInCatchRector`.

use mago_syntax::cst::{Node, NodeKind};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::statements::{enclosing, tokens};
use crate::rules::support::variable_usage::{try_of, used_in, used_in_next};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoUnusedCatchVariable;

impl Rule for NoUnusedCatchVariable {
    fn id(&self) -> &'static str {
        "lint/php-8.0/no-unused-catch-variable"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80000)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Try])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Try(r#try) = node else {
            return;
        };
        for catch in &r#try.catch_clauses {
            let Some(variable) = &catch.variable else {
                continue;
            };
            let name = &variable.name[1..];
            // The catch's own statements and the `finally` statements.
            if used_in(Node::Block(&catch.block), None, name)
                || r#try
                    .finally_clause
                    .as_ref()
                    .is_some_and(|f| used_in(Node::Block(&f.block), None, name))
            {
                continue;
            }
            // `isVariableUsedInNextStmt`: the statements after the `try`.
            let (list, index) = enclosing(ctx.source, tokens(ctx), ancestors);
            if used_in_next(&list, index + 1, try_of(ancestors), name) {
                continue;
            }
            let span = node_span(variable);
            out.push(Report::fixed(
                span,
                format!(
                    "The caught exception `{}` is never used; remove the variable.",
                    &ctx.source[span.start..span.end]
                ),
                Safety::Safe,
                vec![Edit::new(
                    Span::new(node_span(&catch.hint).end, span.end),
                    "",
                )],
            ));
        }
    }
}
