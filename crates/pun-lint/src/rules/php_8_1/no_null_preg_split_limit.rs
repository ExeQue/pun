//! Rector 2.8.4's `NullToStrictIntPregSlitFuncCallLimitArgRector`.

use mago_syntax::cst::{Argument, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{is_constant, named};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoNullPregSplitLimit;

impl Rule for NoNullPregSplitLimit {
    fn id(&self) -> &'static str {
        "lint/php-8.1/no-null-preg-split-limit"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80100)
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
        if !named(call, "preg_split") {
            return;
        }
        let arguments = call.argument_list.arguments.as_slice();
        // `resolveArgPosition($args, 'limit', 2)`.
        let position = arguments
            .iter()
            .position(
                |a| matches!(a, Argument::Named(n) if n.name.value.eq_ignore_ascii_case(b"limit")),
            )
            .unwrap_or(2);
        let Some(limit) = arguments.get(position) else {
            return;
        };
        // `getType()->isNull()->yes()`, from the syntax: the `null` constant.
        let limit = value(limit);
        if !is_constant(limit, "null") {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Pass `0`, not `null`, as the limit of `preg_split()`: PHP 8.1 deprecates `null` there.",
            Safety::Safe,
            vec![Edit::new(node_span(limit), "0")],
        ));
    }
}
