use std::collections::HashSet;

use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_codex::metadata::function_like::FunctionLikeMetadata;
use mago_codex::visibility::Visibility;
use mago_word::word;
use pun_phpstan::symbols;

use super::Checker;
use super::params::Variants;

pub(super) enum Method<'a> {
    User {
        declaring: &'a ClassLikeMetadata,
        f: &'a FunctionLikeMetadata,
        name: mago_word::Word,
        original_name: mago_word::Word,
        visibility: Visibility,
        storage: Option<mago_word::Word>,
    },
    Builtin {
        declaring: &'static symbols::Class,
        f: &'static symbols::Function,
    },
}

impl Method<'_> {
    pub(super) fn declaring_display(&self, c: &Checker) -> String {
        match self {
            Method::User { declaring, .. } => c.display(declaring),
            Method::Builtin { declaring, .. } => declaring.name.clone(),
        }
    }

    pub(super) fn declaring_name(&self) -> String {
        match self {
            Method::User { declaring, .. } => {
                String::from_utf8_lossy(declaring.original_name.as_bytes()).into_owned()
            }
            Method::Builtin { declaring, .. } => declaring.name.clone(),
        }
    }

    pub(super) fn name(&self) -> String {
        match self {
            Method::User { original_name, .. } => {
                String::from_utf8_lossy(original_name.as_bytes()).into_owned()
            }
            Method::Builtin { f, .. } => f.name.clone(),
        }
    }

    pub(super) fn is_static(&self) -> bool {
        match self {
            Method::User { f, .. } => f.method_metadata.as_ref().is_some_and(|m| m.is_static),
            Method::Builtin { f, .. } => f.is_static,
        }
    }

    pub(super) fn is_abstract(&self) -> bool {
        match self {
            Method::User { declaring, f, .. } => {
                f.method_metadata.as_ref().is_some_and(|m| m.is_abstract)
                    || declaring.kind.is_interface()
            }
            Method::Builtin { .. } => false,
        }
    }

    pub(super) fn is_final(&self) -> bool {
        match self {
            Method::User { f, .. } => f.method_metadata.as_ref().is_some_and(|m| m.is_final),
            Method::Builtin { .. } => false,
        }
    }

    pub(super) fn visibility(&self) -> Visibility {
        match self {
            Method::User { visibility, .. } => *visibility,
            Method::Builtin { .. } => Visibility::Public,
        }
    }

    pub(super) fn declared_in_interface(&self) -> bool {
        match self {
            Method::User { declaring, .. } => declaring.kind.is_interface(),
            Method::Builtin { declaring, .. } => declaring.is_interface,
        }
    }

    pub(super) fn variants(&self) -> Variants {
        match self {
            Method::User {
                f,
                declaring,
                name,
                storage: holder,
                ..
            } => {
                let mut v = Variants::of(f);
                for a in &mut v.all {
                    let storage = holder
                        .map(|holder| {
                            mago_codex::identifier::method::MethodIdentifier::new(holder, *name)
                        })
                        .or_else(|| declaring.declaring_method_ids.get(name).copied());
                    a.class = Some(declaring.name.to_string());
                    a.callee = Some(super::params::UserCallee {
                        class: Some(
                            storage
                                .map_or(declaring.name, |id| id.get_class_name())
                                .to_string(),
                        ),
                        name: storage
                            .map_or(f.name, |id| id.get_method_name())
                            .to_string(),
                        reflected_name: name.to_string(),
                    });
                }
                v
            }
            Method::Builtin { f, declaring } => {
                let mut v = Variants::of_builtin(f);
                for a in v.all.iter_mut().chain(v.named.iter_mut().flatten()) {
                    a.class = Some(declaring.name.clone());
                }
                v
            }
        }
    }
}

