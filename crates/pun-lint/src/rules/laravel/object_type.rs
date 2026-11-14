use std::collections::HashSet;

use mago_codex::metadata::CodebaseMetadata;
use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::reference::TReference;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{Call, ClassLikeMemberSelector, Expression, Hint, Node, Variable};

use super::is_a;
use super::names::resolve_class_name;
use crate::Context;
use crate::rules::types::class_scope;
use crate::rules::types::compare::mapper::flatten;
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};

pub(crate) fn is_object_type(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
    class: &str,
) -> bool {
    match unparenthesize(expr) {
        Expression::Identifier(id) => {
            name_class(ancestors, id).is_some_and(|name| is_a(ctx, &name, &[class]))
        }
        Expression::Static(_) | Expression::Self_(_) | Expression::Parent(_) => {
            special_class(ancestors, expr).is_some_and(|name| is_a(ctx, &name, &[class]))
        }
        e => match mixin_receiver(&facts(ctx), e) {
            Some(Receiver::Class(name)) => is_a(ctx, &name, &[class]),
            Some(Receiver::Error) => false,
            None => match facts(ctx).full_type(node_span(e)) {
                Some(t) => is_type_of(ctx, t, class),
                None => unanalysed_param_hint(ancestors, e)
                    .is_some_and(|h| is_hint_type(ctx, ancestors, h, class)),
            },
        },
    }
}

enum Receiver {
    Class(String),
    Error,
}

fn mixin_receiver(facts: &Facts, expr: &Expression<'_>) -> Option<Receiver> {
    let Expression::Call(Call::Method(call)) = unparenthesize(expr) else {
        return None;
    };
    let ClassLikeMemberSelector::Identifier(name) = &call.method else {
        return None;
    };
    let inner = mixin_receiver(facts, call.object);
    let class = match &inner {
        Some(Receiver::Error) => return Some(Receiver::Error),
        Some(Receiver::Class(class)) => class.clone(),
        None => match &facts
            .full_type(node_span(unparenthesize(call.object)))?
            .types[..]
        {
            [atomic] => object_class(atomic)?,
            _ => return None,
        },
    };
    let codebase = facts.documented();
    known_lineage(codebase, &class)?;
    if codebase.method_exists(class.as_bytes(), name.value) {
        return match inner {
            Some(Receiver::Class(_)) if returns_static(codebase, &class, name.value) => {
                Some(Receiver::Class(class))
            }
            _ => None,
        };
    }
    match mixin_method(codebase, &class, name.value, &mut HashSet::new())? {
        Some((mixin, true)) => Some(Receiver::Class(mixin)),
        Some((_, false)) => None,
        None => Some(Receiver::Error),
    }
}

fn known_lineage<'c>(
    codebase: &'c CodebaseMetadata,
    class: &str,
) -> Option<Vec<&'c ClassLikeMetadata>> {
    let metadata = codebase.get_class_like(class.as_bytes())?;
    let parents = (metadata.all_parent_classes.iter())
        .map(|p| codebase.get_class_like(p.as_bytes()))
        .collect::<Option<Vec<_>>>()?;
    Some(std::iter::once(metadata).chain(parents).collect())
}

fn mixin_method(
    codebase: &CodebaseMetadata,
    class: &str,
    method: &[u8],
    seen: &mut HashSet<String>,
) -> Option<Option<(String, bool)>> {
    let lineage = known_lineage(codebase, class)?;
    let traits = lineage.iter().flat_map(|c| &c.used_traits);
    let mixins = (lineage.iter().map(|c| &c.mixins)).chain(
        traits
            .filter_map(|t| codebase.get_class_like(t.as_bytes()))
            .map(|t| &t.mixins),
    );
    for mixin in mixins.flatten() {
        let [atomic] = &mixin.type_union.types[..] else {
            return None;
        };
        let mixin = object_class(atomic)?;
        if !seen.insert(mixin.to_ascii_lowercase()) {
            continue;
        }
        known_lineage(codebase, &mixin)?;
        if codebase.method_exists(mixin.as_bytes(), method) {
            let returns = returns_static(codebase, &mixin, method);
            return Some(Some((mixin, returns)));
        }
        if let Some(found) = mixin_method(codebase, &mixin, method, seen)? {
            return Some(Some(found));
        }
    }
    Some(None)
}

