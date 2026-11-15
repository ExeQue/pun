use mago_syntax::cst::{Node, NodeKind, Statement};
use pun_problem::{Fix, Safety};

use crate::rules::support::foreach_search::{Kind, refactor};
use crate::rules::support::statement_edits::{HOLDERS, holder_list};
use crate::{Context, Report, Rule};

pub struct PreferArrayFindKey;

impl Rule for PreferArrayFindKey {
    fn id(&self) -> &'static str {
        "lint/php-8.4/prefer-array-find"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80400)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(HOLDERS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if !crate::syntax::statements(node)
            .iter()
            .any(|s| matches!(s, Statement::Foreach(_)))
        {
            return;
        }
        let Some(list) = holder_list(ctx.source, node, ancestors) else {
            return;
        };
        let Some((edits, span)) = refactor(ctx, ancestors, &list, Kind::FindKey) else {
            return;
        };
        out.push(Report {
            span,
            message: "Use `array_find_key()` instead of a `foreach` that looks for a key."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}
