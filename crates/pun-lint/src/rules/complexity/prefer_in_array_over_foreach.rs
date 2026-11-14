use mago_syntax::cst::*;
use pun_problem::{Fix, Safety};

use crate::rules::support::expressions::{is_constant, text};
use crate::rules::support::rewrites::{foreach_list, replace_run};
use crate::rules::support::statement_edits::{HOLDERS, has_equal_variable, holder_list, if_view};
use crate::rules::support::values::native_array;
use crate::syntax::{node_span, same_code, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferInArrayOverForeach;

impl Rule for PreferInArrayOverForeach {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-in-array-over-foreach"
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
        let source = ctx.source;
        let Some(list) = holder_list(source, node, ancestors) else {
            return;
        };
        let mut path = ancestors.to_vec();
        path.push(node);
        for key in 1..list.stmts.len() {
            let Statement::Return(r#return) = list.stmts[key] else {
                continue;
            };
            let Statement::Foreach(foreach) = list.stmts[key - 1] else {
                continue;
            };
            let Some(code) = refactor(ctx, &path, foreach, r#return) else {
                return;
            };
            let Some(edits) = replace_run(ctx, &list, key - 1, key, &code) else {
                return;
            };
            out.push(Report {
                span: node_span(foreach),
                message: "Use `in_array()` instead of this loop.".to_string(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits,
                }),
            });
            return;
        }
    }
}

fn refactor(
    ctx: &Context<'_>,
    path: &[Node<'_, '_>],
    foreach: &Foreach<'_>,
    r#return: &Return<'_>,
) -> Option<String> {
    let source = ctx.source;
    let ForeachTarget::Value(target) = &foreach.target else {
        return None;
    };
    let body = foreach_list(source, foreach)?;
    if body.len() != 1 || !native_array(foreach.expression, path) {
        return None;
    }
    let Some(Statement::If(r#if)) = body.stmts.first() else {
        return None;
    };
    let view = if_view(source, r#if);
    if view.else_.is_some() || !view.elseifs.is_empty() {
        return None;
    }
    let Expression::Binary(condition) = unparenthesize(r#if.condition) else {
        return None;
    };
    let strict = match condition.operator {
        BinaryOperator::Identical(_) => true,
        BinaryOperator::Equal(_) => false,
        _ => return None,
    };
    let value = node_span(unparenthesize(target.value));
    let is_value = |e: &Expression<'_>| {
        matches!(unparenthesize(e), Expression::Variable(_))
            && same_code(source, node_span(unparenthesize(e)), value)
    };
    let compared = if is_value(condition.lhs) {
        condition.rhs
    } else if is_value(condition.rhs) {
        condition.lhs
    } else {
        return None;
    };
    if has_equal_variable(source, Node::Expression(compared), value) {
        return None;
    }
    let Statement::Return(Return {
        value: Some(found), ..
    }) = view.stmts.stmts.first()?
    else {
        return None;
    };
    let bool_of = |e: &Expression<'_>| {
        if is_constant(e, "true") {
            Some(true)
        } else if is_constant(e, "false") {
            Some(false)
        } else {
            None
        }
    };
    let found = bool_of(found)?;
    let otherwise = bool_of(r#return.value?)?;
    if found == otherwise {
        return None;
    }
    let strict = if strict { ", true" } else { "" };
    let call = format!(
        "in_array({}, {}{strict})",
        text(ctx, unparenthesize(compared)),
        text(ctx, unparenthesize(foreach.expression))
    );
    Some(if found {
        format!("return {call};")
    } else {
        format!("return !{call};")
    })
}
