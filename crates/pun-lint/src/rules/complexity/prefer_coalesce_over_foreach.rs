use mago_syntax::cst::*;
use pun_problem::{Fix, Safety};

use crate::rules::support::expressions::{equal, operand, text};
use crate::rules::support::rewrites::{foreach_list, merge, replace_run};
use crate::rules::support::statement_edits::{HOLDERS, holder_list, if_view};
use crate::rules::support::values::native_array;
use crate::syntax::unparenthesize;
use crate::{Context, Report, Rule};

pub struct PreferCoalesceOverForeach;

enum Inner<'a> {
    Return(&'a Expression<'a>),
    Assign(&'a Assignment<'a>),
}

impl Rule for PreferCoalesceOverForeach {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-null-coalesce"
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
        let mut edits = Vec::new();
        let mut skip_next = false;
        for key in 0..list.stmts.len() {
            if std::mem::take(&mut skip_next) {
                continue;
            }
            let Statement::Foreach(foreach) = list.stmts[key] else {
                continue;
            };
            let ForeachTarget::KeyValue(target) = &foreach.target else {
                continue;
            };
            let Some((condition, inner)) = match_inner(ctx, &path, foreach) else {
                continue;
            };
            let checked = if equal(ctx, condition.lhs, target.key) {
                condition.rhs
            } else if equal(ctx, condition.rhs, target.key) {
                condition.lhs
            } else {
                return;
            };
            let fetch = format!(
                "{}[{}]",
                text(ctx, unparenthesize(foreach.expression)),
                text(ctx, unparenthesize(checked))
            );
            let replaced = match inner {
                Inner::Assign(assign) => {
                    if !equal(ctx, target.value, assign.rhs) {
                        return;
                    }
                    let var = text(ctx, unparenthesize(assign.lhs));
                    replace_run(ctx, &list, key, key, &format!("{var} = {fetch} ?? {var};"))
                }
                Inner::Return(value) => {
                    if !equal(ctx, target.value, value) {
                        return;
                    }
                    let Some(Statement::Return(next)) = list.stmts.get(key + 1) else {
                        continue;
                    };
                    let default = match next.value {
                        Some(v) => operand(ctx, v, 141, None),
                        None => operand(ctx, checked, 141, None),
                    };
                    skip_next = true;
                    replace_run(
                        ctx,
                        &list,
                        key,
                        key + 1,
                        &format!("return {fetch} ?? {default};"),
                    )
                }
            };
            let Some(replaced) = replaced else { return };
            edits.extend(replaced);
        }
        let edits = merge(edits);
        let Some(first) = edits.first() else { return };
        out.push(Report {
            span: first.span,
            message: "Read the array by key with `??` instead of this loop.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}

fn match_inner<'a>(
    ctx: &Context<'_>,
    path: &[Node<'_, '_>],
    foreach: &'a Foreach<'a>,
) -> Option<(&'a Binary<'a>, Inner<'a>)> {
    let body = foreach_list(ctx.source, foreach)?;
    if body.len() != 1 {
        return None;
    }
    let Some(Statement::If(r#if)) = body.stmts.first() else {
        return None;
    };
    let Expression::Binary(condition) = unparenthesize(r#if.condition) else {
        return None;
    };
    if !matches!(condition.operator, BinaryOperator::Identical(_)) {
        return None;
    }
    let view = if_view(ctx.source, r#if);
    if view.stmts.len() != 1 || view.else_.is_some() || !view.elseifs.is_empty() {
        return None;
    }
    if !native_array(foreach.expression, path) {
        return None;
    }
    let inner = match view.stmts.stmts.first()? {
        Statement::Return(Return {
            value: Some(value), ..
        }) => Inner::Return(value),
        Statement::Expression(e) => {
            let Expression::Assignment(assign) = unparenthesize(e.expression) else {
                return None;
            };
            if !matches!(assign.operator, AssignmentOperator::Assign(_))
                || matches!(assign.rhs, Expression::UnaryPrefix(u) if u.operator.is_reference())
                || matches!(
                    unparenthesize(assign.lhs),
                    Expression::ArrayAccess(_) | Expression::ArrayAppend(_)
                )
            {
                return None;
            }
            Inner::Assign(assign)
        }
        _ => return None,
    };
    Some((condition, inner))
}
