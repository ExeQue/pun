use crate::problem::{Fix, Safety};
use mago_syntax::cst::{Argument, ArgumentList, ClassLikeMemberSelector, Node, NodeKind};

use crate::rules::support::arguments::append_argument;
use crate::rules::support::expressions::named;
use crate::rules::support::values::is_object_of;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct AddCsvEscapeArgument;

impl Rule for AddCsvEscapeArgument {
    fn id(&self) -> &'static str {
        "lint/php-8.4/add-csv-escape-argument"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80400)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall, NodeKind::MethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (list, at_most) = match node {
            Node::FunctionCall(call) => {
                let at_most = if named(call, "fputcsv") || named(call, "fgetcsv") {
                    4
                } else if named(call, "str_getcsv") {
                    3
                } else {
                    return;
                };
                (&call.argument_list, at_most)
            }
            Node::MethodCall(call) => {
                let ClassLikeMemberSelector::Identifier(name) = &call.method else {
                    return;
                };
                let at_most = match name.value {
                    b"setCsvControl" | b"fgetcsv" => 2,
                    b"fputcsv" => 3,
                    _ => return,
                };
                if !is_object_of(
                    ancestors,
                    call.object,
                    "SplFileObject",
                    node_span(call).start,
                ) {
                    return;
                }
                (&call.argument_list, at_most)
            }
            _ => return,
        };
        if skip(list, at_most) {
            return;
        }
        out.push(Report {
            span: node_span(&node),
            message: "Add the escape argument to the CSV call, as PHP 8.4 requires.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![append_argument(ctx.source, list, "escape: '\\\\'")],
            }),
        });
    }
}

fn skip(list: &ArgumentList<'_>, at_most: usize) -> bool {
    list.arguments.len() > at_most
        || list
            .arguments
            .iter()
            .any(|a| matches!(a, Argument::Named(n) if n.name.value == b"escape"))
}
