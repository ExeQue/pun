//! Rector 2.8.4's `ListSplitStringRector`.

use mago_syntax::cst::{Expression, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::expressions::text;
use crate::rules::types::compare::mapper::expr_type;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoListFromString;

impl Rule for NoListFromString {
    fn id(&self) -> &'static str {
        "lint/php-7.0/no-list-from-string"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70000)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Assignment])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Assignment(assign) = node else {
            return;
        };
        if !assign.operator.is_assign()
            || !matches!(assign.lhs, Expression::List(_) | Expression::Array(_))
        {
            return;
        }
        let expr = unparenthesize(assign.rhs);
        if !expr_type(&facts(ctx), ancestors, expr, false)
            .is_string()
            .yes()
        {
            return;
        }
        out.push(Report::fixed(
            node_span(expr),
            "`list()` cannot split a string since PHP 7.0; split it with `str_split()`.",
            Safety::Safe,
            vec![Edit::new(
                node_span(expr),
                format!("str_split({})", text(ctx, expr)),
            )],
        ));
    }
}
