//! Rector 2.8.4's `FilterVarToAddSlashesRector`.

use mago_syntax::cst::{Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::{remove_argument, value};
use crate::rules::support::expressions::{is_constant, named};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoFilterMagicQuotes;

impl Rule for NoFilterMagicQuotes {
    fn id(&self) -> &'static str {
        "lint/php-7.4/no-filter-magic-quotes"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        let list = &call.argument_list;
        if !named(call, "filter_var")
            || !list
                .arguments
                .get(1)
                .is_some_and(|a| is_constant(value(a), "FILTER_SANITIZE_MAGIC_QUOTES"))
        {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Replace `filter_var()` with `FILTER_SANITIZE_MAGIC_QUOTES`, which PHP 8.0 removes, with `addslashes()`.",
            Safety::Safe,
            vec![
                Edit::new(node_span(call.function), "addslashes"),
                remove_argument(list, 1),
            ],
        ));
    }
}
