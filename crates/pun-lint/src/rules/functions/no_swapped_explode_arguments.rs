use mago_syntax::cst::{Argument, Expression, Literal, LiteralString, Node, NodeKind};

use crate::rules::support::names::function_call_matches;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoSwappedExplodeArguments;

const THRESHOLD: usize = 10;

impl Rule for NoSwappedExplodeArguments {
    fn id(&self) -> &'static str {
        "lint/functions/no-swapped-explode-arguments"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(c) = node else { return };
        if !function_call_matches(ancestors, c, "explode") {
            return;
        }
        let arguments = &c.argument_list.arguments.nodes;
        let (Some(Argument::Positional(separator)), Some(Argument::Positional(string))) =
            (arguments.first(), arguments.get(1))
        else {
            return;
        };
        if separator.ellipsis.is_some() || string.ellipsis.is_some() {
            return;
        }
        let Some(Some(string_value)) = string_literal(string.value).map(|l| l.value) else {
            return;
        };
        if string_value.len() > THRESHOLD {
            return;
        }
        if let Some(Some(separator_value)) = string_literal(separator.value).map(|l| l.value)
            && separator_value.len() <= string_value.len()
        {
            return;
        }
        out.push(Report {
            span: node_span(string.value),
            message: "The arguments to `explode` may be swapped.".to_string(),
            help: Some("Swap the arguments so the separator comes first.".to_string()),
            fix: None,
        });
    }
}

fn string_literal<'a>(expression: &'a Expression<'a>) -> Option<&'a LiteralString<'a>> {
    match expression {
        Expression::Literal(Literal::String(literal)) => Some(literal),
        _ => None,
    }
}