fn returns_static(codebase: &CodebaseMetadata, class: &str, method: &[u8]) -> bool {
    let declared = codebase.get_declaring_method(class.as_bytes(), method);
    declared
        .and_then(|m| m.return_type_metadata.as_ref())
        .is_some_and(|t| {
            matches!(&t.type_union.types[..], [TAtomic::Object(TObject::Named(n))] if n.is_this || n.is_static)
        })
}

fn unanalysed_param_hint<'a>(
    ancestors: &[Node<'a, 'a>],
    expr: &Expression<'_>,
) -> Option<&'a Hint<'a>> {
    let Expression::Variable(Variable::Direct(v)) = expr else {
        return None;
    };
    let function = ancestors.iter().rev().find_map(|a| match a {
        Node::Function(f) => Some(Some(*f)),
        Node::Closure(_) | Node::ArrowFunction(_) | Node::Method(_) => Some(None),
        _ => None,
    })??;
    let param = (function.parameter_list.parameters.iter()).find(|p| p.variable.name == v.name)?;
    let mut assigned = false;
    let mut stack = vec![Node::Block(&function.body)];
    while let Some(node) = stack.pop() {
        if let Node::Assignment(a) = node
            && matches!(a.lhs, Expression::Variable(Variable::Direct(l)) if l.name == v.name)
        {
            assigned = true;
        }
        node.visit_children(|c| stack.push(c));
    }
    (!assigned).then_some(param.hint.as_ref()?)
}

pub(crate) fn is_type_of(ctx: &Context<'_>, t: &TUnion, class: &str) -> bool {
    if t.is_mixed() {
        return false;
    }
    if let [atomic] = &t.types[..]
        && let Some(name) = object_class(atomic)
    {
        return is_a(ctx, &name, &[class]);
    }
    let mut kept = t.types.iter().filter(|a| match a {
        TAtomic::Null => false,
        TAtomic::Scalar(TScalar::Bool(b)) => !b.is_false(),
        _ => true,
    });
    let mut any = false;
    kept.all(|a| {
        any = true;
        object_class(a).is_some_and(|name| is_subclass(ctx, &name, class))
    }) && any
}

pub(crate) fn is_hint_type(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    hint: &Hint<'_>,
    class: &str,
) -> bool {
    let named = |h: &Hint<'_>| match h {
        Hint::Identifier(id) => is_a(ctx, &resolve_class_name(ancestors, id), &[class]),
        Hint::Self_(_) => class_scope(ancestors)
            .and_then(|s| s.name)
            .is_some_and(|own| is_a(ctx, &own, &[class])),
        Hint::Parent(_) => {
            let facts = facts(ctx);
            let parents: Vec<String> = class_scope(ancestors)
                .and_then(|s| s.name.zip(s.parent_class))
                .map(|(own, parent)| {
                    let all = facts.codebase.get_class_like(own.as_bytes());
                    all.map_or(vec![parent], |c| {
                        c.all_parent_classes
                            .iter()
                            .map(std::string::ToString::to_string)
                            .collect()
                    })
                })
                .unwrap_or_default();
            !parents.is_empty() && parents.iter().all(|p| is_a(ctx, p, &[class]))
        }
        _ => false,
    };
    let mut parts = Vec::new();
    match hint {
        Hint::Identifier(_) | Hint::Self_(_) | Hint::Parent(_) => return named(hint),
        Hint::Nullable(n) => return named(n.hint) || matches!(n.hint, Hint::False(_)),
        Hint::Union(_) | Hint::False(_) => flatten(hint, true, &mut parts),
        _ => return false,
    }
    let without_null: Vec<&Hint<'_>> = parts
        .into_iter()
        .filter(|h| !matches!(h, Hint::Null(_)))
        .collect();
    !without_null.is_empty()
        && without_null
            .into_iter()
            .filter(|h| !matches!(h, Hint::False(_)))
            .all(named)
}

pub(crate) fn object_class(atomic: &TAtomic) -> Option<String> {
    match atomic {
        TAtomic::Object(TObject::Named(n)) => Some(n.name.to_string()),
        TAtomic::Object(TObject::Enum(e)) => Some(e.name.to_string()),
        TAtomic::Reference(TReference::Symbol { name, .. }) => Some(name.to_string()),
        _ => None,
    }
}

