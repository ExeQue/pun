//! Rector 2.8.4's `RemoveReflectionSetAccessibleCallsRector`.

use mago_syntax::cst::{Call, ClassLikeMemberSelector, Expression, Node, NodeKind, Statement};

use crate::problem::Safety;
use crate::rules::support::statement_edits::{enclosing, remove};
use crate::rules::support::values::is_object_of;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoReflectionSetAccessible;

impl Rule for NoReflectionSetAccessible {
    fn id(&self) -> &'static str {
        "lint/php-8.1/no-reflection-set-accessible"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80100)
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
        let Expression::Call(Call::Method(call)) = e.expression else {
            return;
        };
        let ClassLikeMemberSelector::Identifier(name) = &call.method else {
            return;
        };
        if !name.value.eq_ignore_ascii_case(b"setAccessible") {
            return;
        }
        let at = node_span(call).start;
        if !is_object_of(ancestors, call.object, "ReflectionProperty", at)
            && !is_object_of(ancestors, call.object, "ReflectionMethod", at)
        {
            return;
        }
        let (list, index) = enclosing(ctx.source, statement, ancestors);
        out.push(Report::fixed(
            node_span(statement),
            "Remove the `setAccessible()` call, which has no effect since PHP 8.1.",
            Safety::Safe,
            remove(ctx, &list, &[index]),
        ));
    }
}
