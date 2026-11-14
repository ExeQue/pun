use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::reference::TReference;
use mago_syntax::cst::{Argument, Modifier, Node, NodeKind};

use super::{argument_texts, call, class_name, is_a, is_name, member_name, replace};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const TEST_CASE: &str = "Illuminate\\Foundation\\Testing\\TestCase";

pub struct PreferTravelTo;

impl Rule for PreferTravelTo {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-travel-to"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::StaticMethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::StaticMethodCall(s) = node else {
            return;
        };
        if !member_name(&s.method).is_some_and(|n| is_name(n, "setTestNow")) {
            return;
        }
        if !in_test_case_scope(ctx, ancestors) {
            return;
        }
        let Some(class) = class_name(s.class, ancestors) else {
            return;
        };
        let carbon = [
            "Carbon\\Carbon",
            "Carbon\\CarbonImmutable",
            "Illuminate\\Support\\Carbon",
        ];
        if !is_a(ctx, &class, &carbon) {
            return;
        }
        let args = argument_texts(ctx, &s.argument_list);
        let args = if args.is_empty() { vec!["null"] } else { args };
        out.push(replace(
            node_span(s),
            "Use `$this->travelTo()` instead of `setTestNow()` in a test.".to_string(),
            call("$this->travelTo", &args),
        ));
    }
}

fn in_test_case_scope(ctx: &Context<'_>, ancestors: &[Node<'_, '_>]) -> bool {
    let mut closure = None;
    for (index, ancestor) in ancestors.iter().enumerate().rev() {
        match ancestor {
            Node::Closure(c) if c.r#static.is_none() => closure = Some(index),
            Node::ArrowFunction(f) if f.r#static.is_none() => closure = Some(index),
            Node::Closure(_) | Node::ArrowFunction(_) | Node::Function(_) => return false,
            Node::Method(m) if m.modifiers.iter().any(|m| matches!(m, Modifier::Static(_))) => {
                return false;
            }
            Node::Method(_) => return class_is_test_case(ctx, &ancestors[..index]),
            _ => {}
        }
    }
    closure.is_some_and(|index| closure_this_is_test_case(ctx, ancestors, index))
}

fn class_is_test_case(ctx: &Context<'_>, ancestors: &[Node<'_, '_>]) -> bool {
    let Some(class) = ancestors.iter().rev().find_map(|a| match a {
        Node::Class(c) => Some(c),
        _ => None,
    }) else {
        return false;
    };
    let own = String::from_utf8_lossy(class.name.value);
    let own = match crate::rules::support::names::scope_statements(ancestors).0 {
        Some(ns) => format!("{ns}\\{own}"),
        None => own.into_owned(),
    };
    own.eq_ignore_ascii_case(TEST_CASE)
        || class
            .extends
            .as_ref()
            .and_then(|e| e.types.first())
            .is_some_and(|parent| {
                is_a(
                    ctx,
                    &super::names::resolve_class_name(ancestors, parent),
                    &[TEST_CASE],
                )
            })
}

fn closure_this_is_test_case(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], index: usize) -> bool {
    let span = node_span(&ancestors[index]);
    let Some(call) = ancestors[..index].iter().rev().find_map(|a| match a {
        Node::FunctionCall(call) => Some(*call),
        _ => None,
    }) else {
        return false;
    };
    let Some(position) = (call.argument_list.arguments.iter()).position(|a| {
        matches!(a, Argument::Positional(_)) && node_span(unparenthesize(a.value())) == span
    }) else {
        return false;
    };
    let Some(name) = crate::rules::support::names::called_function_name(ancestors, call.function)
    else {
        return false;
    };
    let facts = crate::rules::types::facts::facts(ctx);
    let namespaced =
        (crate::rules::types::names::class_imports(ancestors).0).filter(|_| !name.contains('\\'));
    let documented = facts.documented();
    let Some(function) = namespaced
        .and_then(|ns| documented.get_function(format!("{ns}\\{name}").as_bytes()))
        .or_else(|| documented.get_function(name.as_bytes()))
    else {
        return false;
    };
    let Some(this) = (function.parameters.get(position)).and_then(|p| p.closure_this_type.as_ref())
    else {
        return false;
    };
    !this.type_union.types.is_empty()
        && this.type_union.types.iter().all(|t| match t {
            TAtomic::Object(TObject::Named(n)) => is_a(
                ctx,
                &String::from_utf8_lossy(n.name.as_bytes()),
                &[TEST_CASE],
            ),
            TAtomic::Reference(TReference::Symbol { name, .. }) => {
                is_a(ctx, &String::from_utf8_lossy(name.as_bytes()), &[TEST_CASE])
            }
            _ => false,
        })
}
