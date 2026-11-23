use mago_syntax::cst::{ArrayAccess, Expression, Node, NodeKind};

use super::object_type::is_object_type;
use super::{call, is_by_ref_argument, is_written, replace};
use crate::rules::support::expressions::text;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

/// The methods that replace a read, an assignment, an `isset()` and an `unset()` of an offset on an object of
/// `contract`.
struct Methods {
    contract: &'static str,
    read: &'static str,
    assign: &'static str,
    isset: &'static str,
    unset: Option<&'static str>,
}

const CONTAINER: &[Methods] = &[Methods {
    contract: "Illuminate\\Contracts\\Container\\Container",
    read: "make",
    assign: "bind",
    isset: "bound",
    unset: None,
}];

const REPOSITORIES: &[Methods] = &[
    Methods {
        contract: "Illuminate\\Contracts\\Config\\Repository",
        read: "get",
        assign: "set",
        isset: "has",
        unset: Some("set"),
    },
    Methods {
        contract: "Illuminate\\Contracts\\Cache\\Repository",
        read: "get",
        assign: "set",
        isset: "has",
        unset: Some("forget"),
    },
];

const TARGETS: &[NodeKind] = &[
    NodeKind::Assignment,
    NodeKind::IssetConstruct,
    NodeKind::Unset,
    NodeKind::ArrayAccess,
];

pub struct PreferContainerMethodCall;

impl Rule for PreferContainerMethodCall {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-container-method-call"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(TARGETS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        refactor(CONTAINER, node, ancestors, ctx, out);
    }
}

pub struct PreferRepositoryMethodCall;

impl Rule for PreferRepositoryMethodCall {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-repository-method-call"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(TARGETS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        refactor(REPOSITORIES, node, ancestors, ctx, out);
    }
}

fn refactor(
    types: &[Methods],
    node: Node<'_, '_>,
    ancestors: &[Node<'_, '_>],
    ctx: &Context<'_>,
    out: &mut Vec<Report>,
) {
    let matched = |access: &ArrayAccess<'_>| {
        types
            .iter()
            .find(|m| is_object_type(ctx, ancestors, access.array, m.contract))
    };
    let (access, method, value, statement) = match node {
        Node::Assignment(a) if a.operator.is_assign() && !a.rhs.is_reference() => {
            let Expression::ArrayAccess(access) = unparenthesize(a.lhs) else {
                return;
            };
            let Some(methods) = matched(access) else {
                return;
            };
            (access, methods.assign, Some(a.rhs), false)
        }
        Node::IssetConstruct(i) => {
            let values: Vec<_> = i.values.iter().collect();
            let Some(access) = single(&values) else {
                return;
            };
            let Some(methods) = matched(access) else {
                return;
            };
            (access, methods.isset, None, false)
        }
        Node::Unset(u) => {
            let values: Vec<_> = u.values.iter().collect();
            let Some(access) = single(&values) else {
                return;
            };
            let Some(method) = matched(access).and_then(|m| m.unset) else {
                return;
            };
            (access, method, None, true)
        }
        Node::ArrayAccess(access) => {
            // writes are handled through their parent node
            let span = node_span(access);
            if is_written(span, ancestors)
                || is_by_ref_argument(ctx, span, ancestors)
                || in_simple_interpolation(ancestors)
            {
                return;
            }
            let Some(methods) = matched(access) else {
                return;
            };
            (access, methods.read, None, false)
        }
        _ => return,
    };
    let mut args = vec![text(ctx, access.index)];
    args.extend(value.map(|v| text(ctx, v)));
    let mut code = call(&format!("{}->{method}", text(ctx, access.array)), &args);
    if statement {
        code.push(';');
    }
    out.push(replace(
        node_span(&node),
        format!("Call `{method}()` instead of using array access."),
        code,
    ));
}

/// A call can be written in a `{$...}` part of a string but not in a `$var[key]` one, where Rector writes the key as
/// a constant.
fn in_simple_interpolation(ancestors: &[Node<'_, '_>]) -> bool {
    ancestors
        .iter()
        .rev()
        .find(|a| matches!(a, Node::StringPart(_) | Node::BracedExpressionStringPart(_)))
        .is_some_and(|a| {
            matches!(
                a,
                Node::StringPart(mago_syntax::cst::StringPart::Expression(_))
            )
        })
}

fn single<'a>(values: &[&&'a Expression<'a>]) -> Option<&'a ArrayAccess<'a>> {
    match values {
        [Expression::ArrayAccess(access)] => Some(access),
        _ => None,
    }
}
