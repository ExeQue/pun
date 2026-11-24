//! Rector 2.8.4's `IfIssetToCoalescingRector`.

use mago_syntax::cst::{Construct, Expression, If, Node, NodeKind, Statement};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::expressions::{equal, operand};
use crate::rules::support::statement_edits::{HOLDERS, assign, holder_list, if_view};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferCoalesceReturn;

impl Rule for PreferCoalesceReturn {
    fn id(&self) -> &'static str {
        "lint/php-7.0/prefer-coalesce-return"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70000)
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
        if !raw.iter().any(|s| matches!(s, Statement::If(_))) {
            return;
        }
        let source = ctx.source;
        let Some(list) = holder_list(source, node, ancestors) else {
            return;
        };
        for (key, stmt) in list.stmts.iter().enumerate() {
            let Statement::Return(r#return) = stmt else {
                continue;
            };
            let Some(fallback) = r#return.value else {
                continue;
            };
            let Some(Statement::If(prev)) = key.checked_sub(1).map(|k| list.stmts[k]) else {
                continue;
            };
            let Expression::Construct(Construct::Isset(isset)) = unparenthesize(prev.condition)
            else {
                continue;
            };
            let Some(value) = match_bare_if_only_return(ctx, prev) else {
                continue;
            };
            let Some(first) = isset.values.first() else {
                continue;
            };
            if !equal(ctx, value, first) {
                continue;
            }
            if assign(fallback).is_some_and(|a| equal(ctx, value, a.lhs)) {
                continue;
            }
            let span = node_span(prev);
            let rest = &source[span.end..];
            let after = span.end + rest.len() - rest.trim_start().len();
            let code = format!(
                "{} ?? {}",
                operand(ctx, value, 140, Some(140)),
                operand(ctx, fallback, 141, None)
            );
            out.push(Report::fixed(
                Span::new(span.start, node_span(*stmt).end),
                "Return the `??` of the value and the fallback instead of checking `isset()` first.",
                Safety::Safe,
                vec![
                    Edit::new(Span::new(span.start, after), ""),
                    Edit::new(node_span(fallback), code),
                ],
            ));
        }
    }
}

/// `matchBareIfOnlyStmt`, then the value of its `return`.
fn match_bare_if_only_return<'a>(
    ctx: &Context<'_>,
    r#if: &'a If<'a>,
) -> Option<&'a Expression<'a>> {
    let view = if_view(ctx.source, r#if);
    if view.else_.is_some() || !view.elseifs.is_empty() || view.stmts.len() != 1 {
        return None;
    }
    match view.stmts.last() {
        Some(Statement::Return(only)) => only.value,
        _ => None,
    }
}
