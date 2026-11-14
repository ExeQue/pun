use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{Access, BinaryOperator, Call, Expression, Node, NodeKind, Statement};
use pun_problem::{Fix, Safety};

use super::class_method::type_of;
use super::php_parser::{Token, previous_token_end, tokens};
use super::rector_batch5::{called_function_name, doc_comment, resolve_class_name};
use super::stmt_splice::{Change, Code, NewStatement, run_edits};
use super::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoAlwaysTrueInstanceofAssert;

impl Rule for NoAlwaysTrueInstanceofAssert {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-always-true-instanceof-assert"
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
        if !matches!(node, Node::ExpressionStatement(_)) {
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
                message: "This `assert()` is always true for the value's type.".to_string(),
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
    let Node::Statement(statement @ Statement::Expression(e)) = chain[chain.len() - 1] else {
        return None;
    };
    if !is_dead_instanceof_assert(facts, chain, e.expression) {
        return None;
    }
    let start = node_span(statement).start;
    match doc_comment(source, previous_token_end(source, tokens, start), start) {
        Some(doc) => Some(Change::Replace(vec![NewStatement {
            comments: vec![doc],
            code: Code::Nop,
        }])),
        None => Some(Change::Remove),
    }
}

fn is_dead_instanceof_assert(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
) -> bool {
    let Expression::Call(Call::Function(call)) = expr else {
        return false;
    };
    if !called_function_name(ancestors, call.function)
        .is_some_and(|n| n.eq_ignore_ascii_case("assert"))
    {
        return false;
    }
    let arguments: Vec<_> = call.argument_list.arguments.iter().collect();
    let [argument] = arguments[..] else {
        return false;
    };
    let Expression::Binary(instanceof) = unparenthesize(argument.value()) else {
        return false;
    };
    if !matches!(instanceof.operator, BinaryOperator::Instanceof(_)) {
        return false;
    }
    let value = unparenthesize(instanceof.lhs);
    if !matches!(
        value,
        Expression::Access(Access::Property(_)) | Expression::Variable(_)
    ) {
        return false;
    }
    let Expression::Identifier(name) = instanceof.rhs else {
        return false;
    };
    let wanted = resolve_class_name(ancestors, name);
    let Some(t) = type_of(facts, value) else {
        return false;
    };
    !t.types.is_empty()
        && t.types.iter().all(|a| {
            let own = match a {
                TAtomic::Object(TObject::Named(n)) => n.name,
                TAtomic::Object(TObject::Enum(e)) => e.get_name(),
                _ => return false,
            };
            own.as_bytes().eq_ignore_ascii_case(wanted.as_bytes())
                || facts
                    .codebase
                    .is_instance_of(own.as_bytes(), wanted.as_bytes())
        })
}
