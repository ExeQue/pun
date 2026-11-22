use crate::problem::{Fix, Safety};
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::array::TArray;
use mago_codex::ttype::atomic::array::key::ArrayKey;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::atomic::scalar::string::TStringLiteral;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{
    Access, ArrayAccess, AssignmentOperator, BinaryOperator, Construct, Expression, Literal, Node,
    NodeKind, Statement, UnaryPrefixOperator, Variable,
};

use crate::rules::dead_code::no_implode_ternary::is_empty_array;
use crate::rules::support::class_members::{type_of, walk};
use crate::rules::support::conditions::{comments_between, if_parts, is_stmts_aware};
use crate::rules::support::expressions::{args, call_to};
use crate::rules::support::statement_splice::{Change, Code, NewStatement, run_edits};
use crate::rules::support::statements::{Token, enclosing, previous_token_end, remove, tokens};
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, same_expression, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoRedundantForeachGuard;

const SUPERGLOBALS: &[&[u8]] = &[
    b"$GLOBALS",
    b"$_SERVER",
    b"$_GET",
    b"$_POST",
    b"$_FILES",
    b"$_COOKIE",
    b"$_SESSION",
    b"$_REQUEST",
    b"$_ENV",
];

impl Rule for NoRedundantForeachGuard {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-redundant-foreach-guard"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::If, NodeKind::Foreach])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let source = ctx.source;
        let tokens = tokens(ctx);
        let facts = facts(ctx);
        let edits = match node {
            Node::Foreach(foreach) => {
                let n = ancestors.len();
                if !is_stmts_aware(ancestors[n - 2], &ancestors[..n - 2])
                    || in_if_body(&ancestors[..n - 1])
                {
                    return;
                }
                let (list, index) = enclosing(source, tokens, ancestors);
                if index == 0 || index + 1 != list.stmts.len() || list.nop {
                    return;
                }
                let Statement::If(previous) = list.stmts[index - 1] else {
                    return;
                };
                if !is_empty_guard(source, tokens, &facts, previous, foreach.expression) {
                    return;
                }
                Some(remove(source, tokens, &list, &[index - 1]))
            }
            Node::If(_) => run_edits(source, tokens, ancestors, &|chain| {
                guard(source, tokens, &facts, chain)
            }),
            _ => return,
        };
        let Some(edits) = edits else { return };
        out.push(Report {
            span: node_span(&ancestors[ancestors.len() - 1]),
            message: "A `foreach` over an empty array does nothing; this check is not needed."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}

fn in_if_body(ancestors: &[Node<'_, '_>]) -> bool {
    ancestors
        .iter()
        .rev()
        .find(|a| !matches!(a, Node::Block(_) | Node::Statement(_)))
        .is_some_and(|a| matches!(a, Node::IfStatementBody(_) | Node::IfColonDelimitedBody(_)))
}

fn guard(source: &str, tokens: &[Token], facts: &Facts, chain: &[Node<'_, '_>]) -> Option<Change> {
    let Node::Statement(Statement::If(r#if)) = chain[chain.len() - 1] else {
        return None;
    };
    let parts = if_parts(tokens, r#if);
    if parts.has_else || parts.has_elseifs || parts.stmts.nop {
        return None;
    }
    let [Statement::Foreach(foreach)] = parts.stmts.stmts[..] else {
        return None;
    };
    let (list, index) = enclosing(source, tokens, chain);
    if !is_useless(
        source,
        facts,
        &list.stmts[..index],
        r#if.condition,
        foreach.expression,
    ) {
        return None;
    }
    let start = node_span(r#if).start;
    let mut comments = comments_between(tokens, previous_token_end(source, tokens, start), start);
    let inner = node_span(parts.stmts.stmts[0]);
    comments.extend(comments_between(
        tokens,
        previous_token_end(source, tokens, inner.start),
        inner.start,
    ));
    Some(Change::Replace(vec![NewStatement {
        comments,
        code: Code::Moved(inner),
    }]))
}

fn is_useless(
    source: &str,
    facts: &Facts,
    preceding: &[&Statement<'_>],
    cond: &Expression<'_>,
    items: &Expression<'_>,
) -> bool {
    let items = unparenthesize(items);
    let cond = unparenthesize(cond);
    let at_if = first_equal(source, cond, items).unwrap_or(items);
    if should_skip_items(source, facts, preceding, at_if) {
        return false;
    }
    match cond {
        Expression::Binary(b) if matches!(b.operator, BinaryOperator::And(_)) => {
            matches!(unparenthesize(b.lhs), Expression::Variable(_))
                && same_expression(source, b.lhs, items)
                && is_counter_higher_than_one(source, facts, b.rhs, items)
        }
        Expression::Variable(_)
        | Expression::Access(Access::Property(_) | Access::StaticProperty(_))
            if same_expression(source, cond, items) =>
        {
            native(facts, cond).is_some_and(is_array)
        }
        Expression::Binary(b)
            if matches!(
                b.operator,
                BinaryOperator::NotIdentical(_)
                    | BinaryOperator::NotEqual(_)
                    | BinaryOperator::AngledNotEqual(_)
            ) =>
        {
            (is_empty_array(b.lhs) && same_expression(source, items, b.rhs))
                || (is_empty_array(b.rhs) && same_expression(source, items, b.lhs))
        }
        Expression::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Not(_)) => {
            let Expression::Construct(Construct::Empty(empty)) = unparenthesize(u.operand) else {
                return false;
            };
            same_expression(source, empty.value, items)
                && type_of(facts, empty.value).is_some_and(is_array)
        }
        _ => is_counter_higher_than_one(source, facts, cond, items),
    }
}

fn first_equal<'a>(
    source: &str,
    cond: &'a Expression<'a>,
    items: &Expression<'_>,
) -> Option<&'a Expression<'a>> {
    let mut found = None;
    walk(Node::Expression(cond), &mut |node, _| {
        if found.is_some() {
            return false;
        }
        if let Node::Expression(e) = node
            && !matches!(e, Expression::Parenthesized(_))
            && same_expression(source, e, items)
        {
            found = Some(e);
            return false;
        }
        true
    });
    found
}

fn should_skip_items(
    source: &str,
    facts: &Facts,
    preceding: &[&Statement<'_>],
    items: &Expression<'_>,
) -> bool {
    match items {
        Expression::ArrayAccess(access) => {
            !has_offset(facts, access.array, access.index)
                && !(native(facts, access.array)
                    .is_some_and(mago_codex::ttype::union::TUnion::is_mixed)
                    && type_of(facts, access.array).is_some_and(is_array)
                    && assigned_offset(source, facts, preceding, access))
        }
        Expression::Variable(Variable::Direct(v)) if SUPERGLOBALS.contains(&v.name) => true,
        Expression::Variable(_) => !native(facts, items).is_some_and(is_array),
        _ => false,
    }
}

fn assigned_offset(
    source: &str,
    facts: &Facts,
    preceding: &[&Statement<'_>],
    access: &ArrayAccess<'_>,
) -> bool {
    let key = native(facts, access.index).and_then(constant_key);
    preceding.iter().any(|s| {
        let Statement::Expression(e) = s else {
            return false;
        };
        let Expression::Assignment(a) = unparenthesize(e.expression) else {
            return false;
        };
        let Expression::ArrayAccess(target) = unparenthesize(a.lhs) else {
            return false;
        };
        matches!(a.operator, AssignmentOperator::Assign(_))
            && same_expression(source, target.array, access.array)
            && (same_expression(source, target.index, access.index)
                || (key.is_some() && native(facts, target.index).and_then(constant_key) == key))
    })
}

fn has_offset(facts: &Facts, array: &Expression<'_>, index: &Expression<'_>) -> bool {
    let Some(key) = native(facts, index).and_then(constant_key) else {
        return false;
    };
    let Some(array) = native(facts, array) else {
        return false;
    };
    match array.types.as_ref() {
        [TAtomic::Array(TArray::Keyed(k))] => k
            .known_items
            .as_ref()
            .and_then(|items| items.get(&key))
            .is_some_and(|(optional, _)| !optional),
        [TAtomic::Array(TArray::List(l))] => match key {
            ArrayKey::Integer(i) => usize::try_from(i).is_ok_and(|i| {
                l.known_elements
                    .as_ref()
                    .and_then(|e| e.get(&i))
                    .is_some_and(|(optional, _)| !optional)
            }),
            _ => false,
        },
        _ => false,
    }
}

fn constant_key(t: &TUnion) -> Option<ArrayKey> {
    match t.types.as_ref() {
        [TAtomic::Scalar(TScalar::Integer(i))] => i.get_literal_value().map(ArrayKey::Integer),
        [TAtomic::Scalar(TScalar::String(s))] => match &s.literal {
            Some(TStringLiteral::Value(value)) => Some(ArrayKey::String(*value)),
            _ => None,
        },
        _ => None,
    }
}

fn is_counter_higher_than_one(
    source: &str,
    facts: &Facts,
    cond: &Expression<'_>,
    items: &Expression<'_>,
) -> bool {
    let Expression::Binary(b) = unparenthesize(cond) else {
        return false;
    };
    let (count, number, value) = match b.operator {
        BinaryOperator::GreaterThan(_) => (b.lhs, b.rhs, 0),
        BinaryOperator::GreaterThanOrEqual(_) => (b.lhs, b.rhs, 1),
        BinaryOperator::LessThan(_) => (b.rhs, b.lhs, 0),
        BinaryOperator::LessThanOrEqual(_) => (b.rhs, b.lhs, 1),
        _ => return false,
    };
    if !matches!(unparenthesize(number), Expression::Literal(Literal::Integer(i)) if i.value == Some(value))
    {
        return false;
    }
    let Some(call) = call_to(count, "count") else {
        return false;
    };
    args(call)
        .first()
        .is_some_and(|a| same_expression(source, a.value(), items))
        && native(facts, items).is_some_and(|t| is_array(t) || t.is_never())
}

fn is_empty_guard(
    source: &str,
    tokens: &[Token],
    facts: &Facts,
    r#if: &mago_syntax::cst::If<'_>,
    items: &Expression<'_>,
) -> bool {
    let parts = if_parts(tokens, r#if);
    if parts.has_else || parts.has_elseifs {
        return false;
    }
    let Expression::Construct(Construct::Empty(empty)) = unparenthesize(r#if.condition) else {
        return false;
    };
    if !same_expression(source, empty.value, items) {
        return false;
    }
    let body = match (&parts.stmts.stmts[..], parts.stmts.nop) {
        ([], false) => true,
        ([Statement::Return(r)], false) => r.value.is_none(),
        _ => false,
    };
    body && native(facts, empty.value).is_some_and(is_array)
}

fn native<'f>(facts: &'f Facts, expr: &Expression<'_>) -> Option<&'f TUnion> {
    facts.native_type(node_span(unparenthesize(expr)))
}

fn is_array(t: &TUnion) -> bool {
    !t.types.is_empty() && t.types.iter().all(|a| matches!(a, TAtomic::Array(_)))
}
