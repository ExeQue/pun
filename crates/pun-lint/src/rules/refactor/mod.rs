mod add_override_attribute;
pub(super) mod no_negated_condition;
mod prefer_attribute_over_annotation;
mod remove_function_argument;
mod remove_function_call;
mod rename_cast;
mod rename_class;
mod rename_class_constant;
mod rename_constant;
mod rename_function;
mod rename_method;
mod replace_function;
mod replace_static_with_method;

pub use rename_method::MethodRename;
pub use replace_static_with_method::StaticToMethod;

use std::sync::LazyLock;

use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::reference::TReference;
use mago_syntax::cst::{Expression, Identifier, Node, PartialArgument};
use regex::bytes::Regex;
use serde_json::{Map, Value};

use super::types::class_scope;
use super::types::facts::facts;
use crate::rules::support::names::{
    imported, imported_member, resolve_class_name, scope_statements,
};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError};

rules! {
    pub(super) fn all {
        add_override_attribute => AddOverrideAttribute,
        prefer_attribute_over_annotation => AnnotationToAttribute::default(),
        remove_function_argument => RemoveFunctionArgument::default(),
        remove_function_call => RemoveFunctionCall::default(),
        rename_cast => RenameCast::default(),
        rename_class => RenameClass::default(),
        rename_class_constant => RenameClassConstant::default(),
        rename_constant => RenameConstant::default(),
        rename_function => RenameFunction::default(),
        rename_method => RenameMethod::default(),
        replace_function => ReplaceFunctionWithConstant::default(),
        replace_function => ReplaceFunctionWithNew::default(),
        replace_function => ReplaceFunctionWithStatic::default(),
        replace_static_with_method => ReplaceStaticWithMethod::default(),
    }
}

fn table(options: &Value) -> Result<Option<&Map<String, Value>>, OptionsError> {
    match options {
        Value::Null => Ok(None),
        Value::Object(map) => Ok(Some(map)),
        _ => Err("options must be a table".into()),
    }
}

pub(crate) fn string_map(options: &Value) -> Result<Vec<(String, String)>, OptionsError> {
    let Some(map) = table(options)? else {
        return Ok(Vec::new());
    };
    map.iter()
        .map(|(k, v)| match v.as_str() {
            Some(s) => Ok((k.clone(), s.to_string())),
            None => Err(format!("`{k}` must be a string").into()),
        })
        .collect()
}

pub(crate) fn items(options: &Value) -> Result<Vec<&Value>, OptionsError> {
    let Some(map) = table(options)? else {
        return Ok(Vec::new());
    };
    if let Some(key) = map.keys().find(|k| *k != "items") {
        return Err(format!("unknown option `{key}`; the list goes under `items`").into());
    }
    match map.get("items") {
        Some(Value::Array(items)) => Ok(items.iter().collect()),
        _ => Err("`items` must be an array".into()),
    }
}

pub(crate) fn object<'a>(
    item: &'a Value,
    allowed: &[&str],
    required: &[&str],
) -> Result<&'a Map<String, Value>, OptionsError> {
    let Value::Object(map) = item else {
        return Err("each of `items` must be a table".into());
    };
    if let Some(key) = map.keys().find(|k| !allowed.contains(&k.as_str())) {
        return Err(format!(
            "unknown key `{key}` in `items`; the keys are {}",
            allowed.join(", ")
        )
        .into());
    }
    if let Some(key) = required.iter().find(|k| !map.contains_key(**k)) {
        return Err(format!("each of `items` needs `{key}`").into());
    }
    Ok(map)
}

pub(crate) fn string(map: &Map<String, Value>, key: &str) -> Result<String, OptionsError> {
    map[key]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("`{key}` must be a string").into())
}

pub(crate) fn int(map: &Map<String, Value>, key: &str) -> Result<i64, OptionsError> {
    map[key]
        .as_i64()
        .ok_or_else(|| format!("`{key}` must be an integer").into())
}

static CLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?-u)^[a-zA-Z_\x7f-\xff][a-zA-Z0-9_\x7f-\xff]*(\\[a-zA-Z_\x7f-\xff][a-zA-Z0-9_\x7f-\xff]*)*$")
        .unwrap()
});
static METHOD_OR_CONSTANT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?-u)^[a-zA-Z_\x80-\xff][a-zA-Z0-9_\x80-\xff]*$").unwrap());
static FUNCTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?-u)^([a-zA-Z_\x7f-\xff][a-zA-Z0-9_\x7f-\xff\\]*[a-zA-Z0-9_\x7f-\xff]\\)?([a-zA-Z_\x80-\xff][a-zA-Z0-9_\x80-\xff]*)$",
    )
    .unwrap()
});

fn assert_name(name: &str, regex: &Regex, kind: &str) -> Result<(), OptionsError> {
    match regex.is_match(name.as_bytes()) {
        true => Ok(()),
        false => Err(format!("\"{name}\" is not a valid {kind} name").into()),
    }
}

pub(crate) fn assert_class(name: &str) -> Result<(), OptionsError> {
    assert_name(name, &CLASS, "class")
}

pub(crate) fn assert_constant(name: &str) -> Result<(), OptionsError> {
    assert_name(name, &METHOD_OR_CONSTANT, "constant")
}

pub(crate) fn assert_method(name: &str) -> Result<(), OptionsError> {
    assert_name(name, &METHOD_OR_CONSTANT, "method")
}

