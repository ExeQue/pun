//! Rector 2.8.4's `ReplaceEachAssignmentWithKeyCurrentRector`.

use mago_syntax::cst::{Call, Expression, Node, NodeKind, Statement};

use super::no_list_each::replace_with_statements;
use crate::problem::Safety;
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{named, text};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoEach;

impl Rule for NoEach {
    fn id(&self) -> &'static str {
        "lint/php-7.2/no-each"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70200)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Statement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Statement(statement @ Statement::Expression(e)) = node else {
            return;
        };
        let Expression::Assignment(assign) = unparenthesize(e.expression) else {
            return;
        };
        // `shouldSkip`.
        let Expression::Call(Call::Function(each)) = unparenthesize(assign.rhs) else {
            return;
        };
        if !assign.operator.is_assign()
            || !named(each, "each")
            || matches!(assign.lhs, Expression::List(_) | Expression::Array(_))
        {
            return;
        }
        let Some(first) = each.argument_list.arguments.first() else {
            return;
        };
        let (var, eached) = (text(ctx, assign.lhs), text(ctx, value(first)));
        // `createNewStmts`.
        let stmts = [
            format!("{var}[1] = current({eached});"),
            format!("{var}['value'] = current({eached});"),
            format!("{var}[0] = key({eached});"),
            format!("{var}['key'] = key({eached});"),
            format!("next({eached});"),
        ];
        out.push(Report::fixed(
            node_span(statement),
            "`each()` is removed in PHP 8.0; build its pair with `current()`, `key()` and `next()`.",
            Safety::Safe,
            replace_with_statements(ctx, statement, ancestors, &stmts),
        ));
    }
}
