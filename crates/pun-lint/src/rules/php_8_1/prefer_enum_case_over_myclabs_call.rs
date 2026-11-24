//! Rector 2.8.4's `MyCLabsMethodCallToEnumConstRector`.

use mago_syntax::cst::{
    Access, Argument, Call, ClassLikeMemberSelector, Expression, MethodCall, Node, NodeKind,
    StaticMethodCall,
};

use super::prefer_enum_case_over_spatie_call::class_name;
use crate::problem::{Edit, Safety};
use crate::rules::refactor::{is_class, is_object_type};
use crate::rules::support::expressions::{Prec, dereferenceable, in_parent, text};
use crate::rules::support::values::{Known, known};
use crate::rules::types::facts::facts;
use crate::rules::types::names::resolve_class_name;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const MYCLABS_FQN: &str = "MyCLabs\\Enum\\Enum";

const ENUM_METHODS: [&[u8]; 7] = [
    b"from",
    b"values",
    b"keys",
    b"isValid",
    b"search",
    b"toArray",
    b"assertValidValue",
];

pub struct PreferEnumCaseOverMyCLabsCall;

impl Rule for PreferEnumCaseOverMyCLabsCall {
    fn id(&self) -> &'static str {
        "lint/php-8.1/prefer-enum-case-over-myclabs-call"
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
                if is_replaced_by_outer_call(ctx, ancestors, call) {
                    return;
                }
                let Some(case) = case_name(&call.method) else {
                    return;
                };
                if !is_object_type(ctx, ancestors, call.class, MYCLABS_FQN) {
                    return;
                }
                // `isEnumConstant()`: `self`, `static` and `parent` name no class Rector can load
                let Expression::Identifier(id) = unparenthesize(call.class) else {
                    return;
                };
                let class = resolve_class_name(ancestors, id);
                if !facts(ctx)
                    .codebase
                    .class_constant_exists(class.as_bytes(), case.as_bytes())
                {
                    return;
                }
                (node_span(call), format!("\\{class}::{case}"))
            }
            _ => return,
        };
        out.push(Report::fixed(
            span,
            "Use the native enum case instead of the MyCLabs enum call.",
            Safety::Unsafe,
            vec![Edit::new(span, fixed)],
        ));
    }
}

/// Rector visits the method call around a static call first and replaces both: the static call is the object of an
/// `->getKey()`, `->getValue()` or `->equals()` that is refactored, or the argument of that `->equals()`.
fn is_replaced_by_outer_call(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    call: &StaticMethodCall<'_>,
) -> bool {
    let span = node_span(call);
    let Some((at, outer)) = ancestors
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, a)| match a {
            Node::MethodCall(m) => Some((i, *m)),
            _ => None,
        })
    else {
        return false;
    };
    let is_part = node_span(unparenthesize(outer.object)) == span
        || first_arg(outer).is_some_and(|a| node_span(unparenthesize(a)) == span);
    is_part && refactor_method_call(ctx, &ancestors[..at], outer).is_some()
}

fn identifier<'a>(selector: &ClassLikeMemberSelector<'a>) -> Option<&'a [u8]> {
    match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    }
}

/// The method name, unless it is a dynamic name or one of MyCLabs' own static methods (`shouldOmitEnumCase()`).
fn case_name(method: &ClassLikeMemberSelector<'_>) -> Option<String> {
    let name = identifier(method)?;
    (!ENUM_METHODS.contains(&name)).then(|| String::from_utf8_lossy(name).into_owned())
}

fn first_arg<'a>(call: &'a MethodCall<'a>) -> Option<&'a Expression<'a>> {
    Some(match call.argument_list.arguments.first()? {
        Argument::Positional(a) => a.value,
        Argument::Named(a) => a.value,
    })
}

/// `refactorMethodCall()`: `->getKey()` to `->name`, `->getValue()` to `->value` and `->equals($b)` to `=== $b`.
fn refactor_method_call(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    call: &MethodCall<'_>,
) -> Option<String> {
    let method = identifier(&call.method)?;
    let property = match method {
        b"getKey" => "name",
        b"getValue" => "value",
        b"equals" => "",
        _ => return None,
    };
    if !is_enum_value(ctx, ancestors, call.object) {
        return None;
    }
    if property.is_empty() {
        let left = enum_side(ctx, ancestors, call.object)?;
        let right = enum_side(ctx, ancestors, first_arg(call)?)?;
        return Some(in_parent(
            node_span(call),
            ancestors,
            format!("{left} === {right}"),
            Prec::Infix(80, 80, 80),
        ));
    }
    let fetch = match unparenthesize(call.object) {
        Expression::Call(Call::StaticMethod(s)) => format!(
            "{}::{}->{property}",
            class_name(ancestors, s.class)?,
            case_name(&s.method)?
        ),
        object => format!("{}->{property}", dereferenceable(ctx.source, object)),
    };
    Some(fetch)
}

/// `getNonEnumReturnTypeExpr()`, else `getValidEnumExpr()`: a call kept as written, or the enum value.
fn enum_side(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
) -> Option<String> {
    let expr = unparenthesize(expr);
    match expr {
        Expression::Call(Call::StaticMethod(s)) => {
            if !is_object_type(ctx, ancestors, s.class, MYCLABS_FQN) {
                return Some(text(ctx, expr).to_string());
            }
            let method = identifier(&s.method)?;
            // Rector fails on a `self`, `static` or `parent` caller it can't load, and a dynamic class has no name
            let Expression::Identifier(id) = unparenthesize(s.class) else {
                return None;
            };
            let class = resolve_class_name(ancestors, id);
            if !facts(ctx)
                .codebase
                .class_constant_exists(class.as_bytes(), method)
            {
                return Some(text(ctx, expr).to_string());
            }
            // `getEnumConstFetch()`
            Some(format!(
                "{}::{}",
                class_name(ancestors, s.class)?,
                case_name(&s.method)?
            ))
        }
        Expression::Call(Call::Method(m)) => {
            // Rector loads the class named by the caller variable, and fails on it
            if is_enum_value(ctx, ancestors, m.object) {
                return None;
            }
            Some(text(ctx, expr).to_string())
        }
        Expression::Variable(_) | Expression::Access(Access::Property(_)) => {
            is_enum_value(ctx, ancestors, expr).then(|| text(ctx, expr).to_string())
        }
        _ => None,
    }
}

/// `isObjectType($expr, MyCLabs\Enum\Enum)`. PHPStan types `X::CASE()` by the enum's `@method` tags, and a variable
/// by the value assigned to it, where Pun's analyser leaves them untyped: so a call of a constant's name on a MyCLabs
/// enum class, or a variable last assigned one or typed by its parameter, counts as an instance of that enum.
fn is_enum_value(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    if is_object_type(ctx, ancestors, expr, MYCLABS_FQN) {
        return true;
    }
    match known(ancestors, expr, node_span(expr).start) {
        Some(Known::Value(Expression::Call(Call::StaticMethod(s)))) => {
            let (Expression::Identifier(id), Some(method)) =
                (unparenthesize(s.class), identifier(&s.method))
            else {
                return false;
            };
            let class = resolve_class_name(ancestors, id);
            is_class(ctx, &class, MYCLABS_FQN)
                && facts(ctx)
                    .codebase
                    .class_constant_exists(class.as_bytes(), method)
        }
        Some(Known::Hint(mago_syntax::cst::Hint::Identifier(id))) => {
            is_class(ctx, &resolve_class_name(ancestors, id), MYCLABS_FQN)
        }
        _ => false,
    }
}
