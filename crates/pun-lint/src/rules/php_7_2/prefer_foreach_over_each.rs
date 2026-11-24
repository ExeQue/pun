//! Rector 2.8.4's `WhileEachToForeachRector`.

use mago_syntax::cst::{Expression, Node, NodeKind, Statement, WhileBody};

use super::no_list_each::match_list_and_each;
use crate::problem::{Edit, Safety, Span};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::text;
use crate::rules::support::statement_edits::line_indent;
use crate::syntax::{node_span, span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferForeachOverEach;

impl Rule for PreferForeachOverEach {
    fn id(&self) -> &'static str {
        "lint/php-7.2/prefer-foreach-over-each"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70200)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::While])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::While(r#while) = node else {
            return;
        };
        let Expression::Assignment(assign) = unparenthesize(r#while.condition) else {
            return;
        };
        let Some((mut items, each)) = match_list_and_each(assign) else {
            return;
        };
        let Some(first) = each.argument_list.arguments.first() else {
            return;
        };
        let arg = text(ctx, value(first));
        let first = text(ctx, first);
        let mut foreached = if items.len() == 1 {
            format!("array_keys({first})")
        } else {
            arg.to_string()
        };
        let mut trailing_comma_last = false;
        let item = match items.pop() {
            Some(Some(item)) => item,
            Some(None) => {
                foreached = format!("array_keys({first})");
                trailing_comma_last = true;
                match items.first() {
                    Some(Some(item)) => *item,
                    _ => return,
                }
            }
            None => return,
        };
        // is key included? add it to foreach
        let key = match items.pop() {
            Some(Some(key)) if !trailing_comma_last => format!("{} => ", text(ctx, key)),
            _ => String::new(),
        };
        let header = format!("foreach ({foreached} as {key}{})", text(ctx, item));
        let start = node_span(&r#while.r#while).start;
        // The body is printed as a block, `{` after the header.
        let edits = match &r#while.body {
            WhileBody::Statement(Statement::Block(block)) => vec![Edit::new(
                Span::new(start, span(block.left_brace).end),
                format!("{header} {{"),
            )],
            WhileBody::Statement(statement) => {
                let indent = line_indent(ctx.source, start);
                vec![Edit::new(
                    Span::new(start, node_span(*statement).end),
                    format!(
                        "{header} {{\n{indent}    {}\n{indent}}}",
                        text(ctx, *statement)
                    ),
                )]
            }
            WhileBody::ColonDelimited(body) => vec![
                Edit::new(
                    Span::new(start, span(body.colon).end),
                    format!("{header} {{"),
                ),
                Edit::new(
                    Span::new(
                        node_span(&body.end_while).start,
                        node_span(&body.terminator).end,
                    ),
                    "}",
                ),
            ],
        };
        out.push(Report::fixed(
            node_span(r#while),
            "`each()` is removed in PHP 8.0; loop with `foreach` instead.",
            Safety::Safe,
            edits,
        ));
    }
}
