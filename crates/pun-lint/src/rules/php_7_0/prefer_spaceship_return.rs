//! Rector 2.8.4's `IfToSpaceshipRector`, with `BattleshipTernaryAnalyzer`.

use mago_syntax::cst::{BinaryOperator, Conditional, Expression, If, Node, NodeKind, Statement};

use super::prefer_spaceship::{are_values, spaceship};
use crate::problem::{Edit, Safety, Span};
use crate::rules::support::expressions::equal;
use crate::rules::support::statement_edits::{HOLDERS, List, holder_list, if_view};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferSpaceshipReturn;

const MESSAGE: &str =
    "Return the spaceship operator `<=>` instead of comparing in an `if` and a ternary.";

impl Rule for PreferSpaceshipReturn {
    fn id(&self) -> &'static str {
        "lint/php-7.0/prefer-spaceship-return"
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
        let Some(list) = holder_list(ctx.source, node, ancestors) else {
            return;
        };
        let mut paired = Vec::new();
        for (key, stmt) in list.stmts.iter().enumerate() {
            let Statement::Return(r#return) = stmt else {
                continue;
            };
            let Some(Expression::Conditional(ternary)) = r#return.value.map(unparenthesize) else {
                continue;
            };
            // preceded by if
            let Some(Statement::If(prev)) = key.checked_sub(1).map(|k| list.stmts[k]) else {
                continue;
            };
            let Some(compared) = match_expr_compared_exprs_return_zero(ctx, prev) else {
                continue;
            };
            let Some(code) =
                create_return_spaceship(ctx, battleship(ctx, ternary, compared), compared)
            else {
                continue;
            };
            paired.push(key - 1);
            let span = Span::new(node_span(prev).start, node_span(*stmt).end);
            let mut edits = vec![Edit::new(span, code)];
            // Rector returns the closure, so its printer writes the closure's `function (` again.
            if let Some(Node::Closure(closure)) = ancestors.last() {
                let gap = Span::new(
                    node_span(&closure.function).end,
                    node_span(&closure.parameter_list).start,
                );
                if &ctx.source[gap.start..gap.end] != " " && closure.ampersand.is_none() {
                    edits.push(Edit::new(gap, " "));
                }
            }
            out.push(Report::fixed(span, MESSAGE, Safety::Safe, edits));
        }
        // `refactorIf`: the `if` is a statement holder itself, visited after the list that holds it.
        for (key, stmt) in list.stmts.iter().enumerate() {
            if let Statement::If(r#if) = stmt
                && !paired.contains(&key)
                && let Some(code) = refactor_if(ctx, r#if)
            {
                out.push(Report::fixed(
                    node_span(*stmt),
                    MESSAGE,
                    Safety::Safe,
                    vec![Edit::new(node_span(*stmt), code)],
                ));
            }
        }
    }
}

type Compared<'a> = (&'a Expression<'a>, &'a Expression<'a>);

fn refactor_if(ctx: &Context<'_>, r#if: &If<'_>) -> Option<String> {
    let view = if_view(ctx.source, r#if);
    if !view.elseifs.is_empty() {
        return None;
    }
    let (r#else, _) = view.else_.as_ref()?;
    let compared = match_expr_compared_exprs_return_zero(ctx, r#if)?;
    let ternary = only_return_ternary(r#else)?;
    create_return_spaceship(ctx, battleship(ctx, ternary, compared), compared)
}

fn match_expr_compared_exprs_return_zero<'a>(
    ctx: &Context<'_>,
    r#if: &'a If<'a>,
) -> Option<Compared<'a>> {
    let Expression::Binary(binary) = unparenthesize(r#if.condition) else {
        return None;
    };
    if !matches!(
        binary.operator,
        BinaryOperator::Equal(_) | BinaryOperator::Identical(_)
    ) {
        return None;
    }
    let view = if_view(ctx.source, r#if);
    if view.stmts.len() != 1 {
        return None;
    }
    let Some(Statement::Return(only)) = view.stmts.last() else {
        return None;
    };
    if !are_values(&[only.value], [0]) {
        return None;
    }
    Some((binary.lhs, binary.rhs))
}

fn only_return_ternary<'a>(r#else: &List<'a>) -> Option<&'a Conditional<'a>> {
    if r#else.len() != 1 {
        return None;
    }
    let Some(Statement::Return(only)) = r#else.last() else {
        return None;
    };
    match only.value.map(unparenthesize) {
        Some(Expression::Conditional(ternary)) => Some(ternary),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Order {
    Asc,
    Desc,
}

fn create_return_spaceship(
    ctx: &Context<'_>,
    order: Option<Order>,
    (first, second): Compared<'_>,
) -> Option<String> {
    let code = match order? {
        Order::Desc => spaceship(ctx, first, second),
        Order::Asc => spaceship(ctx, second, first),
    };
    Some(format!("return {code};"))
}

/// `BattleshipTernaryAnalyzer::isGreaterLowerCompareReturnOneAndMinusOne`.
fn battleship(
    ctx: &Context<'_>,
    ternary: &Conditional<'_>,
    (first, second): Compared<'_>,
) -> Option<Order> {
    let Expression::Binary(cond) = unparenthesize(ternary.condition) else {
        return None;
    };
    let greater = match cond.operator {
        BinaryOperator::GreaterThan(_) => true,
        BinaryOperator::LessThan(_) => false,
        _ => return None,
    };
    ternary.then?;
    let same = if equal(ctx, cond.lhs, first) && equal(ctx, cond.rhs, second) {
        true
    } else if equal(ctx, cond.rhs, first) && equal(ctx, cond.lhs, second) {
        false
    } else {
        return None;
    };
    // `evaluateGreater` reads a matching `>` as descending, `evaluateSmaller` a matching `<` as ascending.
    let desc = greater == same;
    let order = |one_first: Order, minus_first: Order| {
        if are_values(&[ternary.then, Some(ternary.r#else)], [1, -1]) {
            Some(one_first)
        } else if are_values(&[Some(ternary.r#else), ternary.then], [1, -1]) {
            Some(minus_first)
        } else {
            None
        }
    };
    if desc {
        // `evaluateTernaryDesc`.
        order(Order::Desc, Order::Asc)
    } else {
        // `evaluateTernaryAsc`.
        order(Order::Asc, Order::Desc)
    }
}
