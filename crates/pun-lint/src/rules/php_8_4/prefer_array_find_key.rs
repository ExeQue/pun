use crate::problem::{Fix, Safety};
use mago_syntax::cst::{Node, NodeKind, Statement};

use crate::rules::support::foreach_search::{Kind, refactor};
use crate::rules::support::statement_edits::{HOLDERS, holder_list};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct PreferArrayFindKey {
    /// `array-find-key` is left out of the rule's `functions`.
    off: bool,
}

impl Rule for PreferArrayFindKey {
    fn id(&self) -> &'static str {
        "lint/php-8.4/prefer-array-find"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !super::prefer_array_find::functions(options, "array-find-key")?;
        Ok(())
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
        if self.off {
            return;
        }
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
