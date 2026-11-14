use super::{Template, Tri, Type, mixed};
use crate::symbols;

pub(super) fn static_object(class: &str) -> Type {
    let Some(c) = symbols::lookup(class) else {
        return Type::Object(class.to_string(), Vec::new(), false);
    };
    let args = c
        .templates
        .iter()
        .map(|t| {
            Type::Template(Box::new(Template {
                name: t.name.clone(),
                scope: format!("class {}", c.name),
                bound: crate::resolve::resolve_str(&t.bound, Some(&c.name)),
                default: t
                    .default
                    .as_ref()
                    .map(|d| crate::resolve::resolve_str(d, Some(&c.name))),
                argument: true,
            }))
        })
        .collect();
    Type::Object(c.name.clone(), args, false)
}

pub(super) fn static_of(class: &str, args: &[Type]) -> Type {
    if args.is_empty() {
        static_object(class)
    } else {
        Type::Object(class.to_string(), args.to_vec(), false)
    }
}

pub(super) fn iterable_arg(class: &str, args: &[Type], key: bool) -> Type {
    let Some(c) = symbols::lookup(class) else {
        return Type::Error;
    };
    c.iterable
        .as_ref()
        .map(|(k, v)| if key { k } else { v })
        .map_or(Type::Error, |s| {
            let t = with_class_args(&crate::resolve::resolve_str(s, Some(&c.name)), &c, args);
            with_ancestor_args(&t, &c, args)
        })
}

fn with_ancestor_args(t: &Type, c: &symbols::Class, args: &[Type]) -> Type {
    if let Type::Template(tpl) = t
        && let Some(ancestor) = tpl.scope.strip_prefix("class ")
        && let Some(given) = ancestor_args(c, ancestor, args)
        && let Some(i) = symbols::lookup(ancestor)
            .and_then(|a| a.templates.iter().position(|x| x.name == tpl.name))
        && let Some(arg) = given.get(i)
    {
        return arg.clone();
    }
    t.traverse(&|x| with_ancestor_args(x, c, args))
}

pub fn ancestor_args(c: &symbols::Class, ancestor: &str, args: &[Type]) -> Option<Vec<Type>> {
    let generic = c.generics.get(ancestor)?;
    Some(
        generic
            .iter()
            .map(|s| with_class_args(&crate::resolve::resolve_str(s, Some(&c.name)), c, args))
            .collect(),
    )
}

pub(super) fn with_class_args(t: &Type, c: &symbols::Class, args: &[Type]) -> Type {
    if let Type::Template(tpl) = t
        && tpl.scope == format!("class {}", c.name)
        && let Some(i) = c.templates.iter().position(|x| x.name == tpl.name)
    {
        return match args.get(i) {
            Some(Type::Wildcard) | None if matches!(tpl.bound, Type::Mixed { .. }) => mixed(),
            Some(Type::Wildcard) | None => tpl.bound.clone(),
            Some(Type::Projection(_, arg)) => (**arg).clone(),
            Some(arg) => arg.clone(),
        };
    }
    t.traverse(&|x| with_class_args(x, c, args))
}

pub(super) fn is_instance_of(class: &str, target: &str) -> Tri {
    match symbols::lookup(class) {
        Some(c) if c.name == target || c.ancestors.iter().any(|a| a == target) => Tri::Yes,
        Some(c) if c.is_interface => Tri::Maybe,
        Some(_) => Tri::No,
        None => Tri::Maybe,
    }
}

pub(super) fn string_is_callable(bytes: &[u8]) -> Tri {
    let s = String::from_utf8_lossy(bytes);
    let s = s.as_ref();
    if s.is_empty() {
        return Tri::No;
    }
    if symbols::has_function(s) {
        return Tri::Yes;
    }
    let Some((class, method)) = s.split_once("::") else {
        return Tri::No;
    };
    let word = |w: &str, backslash: bool| {
        let ok = |ch: char| {
            ch == '_' || !ch.is_ascii() || ch.is_ascii_alphanumeric() || (backslash && ch == '\\')
        };
        w.chars()
            .next()
            .is_some_and(|ch| ok(ch) && !ch.is_ascii_digit())
            && w.chars().all(ok)
    };
    if !word(class, true) || !word(method, false) {
        return Tri::No;
    }
    match symbols::lookup(class) {
        None => Tri::Maybe,
        Some(c) if has_method(&c, &method.to_ascii_lowercase()) => Tri::Yes,
        Some(c) if !c.is_final => Tri::Maybe,
        Some(_) => Tri::No,
    }
}

pub(super) fn has_method(c: &symbols::Class, name: &str) -> bool {
    std::iter::once(c.name.as_str())
        .chain(c.ancestors.iter().map(String::as_str))
        .any(|n| symbols::lookup(n).is_some_and(|a| a.methods.contains_key(name)))
}

pub(super) fn canonical_class(class: &str) -> String {
    symbols::lookup(class).map_or_else(|| class.to_string(), |c| c.name.clone())
}
