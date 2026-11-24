//! Rector 2.8.4's `MultiExceptionCatchRector`.

use mago_syntax::cst::{Hint, Node, NodeKind, TryCatchClause};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::expressions::text;
use crate::rules::support::statement_edits::{block_list, same_with_comments};
use crate::syntax::{node_span, span};
use crate::{Context, Report, Rule};

pub struct PreferMultiCatch;

impl Rule for PreferMultiCatch {
    fn id(&self) -> &'static str {
        "lint/php-7.1/prefer-multi-catch"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70100)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Try])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Try(r#try) = node else {
            return;
        };
        let catches = r#try.catch_clauses.as_slice();
        let mut from = 0;
        for key in 0..catches.len() {
            // already duplicated catch → remove it and join the type
            if key + 1 < catches.len() && same_body(ctx, &catches[key], &catches[key + 1]) {
                continue;
            }
            if key > from {
                out.push(merge(ctx, &catches[from..=key]));
            }
            from = key + 1;
        }
    }
}

/// `$this->betterStandardPrinter->print($catch->stmts)` of both catches is the same.
fn same_body(ctx: &Context<'_>, a: &TryCatchClause<'_>, b: &TryCatchClause<'_>) -> bool {
    same_with_comments(
        ctx,
        &block_list(ctx.source, &a.block),
        &block_list(ctx.source, &b.block),
    )
}

/// The last catch of the run takes every type of the run and the first catch's variable.
fn merge(ctx: &Context<'_>, run: &[TryCatchClause<'_>]) -> Report {
    let mut types = Vec::new();
    for catch in run {
        flatten(ctx, &catch.hint, &mut types);
    }
    let variable = run[0]
        .variable
        .as_ref()
        .map(|v| format!(" {}", String::from_utf8_lossy(v.name)))
        .unwrap_or_default();
    let last = &run[run.len() - 1];
    let edit = Span::new(
        span(run[0].r#catch.span).start,
        span(last.right_parenthesis).end,
    );
    Report::fixed(
        Span::new(node_span(&run[0]).start, node_span(last).end),
        "These catches run the same code; catch the exceptions together with `|`.",
        Safety::Safe,
        vec![Edit::new(
            edit,
            format!("catch ({}{variable})", types.join("|")),
        )],
    )
}

fn flatten<'s>(ctx: &Context<'s>, hint: &Hint<'_>, out: &mut Vec<&'s str>) {
    match hint {
        Hint::Union(union) => {
            flatten(ctx, union.left, out);
            flatten(ctx, union.right, out);
        }
        hint => out.push(text(ctx, hint)),
    }
}
