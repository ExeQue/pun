//! Rector 2.8.4's `SpatieEnumMethodCallToEnumConstRector`.

use mago_syntax::cst::{
    ClassLikeMemberSelector, Expression, MethodCall, Node, NodeKind, StaticMethodCall,
};

use crate::problem::{Edit, Safety};
use crate::rules::refactor::is_object_type;
use crate::rules::types::names::resolve_class_name;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const SPATIE_FQN: &str = "Spatie\\Enum\\Enum";

const ENUM_METHODS: [&[u8]; 7] = [
    b"from",
    b"values",
    b"keys",
    b"isValid",
    b"search",
    b"toArray",
    b"assertValidValue",
];

pub struct PreferEnumCaseOverSpatieCall;

impl Rule for PreferEnumCaseOverSpatieCall {
    fn id(&self) -> &'static str {
        "lint/php-8.1/prefer-enum-case-over-spatie-call"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80100)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall, NodeKind::StaticMethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (span, fixed) = match node {
            Node::MethodCall(call) => {
                let Some(fixed) = refactor_method_call(ctx, ancestors, call) else {
                    return;
                };
                (node_span(call), fixed)
            }
            Node::StaticMethodCall(call) => {
                // Rector visits the method call around it first and replaces both.
                let outer = ancestors.iter().rev().find_map(|a| match a {
                    Node::MethodCall(m) => Some(*m),
                    _ => None,
                });
                if outer.is_some_and(|m| {
                    node_span(unparenthesize(m.object)) == node_span(call)
                        && refactor_method_call(ctx, ancestors, m).is_some()
                }) {
                    return;
                }
                let Some(name) = case_name(&call.method) else {
                    return;
                };
                if !is_object_type(ctx, ancestors, call.class, SPATIE_FQN) {
                    return;
                }
                let Some(class) = class_name(ancestors, call.class) else {
                    return;
                };
                (
                    node_span(call),
                    format!("{class}::{}", name.to_ascii_uppercase()),
                )
            }
            _ => return,
        };
        out.push(Report::fixed(
            span,
            "Use the native enum case instead of the Spatie enum call.",
            Safety::Unsafe,
            vec![Edit::new(span, fixed)],
        ));
    }
}

/// The method name of the call, unless it is a dynamic name or one of Spatie's own static methods.
fn case_name(method: &ClassLikeMemberSelector<'_>) -> Option<String> {
    let ClassLikeMemberSelector::Identifier(id) = method else {
        return None;
    };
    (!ENUM_METHODS.contains(&id.value)).then(|| String::from_utf8_lossy(id.value).into_owned())
}

/// `createClassConstFetch()`'s class: `self`, `static` and `parent` as written, any other name fully qualified.
fn class_name(ancestors: &[Node<'_, '_>], class: &Expression<'_>) -> Option<String> {
    match unparenthesize(class) {
        Expression::Identifier(id) => Some(format!("\\{}", resolve_class_name(ancestors, id))),
        Expression::Self_(_) => Some("self".to_string()),
        Expression::Static(_) => Some("static".to_string()),
        Expression::Parent(_) => Some("parent".to_string()),
        _ => None,
    }
}

/// `refactorMethodCall()`: `X::case()->getName()` or `->label()` to `X::CASE->name`, `->getValue()` or `->value()` to
/// `X::CASE->value`.
fn refactor_method_call(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    call: &MethodCall<'_>,
) -> Option<String> {
    let ClassLikeMemberSelector::Identifier(method) = &call.method else {
        return None;
    };
    let property = match method.value {
        b"getName" | b"label" => "name",
        b"getValue" | b"value" => "value",
        _ => return None,
    };
    if !is_object_type(ctx, ancestors, call.object, SPATIE_FQN) {
        return None;
    }
    let Expression::Call(mago_syntax::cst::Call::StaticMethod(StaticMethodCall {
        class,
        method,
        ..
    })) = unparenthesize(call.object)
    else {
        return None;
    };
    let class = class_name(ancestors, class)?;
    let name = case_name(method)?;
    Some(format!(
        "{class}::{}->{property}",
        name.to_ascii_uppercase()
    ))
}
