use mago_syntax::cst::{Node, NodeKind, Statement};
use pun_problem::{Fix, Safety};

use crate::rules::support::foreach_search::{Kind, refactor};
use crate::rules::support::options::part_on;
use crate::rules::support::statement_edits::{HOLDERS, holder_list};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct PreferArrayFind {
    /// `array-find` is left out of the rule's `functions`.
    off: bool,
}

impl Rule for PreferArrayFind {
    fn id(&self) -> &'static str {
        "lint/php-8.4/prefer-array-find"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !functions(options, "array-find")?;
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
        let Some((edits, span)) = refactor(ctx, ancestors, &list, Kind::Find) else {
            return;
        };
        out.push(Report {
            span,
            message: "Use `array_find()` instead of a `foreach` that looks for an item."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}

const FUNCTIONS: [&str; 4] = ["array-all", "array-any", "array-find", "array-find-key"];

/// Pun's `functions`: the rewrites that run, one for each of Rector's four rules.
pub(super) fn functions(options: &serde_json::Value, function: &str) -> Result<bool, OptionsError> {
    part_on(options, "functions", &FUNCTIONS, function)
}
