//! Rector 2.8.4's `GetHeadersArgumentFormatRector`.

use mago_syntax::cst::{Expression, Literal, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::named;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoIntGetHeadersFormat;

impl Rule for NoIntGetHeadersFormat {
    fn id(&self) -> &'static str {
        "lint/php-8.0/no-int-get-headers-format"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80000)
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
        if !named(call, "get_headers") {
            return;
        }
        let Some(format) = call.argument_list.arguments.get(1) else {
            return;
        };
        let format = value(format);
        // `valueResolver->getValue()` is `0` or `1`.
        let Expression::Literal(Literal::Integer(int)) = unparenthesize(format) else {
            return;
        };
        let code = match int.value {
            Some(0) => "false",
            Some(1) => "true",
            _ => return,
        };
        out.push(Report::fixed(
            node_span(int),
            format!("Pass `{code}` as the format of `get_headers()`: PHP 8.0 takes a `bool`."),
            Safety::Safe,
            vec![Edit::new(node_span(int), code)],
        ));
    }
}
