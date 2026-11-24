//! Rector 2.8.4's `GetClassOnNullRector`.

use mago_syntax::cst::{FunctionCall, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{Prec, in_parent, named, operand, text};
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoGetClassNull;

impl Rule for NoGetClassNull {
    fn id(&self) -> &'static str {
        "lint/php-7.2/no-get-class-null"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70200)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Class(_) = node else {
            return;
        };
        // The class's nodes in Rector's traversal order; the first ternary stops it.
        let mut calls = Vec::new();
        let mut path = ancestors.to_vec();
        collect(node, &mut path, &mut calls);
        let facts = facts(ctx);
        for (call, chain) in calls {
            let Some(first) = call.argument_list.arguments.first() else {
                continue;
            };
            let arg = value(first);
            // `TypeCombinator::containsNull()`.
            if !facts
                .full_type(node_span(arg))
                .is_some_and(|t| t.has_null())
            {
                continue;
            }
            let code = format!(
                "{} !== null ? get_class{} : self::class",
                operand(ctx, arg, 80, Some(80)),
                text(ctx, &call.argument_list)
            );
            let span = node_span(call);
            out.push(Report::fixed(
                span,
                "`get_class()` throws on `null` since PHP 8.0; check for `null` first.",
                Safety::Unsafe,
                vec![Edit::new(
                    span,
                    in_parent(span, &chain, code, Prec::Infix(150, 150, 150)),
                )],
            ));
        }
    }
}

type Found<'ast, 'arena> = (&'ast FunctionCall<'arena>, Vec<Node<'ast, 'arena>>);

/// Returns `true` once a ternary stops the traversal.
fn collect<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    path: &mut Vec<Node<'ast, 'arena>>,
    out: &mut Vec<Found<'ast, 'arena>>,
) -> bool {
    match node {
        Node::Conditional(_) => return true,
        Node::FunctionCall(call) if named(call, "get_class") => out.push((call, path.clone())),
        _ => {}
    }
    path.push(node);
    let mut stopped = false;
    for child in node.children() {
        if collect(child, path, out) {
            stopped = true;
            break;
        }
    }
    path.pop();
    stopped
}