pub(super) fn method<'a>(
    c: &Checker<'a, '_>,
    class: &'a ClassLikeMetadata,
    name: &[u8],
) -> Option<Method<'a>> {
    let codebase = c.codebase;
    let lower = word(name.to_ascii_lowercase());
    let mut current = Some(class);
    let mut seen = HashSet::new();
    while let Some(k) = current
        && seen.insert(k.name)
    {
        if let Some(m) = declared(c, k, &lower) {
            return Some(m);
        }
        current = k
            .direct_parent_class
            .and_then(|p| codebase.get_class_like(p.as_bytes()));
    }
    std::iter::once(&class.name)
        .chain(class.all_parent_interfaces.iter())
        .filter_map(|i| codebase.get_class_like(i.as_bytes()))
        .filter(|i| i.kind.is_interface())
        .find_map(|i| declared(c, i, &lower))
}

pub(super) fn reflected_method<'a>(
    c: &Checker<'a, '_>,
    class: &'a ClassLikeMetadata,
    name: &[u8],
) -> Option<Method<'a>> {
    let native = method(c, class, name);
    let annotation = annotation_method(c, class, name);
    if let (
        Some(Method::User {
            declaring: native_class,
            name: native_name,
            ..
        }),
        Some(Method::User {
            declaring: tag_class,
            ..
        }),
    ) = (&native, &annotation)
    {
        let codebase = c.codebase;
        let native_at = native_class
            .declaring_method_ids
            .get(native_name)
            .map(mago_codex::identifier::method::MethodIdentifier::get_class_name)
            .filter(|t| {
                codebase
                    .get_class_like(t.as_bytes())
                    .is_some_and(|t| t.kind.is_trait())
            })
            .unwrap_or(native_class.name);
        let distances = hierarchy_distances(c, class);
        if let (Some(tag), Some(native)) =
            (distances.get(&tag_class.name), distances.get(&native_at))
            && tag <= native
        {
            return annotation;
        }
    }
    native.or(annotation)
}

fn hierarchy_distances(
    c: &Checker,
    class: &ClassLikeMetadata,
) -> std::collections::HashMap<mago_word::Word, usize> {
    let codebase = c.codebase;
    let get = |n: mago_word::Word| codebase.get_class_like(n.as_bytes());
    let traits = |k: &ClassLikeMetadata| {
        let mut out = Vec::new();
        let mut queue: std::collections::VecDeque<_> = k.direct_trait_names().collect();
        let mut seen = HashSet::new();
        while let Some(t) = queue.pop_front() {
            if !seen.insert(t) {
                continue;
            }
            out.push(t);
            if let Some(t) = get(t) {
                queue.extend(t.direct_trait_names());
            }
        }
        out
    };
    let mut distance = 0;
    let mut distances = std::collections::HashMap::from([(class.name, distance)]);
    for t in traits(class) {
        distance += 1;
        distances.entry(t).or_insert(distance);
    }
    let mut current = class;
    let mut seen = HashSet::from([class.name]);
    while let Some(parent) = current.direct_parent_class.and_then(get)
        && seen.insert(parent.name)
    {
        distance += 1;
        distances.entry(parent.name).or_insert(distance);
        for t in traits(parent) {
            distance += 1;
            distances.entry(t).or_insert(distance);
        }
        current = parent;
    }
    for interface in &class.all_parent_interfaces {
        distance += 1;
        distances.entry(*interface).or_insert(distance);
    }
    distances
}

pub(super) fn annotation_method<'a>(
    c: &Checker<'a, '_>,
    class: &'a ClassLikeMetadata,
    name: &[u8],
) -> Option<Method<'a>> {
    let lower = word(name.to_ascii_lowercase());
    let codebase = c.codebase;
    let mut pending = vec![(class, class)];
    let mut seen = HashSet::new();
    while let Some((k, declaring)) = pending.pop() {
        if !seen.insert(k.name) {
            continue;
        }
        if (k.pseudo_methods.contains(&lower) || k.static_pseudo_methods.contains(&lower))
            && let Some(f) = codebase
                .function_likes
                .get(&(k.name, lower))
                .map(Box::as_ref)
        {
            let mut found = user(c, declaring, f, lower);
            if let Method::User { storage, .. } = &mut found {
                *storage = Some(k.name);
            }
            return Some(found);
        }
        let start = pending.len();
        let get = |n: &mago_word::Word| codebase.get_class_like(n.as_bytes());
        pending.extend(
            k.direct_trait_names()
                .filter_map(|t| get(&t))
                .map(|t| (t, k)),
        );
        if let Some(p) = k.direct_parent_class.as_ref().and_then(get) {
            pending.push((p, p));
        }
        pending.extend(
            k.all_parent_interfaces
                .iter()
                .filter_map(get)
                .map(|i| (i, i)),
        );
        pending[start..].reverse();
    }
    None
}

