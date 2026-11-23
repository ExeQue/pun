//! Rector 2.8.4's `SensitiveDefineRector`.

use mago_syntax::cst::{Node, NodeKind};

use crate::problem::Safety;
use crate::rules::support::arguments::remove_argument;
use crate::rules::support::expressions::named;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoCaseInsensitiveDefine;

impl Rule for NoCaseInsensitiveDefine {
    fn id(&self) -> &'static str {
        "lint/php-7.3/no-case-insensitive-define"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70300)
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
        if !named(call, "define") || call.argument_list.arguments.len() < 3 {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Remove the case-insensitive argument of `define()`: PHP 8.0 ignores it.",
            Safety::Safe,
            vec![remove_argument(&call.argument_list, 2)],
        ));
    }
}
