//! Rector 2.8.4's `Utf8DecodeEncodeToMbConvertEncodingRector`.

use crate::problem::{Edit, Safety};
use mago_syntax::cst::{Node, NodeKind};

use crate::rules::support::arguments::append_argument;
use crate::rules::support::expressions::named;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoUtf8EncodeDecode;

impl Rule for NoUtf8EncodeDecode {
    fn id(&self) -> &'static str {
        "lint/php-8.2/no-utf8-encode-decode"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80200)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        // `$node->args[1] = ...` (and `[2]`): an argument already there is replaced, a missing one appended.
        let (name, set): (&str, &[&str]) = if named(call, "utf8_decode") {
            ("utf8_decode", &["'ISO-8859-1'"])
        } else if named(call, "utf8_encode") {
            ("utf8_encode", &["'UTF-8'", "'ISO-8859-1'"])
        } else {
            return;
        };
        let list = &call.argument_list;
        let arguments = list.arguments.as_slice();
        let mut edits = vec![Edit::new(node_span(call.function), "mb_convert_encoding")];
        // The arguments to replace, then the ones to add after them (one edit, so the two never meet).
        let present = set.len().min(arguments.len().saturating_sub(1));
        let (replaced, added) = set.split_at(present);
        for (i, code) in replaced.iter().enumerate() {
            let mut code = (*code).to_string();
            if i + 1 == present {
                code.extend(added.iter().map(|a| format!(", {a}")));
            }
            edits.push(Edit::new(node_span(&arguments[i + 1]), code));
        }
        if present == 0 {
            edits.push(append_argument(ctx.source, list, &added.join(", ")));
        }
        out.push(Report::fixed(
            node_span(&node),
            format!("Replace `{name}()`, which PHP 8.2 deprecates, with `mb_convert_encoding()`."),
            Safety::Safe,
            edits,
        ));
    }
}