pub(crate) fn assert_function(name: &str) -> Result<(), OptionsError> {
    assert_name(name, &FUNCTION, "function")
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    Function,
    Constant,
}

pub(crate) fn name_of(ancestors: &[Node<'_, '_>], name: &Identifier<'_>, kind: Kind) -> String {
    let written = String::from_utf8_lossy(name.value()).into_owned();
    match name {
        Identifier::FullyQualified(_) => written[1..].to_string(),
        Identifier::Local(local) => {
            imported_member(ancestors, local.value, kind == Kind::Function).unwrap_or(written)
        }
        Identifier::Qualified(_) => {
            imported(ancestors, &written).unwrap_or_else(|| match scope_statements(ancestors).0 {
                Some(ns) => format!("{ns}\\{written}"),
                None => written,
            })
        }
    }
}

pub(crate) fn function_name(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    name: &Identifier<'_>,
) -> String {
    let resolved = name_of(ancestors, name, Kind::Function);
    if let Identifier::Local(local) = name
        && resolved.as_bytes() == local.value
        && let Some(ns) = scope_statements(ancestors).0
    {
        let namespaced = format!("{ns}\\{resolved}");
        if facts(ctx).codebase.function_exists(namespaced.as_bytes()) {
            return namespaced;
        }
    }
    resolved
}

pub(crate) fn called<'ast, 'arena>(node: Node<'ast, 'arena>) -> Option<&'ast Expression<'arena>> {
    match node {
        Node::FunctionCall(call) => Some(call.function),
        Node::FunctionPartialApplication(p) => {
            let args = &p.argument_list.arguments;
            (args.len() == 1
                && matches!(args.get(0), Some(PartialArgument::VariadicPlaceholder(_))))
            .then_some(p.function)
        }
        _ => None,
    }
}

pub(crate) fn is_name(resolved: &str, desired: &str) -> bool {
    !desired.is_empty() && resolved.eq_ignore_ascii_case(desired)
}

pub(crate) fn fully_qualified(name: &str) -> String {
    format!("\\{name}")
}

pub(crate) fn is_class(ctx: &Context<'_>, resolved: &str, required: &str) -> bool {
    let codebase = &facts(ctx).codebase;
    resolved == required
        || (codebase.get_class_like(required.as_bytes()).is_some()
            && codebase.is_instance_of(resolved.as_bytes(), required.as_bytes()))
}

pub(crate) fn is_object_type(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    class: &Expression<'_>,
    required: &str,
) -> bool {
    match unparenthesize(class) {
        Expression::Access(mago_syntax::cst::Access::ClassConstant(_)) => false,
        Expression::Identifier(id) => is_class(ctx, &resolve_class_name(ancestors, id), required),
        Expression::Static(_) | Expression::Self_(_) => class_scope(ancestors)
            .and_then(|c| c.name)
            .is_some_and(|name| is_class(ctx, &name, required)),
        Expression::Parent(_) => parents(ctx, ancestors)
            .is_some_and(|chain| chain.iter().all(|p| is_class(ctx, p, required))),
        other => {
            let facts = facts(ctx);
            let Some(t) = facts.full_type(node_span(other)) else {
                return false;
            };
            let kept: Vec<&TAtomic> = (t.types.iter())
                .filter(|a| !a.is_null() && !matches!(a, TAtomic::Scalar(s) if s.is_false()))
                .collect();
            !kept.is_empty()
                && kept.iter().all(|a| match a {
                    TAtomic::Object(TObject::Named(n)) => {
                        is_class(ctx, &n.name.to_string(), required)
                    }
                    TAtomic::Object(TObject::Enum(e)) => {
                        is_class(ctx, &e.name.to_string(), required)
                    }
                    TAtomic::Reference(TReference::Symbol { name, .. }) => {
                        name.to_string() == required
                    }
                    _ => false,
                })
        }
    }
}

fn parents(ctx: &Context<'_>, ancestors: &[Node<'_, '_>]) -> Option<Vec<String>> {
    let scope = class_scope(ancestors)?;
    scope.name.as_ref()?;
    let first = scope.parent_class?;
    let mut chain = vec![first.clone()];
    if let Some(class) = facts(ctx).codebase.get_class_like(first.as_bytes()) {
        chain.extend(
            class
                .all_parent_classes
                .iter()
                .map(std::string::ToString::to_string),
        );
    }
    Some(chain)
}

pub(crate) fn object_class_name(ctx: &Context<'_>, expr: &Expression<'_>) -> Option<String> {
    let facts = facts(ctx);
    let t = facts.full_type(node_span(unparenthesize(expr)))?;
    let mut names: Vec<String> = Vec::new();
    for atomic in t.types.iter() {
        let name = match atomic {
            TAtomic::Object(TObject::Named(n)) => n.name.to_string(),
            TAtomic::Object(TObject::Enum(e)) => e.name.to_string(),
            _ => return None,
        };
        if !names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            names.push(name);
        }
    }
    let codebase = &facts.codebase;
    let kept: Vec<&String> = (names.iter())
        .filter(|n| {
            !names.iter().any(|other| {
                !other.eq_ignore_ascii_case(n)
                    && codebase.is_instance_of(n.as_bytes(), other.as_bytes())
            })
        })
        .collect();
    match kept[..] {
        [one] => Some(one.clone()),
        _ => None,
    }
}
