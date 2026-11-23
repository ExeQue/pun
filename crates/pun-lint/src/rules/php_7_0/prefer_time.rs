//! Rector 2.8.4's `RenameMktimeWithoutArgsToTimeRector`.

use mago_syntax::cst::{Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::expressions::named;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferTime;

impl Rule for PreferTime {
    fn id(&self) -> &'static str {
        "lint/php-7.0/prefer-time"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70000)
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
        if !named(call, "mktime") || !call.argument_list.arguments.is_empty() {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Call `time()` instead of `mktime()` without arguments.",
            Safety::Safe,
            vec![Edit::new(node_span(call.function), "time")],
        ));
    }
}
