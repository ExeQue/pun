use mago_codex::ttype::atomic::TAtomic;
use mago_syntax::cst::{
    Access, BinaryOperator, Call, Expression, Node, NodeKind, Statement, Variable,
};
use pun_problem::{Edit, Fix, Safety};

use crate::rules::complexity::prefer_empty_array_identical::is_non_typed_from_param;
use crate::rules::dead_code::no_always_false_condition::is_safe;
use crate::rules::support::class_members::walk;
use crate::rules::support::conditions::{comments_between, if_parts};
use crate::rules::support::names::called_function_name;
use crate::rules::support::side_effects::any_node;
use crate::rules::support::statement_splice::{Change, NewStatement, run_edits};
use crate::rules::support::statements::{Token, previous_token_end, tokens};
use crate::rules::types::facts::{Facts, facts, is_builtin_function};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoAlwaysTrueIf;

const RUNTIME_STATE_FUNCTIONS: [&str; 9] = [
    "class_exists",
    "interface_exists",
    "trait_exists",
    "enum_exists",
    "function_exists",
    "method_exists",
    "property_exists",
    "defined",
    "extension_loaded",
];

impl Rule for NoAlwaysTrueIf {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-always-true-if"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::If])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if !matches!(node, Node::If(_)) {
            return;
        }
        let source = ctx.source;
        let tokens = tokens(ctx);
        let facts = facts(ctx);
        let edits = run_edits(source, tokens, ancestors, &|chain| {
            change(source, tokens, &facts, chain)
        });
        if let Some(edits) = edits {
            out.push(Report {
                span: node_span(&ancestors[ancestors.len() - 1]),
                message: "This condition is always true.".to_string(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits,
                }),
            });
        }
    }
}

fn change(source: &str, tokens: &[Token], facts: &Facts, chain: &[Node<'_, '_>]) -> Option<Change> {
    let Node::Statement(Statement::If(r#if)) = chain[chain.len() - 1] else {
        return None;
    };
    let parts = if_parts(tokens, r#if);
    if parts.has_else || parts.has_elseifs {
        return None;
    }
    if any_node(chain[0], &|n| match n {
        Node::Inline(inline) | Node::Statement(Statement::Inline(inline)) => {
            let value = inline.value;
            let value = value
                .strip_prefix(b"\r\n".as_slice())
                .or_else(|| value.strip_prefix(b"\n".as_slice()))
                .unwrap_or(value);
            !value.is_empty()
        }
        _ => false,
    }) {
        return None;
    }
    let cond = unparenthesize(r#if.condition);
    if let Expression::Binary(and) = cond
        && matches!(and.operator, BinaryOperator::And(_))
    {
        let left = unparenthesize(and.lhs);
        let is_true = facts.full_type(node_span(left)).is_some_and(is_true);
        if !is_true || !is_safe(facts, chain, left) {
            return None;
        }
        let right = node_span(unparenthesize(and.rhs));
        let edit = Edit::new(node_span(cond), &source[right.start..right.end]);
        return Some(Change::Edits(vec![edit]));
    }
    if !facts.native_type(node_span(cond)).is_some_and(is_true)
        || should_skip_expr(chain, cond)
        || should_skip_from_variable(facts, cond)
        || writes_variable_variable(chain, node_span(r#if).start)
        || chain.iter().any(|a| matches!(a, Node::Trait(_)))
    {
        return None;
    }
    if parts.stmts.is_empty() {
        return Some(Change::Remove);
    }
    let mut new = NewStatement::all(source, tokens, &parts.stmts);
    let start = node_span(r#if).start;
    let mut comments = comments_between(tokens, previous_token_end(source, tokens, start), start);
    if !comments.is_empty() {
        comments.append(&mut new[0].comments);
        new[0].comments = comments;
    }
    Some(Change::Replace(new))
}

fn is_true(t: &mago_codex::ttype::union::TUnion) -> bool {
    matches!(&t.types[..], [TAtomic::Scalar(s)] if s.is_true())
}

fn should_skip_expr(ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    any_node(Node::Expression(expr), &|n| match n {
        Node::Expression(
            Expression::Access(Access::Property(_) | Access::StaticProperty(_))
            | Expression::ArrayAccess(_)
            | Expression::Call(Call::Method(_) | Call::StaticMethod(_)),
        ) => true,
        Node::Expression(Expression::Call(Call::Function(f))) => {
            called_function_name(ancestors, f.function).is_none_or(|n| {
                RUNTIME_STATE_FUNCTIONS
                    .iter()
                    .any(|r| n.eq_ignore_ascii_case(r))
                    || is_builtin_function(n.trim_start_matches('\\').as_bytes())
            })
        }
        _ => false,
    })
}

fn should_skip_from_variable(facts: &Facts, expr: &Expression<'_>) -> bool {
    any_node(Node::Expression(expr), &|n| {
        matches!(n, Node::Expression(e @ Expression::Variable(_))
            if is_non_typed_from_param(facts, e)
                || facts.native_type(node_span(e)).is_some_and(mago_codex::ttype::union::TUnion::possibly_undefined))
    })
}

fn writes_variable_variable(ancestors: &[Node<'_, '_>], at: usize) -> bool {
    let Some(root) = ancestors.iter().rev().find(|a| {
        matches!(
            a,
            Node::Function(_) | Node::Method(_) | Node::Closure(_) | Node::ArrowFunction(_)
        )
    }) else {
        return false;
    };
    let mut found = false;
    walk(*root, &mut |n, path| {
        if found
            || (!path.is_empty()
                && matches!(
                    n,
                    Node::Function(_) | Node::Closure(_) | Node::ArrowFunction(_)
                ))
        {
            return false;
        }
        if let Node::Assignment(a) = n
            && node_span(a).start < at
            && matches!(
                unparenthesize(a.lhs),
                Expression::Variable(Variable::Indirect(_) | Variable::Nested(_))
            )
        {
            found = true;
        }
        !found
    });
    found
}
