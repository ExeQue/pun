//! Rector 2.8.4's `RestoreIncludePathToIniRestoreRector`.

use mago_syntax::cst::{Node, NodeKind};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::expressions::named;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoRestoreIncludePath;

impl Rule for NoRestoreIncludePath {
    fn id(&self) -> &'static str {
        "lint/php-7.4/no-restore-include-path"
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
        if !named(call, "restore_include_path") {
            return;
        }
        let list = &call.argument_list;
        // `$node->args[0] = new Arg(new String_('include_path'))`: the first argument is replaced, or added.
        let first = match list.arguments.first() {
            Some(a) => node_span(a),
            None => {
                let at = list.left_parenthesis.end.offset as usize;
                Span::new(at, at)
            }
        };
        out.push(Report::fixed(
            node_span(&node),
            "Replace `restore_include_path()`, which PHP 8.0 removes, with `ini_restore('include_path')`.",
            Safety::Safe,
            vec![
                Edit::new(node_span(call.function), "ini_restore"),
                Edit::new(first, "'include_path'"),
            ],
        ));
    }
}
