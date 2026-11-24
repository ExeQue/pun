//! rector-phpunit's `AssertRegExpRector` (PHPUnit 9.1): `assertSame(1, preg_match($regex, $subject))` and its
//! `assertEquals()` / `assertNotSame()` / `assertNotEquals()` and `0` / `true` / `false` forms become
//! `assertMatchesRegularExpression()` or `assertDoesNotMatchRegularExpression()`.

use crate::problem::Safety;
use mago_syntax::cst::{Call, Expression, Literal, Node, NodeKind, Variable};

use super::{AnyCall, NewArg, expr_text, is_test_case_call_named, rewrite_args};
use crate::rules::support::class_members::walk;
use crate::rules::support::expressions::{call_to, is_constant};
use crate::rules::support::statement_edits::enclosing;
use crate::syntax::unparenthesize;
use crate::{Context, Report, Rule};

const NAMES: [&str; 4] = [
    "assertSame",
    "assertEquals",
    "assertNotSame",
    "assertNotEquals",
];

pub struct PreferAssertMatchesRegularExpression;

impl Rule for PreferAssertMatchesRegularExpression {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-assert-matches-regular-expression"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ExpressionStatement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (Node::ExpressionStatement(statement), [.., Node::Statement(stmt)]) = (node, ancestors)
        else {
            return;
        };
        let expr = statement.expression;
        let call_node = match expr {
            Expression::Call(Call::Method(m)) => Node::MethodCall(m),
            Expression::Call(Call::StaticMethod(s)) => Node::StaticMethodCall(s),
            _ => return,
        };
        let call = AnyCall::of(call_node).unwrap();
        let args = call.args();
        if !call.is_name(&NAMES) || args.len() < 2 {
            return;
        }
        let Some(preg_match) = call_to(args[1].value(), "preg_match") else {
            return;
        };
        let mut chain = ancestors.to_vec();
        chain.push(node);
        if !is_test_case_call_named(ctx, &chain, &call, &NAMES) {
            return;
        }
        let preg_args: Vec<_> = preg_match.argument_list.arguments.iter().collect();
        if preg_args.len() < 2 {
            return;
        }
        // the matches stay in use after the assert
        if let Some(Expression::Variable(Variable::Direct(matches))) =
            preg_args.get(2).map(|a| unparenthesize(a.value()))
        {
            let (list, index) = enclosing(ctx.source, stmt, &ancestors[..ancestors.len() - 1]);
            let mut used = false;
            for next in &list.stmts[index + 1..] {
                walk(Node::Statement(next), &mut |n, _| {
                    used |= matches!(n, Node::DirectVariable(v) if v.name == matches.name);
                    !used
                });
            }
            if used {
                return;
            }
        }
        let Some(condition) = old_condition(args[0].value()) else {
            return;
        };
        let positive = call.is_name(&["assertSame", "assertEquals"]);
        let name = match (positive, condition) {
            (true, 1) | (false, 0) => Some("assertMatchesRegularExpression"),
            (true, 0) | (false, 1) => Some("assertDoesNotMatchRegularExpression"),
            _ => None,
        };
        let mut new = vec![
            NewArg::New(expr_text(ctx, preg_args[0].value()).to_string()),
            NewArg::New(expr_text(ctx, preg_args[1].value()).to_string()),
        ];
        new.extend((2..args.len()).map(NewArg::Old));
        out.push(Report::fixed(
            call.span,
            "Use `assertMatchesRegularExpression()` instead of asserting `preg_match()`'s result.",
            Safety::Unsafe,
            rewrite_args(ctx, &call, name, &new),
        ));
    }
}

/// `resolveOldCondition()`: an integer as it is, `true` 1, another constant 0. Rector fails on anything else.
fn old_condition(expr: &Expression<'_>) -> Option<i64> {
    match unparenthesize(expr) {
        Expression::Literal(Literal::Integer(i)) => i64::try_from(i.value?).ok(),
        Expression::Literal(Literal::True(_)) => Some(1),
        Expression::Literal(Literal::False(_) | Literal::Null(_)) => Some(0),
        e @ Expression::ConstantAccess(_) => Some(i64::from(is_constant(e, "true"))),
        _ => None,
    }
}
