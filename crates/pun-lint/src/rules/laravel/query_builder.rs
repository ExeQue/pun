use mago_syntax::cst::{Expression, FunctionLikeParameter, Hint, Node};
use pun_problem::Safety;

use super::object_type::{is_object_type, name_class, object_class, special_class};
use super::print::set_param_type;
use super::{is_a, is_name, member_name};
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report};

const MODEL: &str = "Illuminate\\Database\\Eloquent\\Model";

pub(crate) struct CallParts<'a> {
    pub on: &'a Expression<'a>,
    pub is_static: bool,
    pub name: &'a [u8],
    pub arguments: Vec<&'a Expression<'a>>,
}

pub(crate) fn call_parts<'a>(n: &Node<'a, 'a>) -> Option<CallParts<'a>> {
    let (on, is_static, method, list) = match n {
        Node::StaticMethodCall(s) => (s.class, true, &s.method, &s.argument_list),
        _ => {
            let Node::MethodCall(m) = n else { return None };
            (m.object, false, &m.method, &m.argument_list)
        }
    };
    Some(CallParts {
        on,
        is_static,
        name: member_name(method)?,
        arguments: list
            .arguments
            .iter()
            .map(mago_syntax::cst::Argument::value)
            .collect(),
    })
}

pub(crate) fn is_matching_call(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    call: &CallParts<'_>,
    method: &str,
) -> bool {
    if !is_name(call.name, method) {
        return false;
    }
    if call.is_static {
        return is_proxy_call(ctx, ancestors, call);
    }
    is_object_type(
        ctx,
        ancestors,
        call.on,
        "Illuminate\\Contracts\\Database\\Query\\Builder",
    )
}

fn is_proxy_call(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], call: &CallParts<'_>) -> bool {
    let facts = facts(ctx);
    let class = match unparenthesize(call.on) {
        Expression::Identifier(id) => name_class(ancestors, id),
        e @ (Expression::Self_(_) | Expression::Static(_) | Expression::Parent(_)) => {
            special_class(ancestors, e)
        }
        e => facts
            .full_type(node_span(e))
            .filter(|t| t.types.len() == 1)
            .and_then(|t| object_class(&t.types[0])),
    };
    class.is_some_and(|class| {
        is_a(ctx, &class, &[MODEL])
            && (facts.codebase)
                .get_declaring_method(class.as_bytes(), call.name)
                .is_none()
    })
}

pub(crate) fn closure_first_param<'a>(
    expr: &'a Expression<'a>,
) -> Option<Option<&'a FunctionLikeParameter<'a>>> {
    match unparenthesize(expr) {
        Expression::Closure(c) => Some(c.parameter_list.parameters.first()),
        Expression::ArrowFunction(a) => Some(a.parameter_list.parameters.first()),
        _ => None,
    }
}

fn is_name_type(param: &FunctionLikeParameter<'_>) -> bool {
    matches!(
        param.hint,
        Some(Hint::Identifier(_) | Hint::Self_(_) | Hint::Parent(_))
    )
}

pub(crate) fn type_param(
    node: &Node<'_, '_>,
    param: &FunctionLikeParameter<'_>,
    class: &str,
) -> Option<Report> {
    if is_name_type(param) {
        return None;
    }
    Some(Report::fixed(
        node_span(node),
        format!("Type the query closure's parameter as `{class}`."),
        Safety::Unsafe,
        vec![set_param_type(param, &format!("\\{class}"))],
    ))
}