fn is_subclass(ctx: &Context<'_>, name: &str, class: &str) -> bool {
    if name.eq_ignore_ascii_case(class) {
        return true;
    }
    let facts = facts(ctx);
    (facts.codebase.get_class_like(name.as_bytes())).is_some_and(|c| {
        (c.all_parent_classes.iter())
            .chain(c.all_parent_interfaces.iter())
            .any(|p| p.as_bytes().eq_ignore_ascii_case(class.as_bytes()))
    })
}

pub(crate) fn name_class(
    ancestors: &[Node<'_, '_>],
    id: &mago_syntax::cst::Identifier<'_>,
) -> Option<String> {
    Some(resolve_class_name(ancestors, id))
}

pub(crate) fn special_class(ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> Option<String> {
    let scope = class_scope(ancestors)?;
    match expr {
        Expression::Parent(_) => scope.parent_class,
        _ => scope.name,
    }
}

pub(crate) fn class_string_class(ctx: &Context<'_>, t: &TUnion) -> Option<String> {
    let [TAtomic::Scalar(scalar)] = &t.types[..] else {
        return None;
    };
    match scalar {
        TScalar::ClassLikeString(c) => match (c.literal_value(), c.constraint()) {
            (Some(value), _) => Some(value.to_string()),
            (None, Some(TAtomic::Object(TObject::Named(n)))) if n.is_static || n.is_this => {
                let facts = facts(ctx);
                let class = facts.codebase.get_class_like(n.name.as_bytes())?;
                Some(class.original_name.to_string())
            }
            (None, constraint) => object_class(constraint?),
        },
        s => {
            let value = std::str::from_utf8(s.get_known_literal_string_value()?).ok()?;
            let facts = facts(ctx);
            facts
                .codebase
                .get_class_like(value.as_bytes())
                .map(|_| value.to_string())
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Tri {
    No,
    Maybe,
    Yes,
}

pub(crate) fn class_super_of(ctx: &Context<'_>, sup: &str, sub: &str) -> Tri {
    exact_class_super_of(ctx, sup, sub, false)
}

pub(crate) fn exact_class_super_of(ctx: &Context<'_>, sup: &str, sub: &str, exact: bool) -> Tri {
    if sup.eq_ignore_ascii_case(sub) {
        return Tri::Yes;
    }
    let facts = facts(ctx);
    let (Some(a), Some(b)) = (
        facts.codebase.get_class_like(sup.as_bytes()),
        facts.codebase.get_class_like(sub.as_bytes()),
    ) else {
        return Tri::Maybe;
    };
    if a.kind.is_trait() || b.kind.is_trait() {
        return Tri::No;
    }
    let is_final = |c: &mago_codex::metadata::class_like::ClassLikeMetadata| {
        c.flags.is_final() || c.kind.is_enum()
    };
    if !exact && is_subclass(ctx, sub, sup) {
        return Tri::Yes;
    }
    if is_subclass(ctx, sup, sub)
        || (a.kind.is_interface() && !is_final(b))
        || (b.kind.is_interface() && !exact && !is_final(a))
    {
        return Tri::Maybe;
    }
    Tri::No
}

pub(crate) fn object_super_of_union(ctx: &Context<'_>, class: &str, t: &TUnion) -> Tri {
    let mut answers = t.types.iter().map(|a| object_super_of(ctx, class, a));
    let first = answers.next().unwrap_or(Tri::Yes);
    if answers.all(|a| a == first) {
        first
    } else {
        Tri::Maybe
    }
}

pub(crate) fn object_super_of(ctx: &Context<'_>, class: &str, atomic: &TAtomic) -> Tri {
    if let Some(name) = object_class(atomic) {
        return class_super_of(ctx, class, &name);
    }
    match atomic {
        TAtomic::Callable(c) if c.get_signature().is_some_and(|s| s.is_closure) => {
            class_super_of(ctx, class, "Closure")
        }
        TAtomic::Callable(_)
        | TAtomic::Object(_)
        | TAtomic::Mixed(_)
        | TAtomic::GenericParameter(_)
        | TAtomic::Placeholder => Tri::Maybe,
        TAtomic::Never => Tri::Yes,
        _ => Tri::No,
    }
}