fn declared<'a>(
    c: &Checker<'a, '_>,
    k: &'a ClassLikeMetadata,
    lower: &mago_word::Word,
) -> Option<Method<'a>> {
    let codebase = c.codebase;
    let own = |k: &'a ClassLikeMetadata| -> Option<Method<'a>> {
        if k.flags.is_built_in() {
            let class = c
                .builtins
                .classes
                .get(&String::from_utf8_lossy(k.name.as_bytes()).to_ascii_lowercase())?;
            let f = class
                .methods
                .get(String::from_utf8_lossy(lower.as_bytes()).as_ref())?;
            return Some(Method::Builtin {
                declaring: class,
                f,
            });
        }
        if k.methods.contains(lower)
            && let Some(f) = codebase.get_method(k.name.as_bytes(), lower.as_bytes())
            && f.span.file_id == k.span.file_id
            && k.span.start.offset <= f.span.start.offset
            && f.span.end.offset <= k.span.end.offset
            && !(f.span == k.span && lower.as_bytes() == b"__construct")
        {
            return Some(user(c, k, f, *lower));
        }
        if k.methods.contains(lower)
            && let Some(f) = codebase.get_method(k.name.as_bytes(), lower.as_bytes())
            && f.flags
                .contains(mago_codex::metadata::flags::MetadataFlags::PATCH)
            && codebase
                .get_declaring_method_class(k.name.as_bytes(), lower.as_bytes())
                .is_some_and(|d| d.as_bytes().eq_ignore_ascii_case(k.name.as_bytes()))
            && k.direct_trait_names().any(|t| {
                codebase
                    .get_class_like(t.as_bytes())
                    .is_some_and(|t| t.methods.contains(lower))
            })
        {
            return Some(user(c, k, f, *lower));
        }
        if k.trait_alias_map.contains_key(lower)
            && let Some(f) = codebase.get_declaring_method(k.name.as_bytes(), lower.as_bytes())
        {
            return Some(user(c, k, f, *lower));
        }
        None
    };
    if let Some(method) = own(k) {
        return Some(method);
    }
    let mut pending = vec![(
        k,
        k.direct_trait_names(),
        codebase.get_declaring_method(k.name.as_bytes(), lower.as_bytes()),
    )];
    let mut active = HashSet::from([k.name]);
    let mut found = None;
    while let Some((parent, traits, declaring)) = pending.last_mut() {
        if let Some(Method::User { f, .. }) = found.take()
            && declaring.is_none_or(|d| d.span == f.span)
        {
            found = Some(user(c, parent, f, *lower));
            active.remove(&parent.name);
            pending.pop();
            continue;
        }
        let Some(k) = traits.find_map(|t| codebase.get_class_like(t.as_bytes())) else {
            active.remove(&parent.name);
            pending.pop();
            continue;
        };
        if !active.insert(k.name) {
            continue;
        }
        found = own(k);
        if found.is_some() {
            active.remove(&k.name);
        } else {
            pending.push((
                k,
                k.direct_trait_names(),
                codebase.get_declaring_method(k.name.as_bytes(), lower.as_bytes()),
            ));
        }
    }
    found
}

fn user<'a>(
    c: &Checker<'a, '_>,
    declaring: &'a ClassLikeMetadata,
    f: &'a FunctionLikeMetadata,
    name: mago_word::Word,
) -> Method<'a> {
    let visibility = c
        .codebase
        .get_method_visibility(declaring.name.as_bytes(), name.as_bytes())
        .unwrap_or_else(|| {
            f.method_metadata
                .as_ref()
                .map_or(Visibility::Public, |m| m.visibility)
        });
    let original_name = c
        .codebase
        .get_method_original_name(declaring.name.as_bytes(), name.as_bytes())
        .unwrap_or(f.original_name);
    Method::User {
        declaring,
        f,
        name,
        original_name,
        visibility,
        storage: None,
    }
}
