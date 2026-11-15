use mago_syntax::cst::{Expression, Node, NodeKind};
use pun_problem::{Edit, Fix, Safety};

use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{args, named};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferRoundModeEnum;

impl Rule for PreferRoundModeEnum {
    fn id(&self) -> &'static str {
        "lint/php-8.4/prefer-round-mode-enum"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80400)
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
        let Node::FunctionCall(c) = node else { return };
        let call = c;
        if !named(call, "round") {
            return;
        }
        let args = args(call);
        let [_, _, mode] = args.as_slice() else {
            return;
        };
        let Expression::ConstantAccess(constant) = value(mode) else {
            return;
        };
        let name = constant.name.value();
        let case = match name.strip_prefix(b"\\").unwrap_or(name) {
            b"PHP_ROUND_HALF_UP" => "HalfAwayFromZero",
            b"PHP_ROUND_HALF_DOWN" => "HalfTowardsZero",
            b"PHP_ROUND_HALF_EVEN" => "HalfEven",
            b"PHP_ROUND_HALF_ODD" => "HalfOdd",
            _ => return,
        };
        let span = node_span(constant);
        out.push(Report {
            span,
            message: "Use the `RoundingMode` enum instead of a `PHP_ROUND_*` constant.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, format!("\\RoundingMode::{case}"))],
            }),
        });
    }
}
