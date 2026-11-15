use mago_syntax::cst::{Expression, Node, NodeKind};
use pun_problem::{Edit, Safety};

use super::object_type::is_object_type;
use super::{is_name, member_name};
use crate::rules::support::expressions::text;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoNegatedWhen;

impl Rule for NoNegatedWhen {
    fn id(&self) -> &'static str {
        "lint/laravel/no-negated-when"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::MethodCall(call) = node else { return };
        let Some(name) = member_name(&call.method) else {
            return;
        };
        if !is_name(name, "when") && !is_name(name, "unless") {
            return;
        }
        let Some(first) = call.argument_list.arguments.first() else {
            return;
        };
        let Expression::UnaryPrefix(not) = unparenthesize(first.value()) else {
            return;
        };
        if !not.operator.is_not() {
            return;
        }
        if !is_object_type(
            ctx,
            ancestors,
            call.object,
            "Illuminate\\Support\\Traits\\Conditionable",
        ) {
            return;
        }
        let to = if name == b"when" { "unless" } else { "when" };
        let argument = node_span(first);
        out.push(Report::fixed(
            node_span(call),
            format!("Use `{to}()` instead of negating its condition."),
            Safety::Unsafe,
            vec![
                Edit::new(node_span(&call.method), to),
                Edit::new(argument, text(ctx, unparenthesize(not.operand))),
            ],
        ));
    }
}
