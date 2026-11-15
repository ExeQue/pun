use mago_syntax::cst::{
    ArrayAccess, Expression, Literal, LiteralStringKind, Node, NodeKind, Variable,
};

use super::{in_string, in_superglobal_assign, is_by_ref_argument, is_written, parent, replace};
use crate::rules::complexity::prefer_absolute_include::print_string;
use crate::rules::support::expressions::{Prec, in_parent};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const FACADE: &str = "\\Illuminate\\Support\\Facades\\Request";

pub struct PreferRequestFacade;

impl Rule for PreferRequestFacade {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-request-facade"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::ArrayAccess,
            NodeKind::Variable,
            NodeKind::IssetConstruct,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let span = node_span(&node);
        if in_superglobal_assign(ancestors)
            || is_written(span, ancestors)
            || is_by_ref_argument(ctx, span, ancestors)
            || in_string(ancestors)
        {
            return;
        }
        let (method, code) = match node {
            Node::ArrayAccess(access) => {
                if let Some((_, Node::ArrayAccess(outer))) = parent(span, ancestors)
                    && node_span(unparenthesize(outer.array)) == span
                    && find_all_keys(outer).is_some()
                {
                    return;
                }
                let Some((key, method)) = find_all_keys(access) else {
                    return;
                };
                (method, format!("{FACADE}::{method}({})", quote(&key)))
            }
            Node::Variable(Variable::Direct(v)) => {
                if ancestors.iter().any(|a| match a {
                    Node::ArrayAccess(a) => !is_scalar(a.index),
                    Node::ArrayAppend(_) => true,
                    _ => false,
                }) {
                    return;
                }
                if let Some((_, Node::ArrayAccess(outer))) = parent(span, ancestors)
                    && node_span(unparenthesize(outer.array)) == span
                    && find_all_keys(outer).is_some()
                {
                    return;
                }
                let method = match v.name {
                    b"$_GET" => "query",
                    b"$_POST" => "post",
                    b"$_REQUEST" => "all",
                    _ => return,
                };
                (method, format!("{FACADE}::{method}()"))
            }
            Node::IssetConstruct(isset) => {
                let [value] = isset.values.nodes else {
                    return;
                };
                let Expression::ArrayAccess(access) = unparenthesize(value) else {
                    return;
                };
                if getter(access.array).is_none() {
                    return;
                }
                let Some((key, method)) = find_all_keys(access) else {
                    return;
                };
                if method == "input" {
                    ("exists", format!("{FACADE}::exists({})", quote(&key)))
                } else {
                    let code = format!("{FACADE}::{method}({}) !== null", quote(&key));
                    (
                        method,
                        in_parent(span, ancestors, code, Prec::Infix(80, 80, 80)),
                    )
                }
            }
            _ => return,
        };
        out.push(replace(
            span,
            format!("Use `Request::{method}()` instead of reading the request superglobals."),
            code,
        ));
    }
}

fn getter(expr: &Expression<'_>) -> Option<&'static str> {
    match unparenthesize(expr) {
        Expression::Variable(Variable::Direct(v)) => match v.name {
            b"$_GET" => Some("query"),
            b"$_POST" => Some("post"),
            b"$_REQUEST" => Some("input"),
            _ => None,
        },
        _ => None,
    }
}

fn is_scalar(expr: &Expression<'_>) -> bool {
    matches!(
        unparenthesize(expr),
        Expression::Literal(Literal::String(_) | Literal::Integer(_))
            | Expression::MagicConstant(_)
            | Expression::CompositeString(_)
    )
}

fn find_all_keys(access: &ArrayAccess<'_>) -> Option<(String, &'static str)> {
    let value = match unparenthesize(access.index) {
        Expression::Literal(Literal::String(s)) => String::from_utf8_lossy(s.value?).into_owned(),
        Expression::Literal(Literal::Integer(i)) => i.value?.to_string(),
        _ => return None,
    };
    if let Expression::ArrayAccess(inner) = unparenthesize(access.array) {
        let (key, method) = find_all_keys(inner)?;
        return Some((format!("{key}.{value}"), method));
    }
    Some((value, getter(access.array)?))
}

fn quote(value: &str) -> String {
    print_string(LiteralStringKind::SingleQuoted, value.as_bytes()).unwrap()
}
