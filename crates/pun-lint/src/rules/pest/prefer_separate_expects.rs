//! pest-plugin-rector's `ConvertAndToExpectRector`.

use mago_syntax::cst::{Access, Call, Expression, Node, NodeKind, Statement};

use super::{is_expect_chain, is_name, method_call};
use crate::problem::{Edit, Safety, Span};
use crate::rules::support::expressions::text;
use crate::rules::support::statements::line_indent;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferSeparateExpects;

impl Rule for PreferSeparateExpects {
    fn id(&self) -> &'static str {
        "lint/pest/prefer-separate-expects"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Statement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Statement(statement @ Statement::Expression(e)) = node else {
            return;
        };
        let Expression::Call(Call::Method(outer)) = e.expression else {
            return;
        };
        if !is_expect_chain(outer) {
            return;
        }
        let indent = line_indent(ctx.source, node_span(statement).start);
        let mut edits = Vec::new();
        let mut current = e.expression;
        loop {
            current = match unparenthesize(current) {
                Expression::Call(Call::Method(call)) if is_name(&call.method, "and") => {
                    let Some(argument) = call.argument_list.arguments.first() else {
                        return;
                    };
                    edits.push(Edit::new(
                        Span::new(node_span(call.object).end, node_span(call).end),
                        format!(";\n{indent}expect({})", text(ctx, argument.value())),
                    ));
                    call.object
                }
                Expression::Call(Call::Method(call)) => call.object,
                Expression::Access(Access::Property(fetch)) => fetch.object,
                _ => break,
            };
        }
        if edits.is_empty() || method_call(e.expression).is_none() {
            return;
        }
        edits.reverse();
        out.push(Report::fixed(
            node_span(statement),
            "Write each `->and()` subject as its own `expect()` statement.",
            Safety::Safe,
            edits,
        ));
    }
}
