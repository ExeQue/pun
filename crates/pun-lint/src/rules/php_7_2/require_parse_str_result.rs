//! Rector 2.8.4's `ParseStrWithResultArgumentRector`.

use mago_syntax::cst::{Call, Expression, Node, NodeKind, Statement};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::append_argument;
use crate::rules::support::expressions::named;
use crate::rules::support::statement_edits::{HOLDERS, holder_list};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct RequireParseStrResult;

impl Rule for RequireParseStrResult {
    fn id(&self) -> &'static str {
        "lint/php-7.2/require-parse-str-result"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70200)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(HOLDERS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let raw = crate::syntax::statements(node);
        if !raw.iter().any(|s| parse_str(s).is_some()) {
            return;
        }
        let Some(list) = holder_list(ctx.source, node, ancestors) else {
            return;
        };
        // `processStrWithResult`: each `parse_str()` with one argument that is followed by a statement.
        let mut key = 0;
        let mut reports = Vec::new();
        let mut has_changed = false;
        while key + 1 < list.stmts.len() {
            let Some(call) = parse_str(list.stmts[key]) else {
                key += 1;
                continue;
            };
            let mut edits = vec![append_argument(ctx.source, &call.argument_list, "$result")];
            get_defined_vars(Node::Statement(list.stmts[key + 1]), &mut edits);
            has_changed |= edits.len() > 1;
            reports.push(Report::fixed(
                node_span(call),
                "`parse_str()` without a result argument is removed in PHP 8.0; pass `$result`.",
                Safety::Unsafe,
                edits,
            ));
            key += 2;
        }
        // Rector keeps the holder only when a `get_defined_vars()` was replaced.
        if has_changed {
            out.extend(reports);
        }
    }
}

/// `shouldSkip`: a `parse_str()` statement without a second argument.
fn parse_str<'a>(stmt: &'a Statement<'a>) -> Option<&'a mago_syntax::cst::FunctionCall<'a>> {
    let Statement::Expression(e) = stmt else {
        return None;
    };
    let Expression::Call(Call::Function(call)) = unparenthesize(e.expression) else {
        return None;
    };
    (named(call, "parse_str") && call.argument_list.arguments.get(1).is_none()).then_some(call)
}

fn get_defined_vars(node: Node<'_, '_>, edits: &mut Vec<Edit>) {
    if let Node::FunctionCall(call) = node
        && named(call, "get_defined_vars")
    {
        edits.push(Edit::new(node_span(call), "$result"));
        return;
    }
    node.visit_children(|child| get_defined_vars(child, edits));
}
