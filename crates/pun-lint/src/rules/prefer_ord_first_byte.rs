use mago_syntax::cst::{Expression, Literal, Node, NodeKind};
use pun_problem::{Edit, Fix, Safety};

use super::lint_b_helpers::{Constant, constant, dereferenceable, native, value};
use super::rector_code_quality::{args, named};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferOrdFirstByte;

impl Rule for PreferOrdFirstByte {
    fn id(&self) -> &'static str {
        "lint/php-8.5/prefer-ord-first-byte"
    }

    fn min_php_version(&self) -> u32 {
        80500
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(c) = node else { return };
        let call = c;
        if !named(call, "ord") {
            return;
        }
        let Some(arg) = args(call).first().map(|a| value(a)) else {
            return;
        };
        let at = node_span(call).start;
        let expr = crate::syntax::unparenthesize(arg);
        let int_literal = match expr {
            Expression::Literal(Literal::String(s)) if s.value.is_some_and(|v| v.len() == 1) => {
                return;
            }
            Expression::Literal(Literal::Integer(i)) => {
                let value = i.value.unwrap();
                if value.to_string().len() == 1 {
                    return;
                }
                Some(value)
            }
            _ => None,
        };
        if native(ancestors, expr, at).is_none() {
            return;
        }
        let new = match int_literal {
            Some(v) => v.to_string()[..1].to_string(),
            None => {
                if matches!(expr, Expression::ArrayAccess(_))
                    || matches!(constant(ancestors, expr, at), Some(Constant::Int(_)))
                {
                    return;
                }
                format!("{}[0]", dereferenceable(ctx.source, expr))
            }
        };
        out.push(Report {
            span: node_span(arg),
            message: "Pass `ord()` a single byte.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(node_span(expr), new)],
            }),
        });
    }
}
