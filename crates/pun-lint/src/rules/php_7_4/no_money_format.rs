//! Rector 2.8.4's `MoneyFormatToNumberFormatRector`.

use mago_syntax::cst::{Argument, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{named, text};
use crate::rules::support::values::{Value, value as literal};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoMoneyFormat;

impl Rule for NoMoneyFormat {
    fn id(&self) -> &'static str {
        "lint/php-7.4/no-money-format"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70400)
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
        if !named(call, "money_format") {
            return;
        }
        let args = call.argument_list.arguments.as_slice();
        if args.iter().any(|a| matches!(a, Argument::Named(_))) {
            return;
        }
        let [format, number, ..] = args else {
            return;
        };
        if literal(value(format)) != Some(Value::Str("%i".to_string())) {
            return;
        }
        out.push(Report::fixed(
            node_span(call),
            "`money_format()` is removed in PHP 8.0; use `number_format()`.",
            Safety::Unsafe,
            vec![Edit::new(
                node_span(call),
                format!(
                    "number_format(round({}, 2, PHP_ROUND_HALF_ODD), 2, '.', '')",
                    text(ctx, value(number))
                ),
            )],
        ));
    }
}
