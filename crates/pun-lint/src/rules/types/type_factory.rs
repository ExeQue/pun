use std::sync::Arc;

use mago_codex::metadata::CodebaseMetadata;
use mago_codex::ttype::TType;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::array::TArray;
use mago_codex::ttype::atomic::array::keyed::TKeyedArray;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::object::named::TNamedObject;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{Block, Expression, Return};

use super::add_return_type_from_new::anonymous_parents;
use super::declare::union_order;
use super::native::native_type;
use super::silent_void::{Place, has_silent_void};
use crate::syntax::unparenthesize as unparenthesized;

#[derive(Debug)]
pub(crate) enum Created {
    Mixed,
    Single(TAtomic),
    Union(Vec<TAtomic>),
}

pub(crate) fn create_keep_constant(types: Vec<TAtomic>) -> Created {
    let mut unwrapped = Vec::new();
    for t in types {
        match &t {
            TAtomic::Array(a) if is_constant_array(&t) => {
                unwrapped.extend(unwrap_constant_array(a));
            }
            _ => unwrapped.push(t),
        }
    }
    let mut types = uniquate(unwrapped);
    match types.len() {
        1 => Created::Single(types.remove(0)),
        _ if types.is_empty() || types.iter().any(|t| matches!(t, TAtomic::Mixed(_))) => {
            Created::Mixed
        }
        _ => Created::Union(types),
    }
}

fn uniquate(types: Vec<TAtomic>) -> Vec<TAtomic> {
    let mut unique: Vec<(String, TAtomic)> = Vec::new();
    let mut constants: Vec<(String, String)> = Vec::new();
    let (mut has_false, mut has_true) = (false, false);
    let insert = |list: &mut Vec<(String, TAtomic)>, hash: String, t: TAtomic| match list
        .iter_mut()
        .find(|(h, _)| *h == hash)
    {
        Some(entry) => entry.1 = t,
        None => list.push((hash, t)),
    };
    for t in types {
        let is_true = matches!(&t, TAtomic::Scalar(TScalar::Bool(b)) if b.is_true());
        let is_false = matches!(&t, TAtomic::Scalar(TScalar::Bool(b)) if b.is_false());
        has_true |= is_true;
        has_false |= is_false;
        let t = if has_true && has_false && (is_true || is_false) {
            TAtomic::Scalar(TScalar::bool())
        } else {
            t
        };
        match without_value(&t) {
            Some(general) => {
                let (hash, general_hash) = (type_hash(&t), type_hash(&general));
                match constants.iter_mut().find(|(h, _)| *h == hash) {
                    Some(entry) => entry.1 = general_hash,
                    None => constants.push((hash.clone(), general_hash)),
                }
                insert(&mut unique, hash, t);
            }
            None => insert(&mut unique, type_hash(&t), t),
        }
    }
    for (hash, general) in &constants {
        if unique.iter().any(|(h, _)| h == general) {
            unique.retain(|(h, _)| h != hash);
        }
    }
    unique.into_iter().map(|(_, t)| t).collect()
}

fn without_value(t: &TAtomic) -> Option<TAtomic> {
    Some(TAtomic::Scalar(match t {
        TAtomic::Scalar(TScalar::Float(f)) if f.is_literal() => TScalar::float(),
        TAtomic::Scalar(TScalar::String(s)) if s.is_known_literal() => TScalar::string(),
        TAtomic::Scalar(TScalar::Integer(i)) if i.is_literal() => TScalar::int(),
        TAtomic::Scalar(TScalar::Bool(b)) if b.is_true() || b.is_false() => TScalar::bool(),
        _ => return None,
    }))
}

fn is_constant_value(t: &TAtomic) -> bool {
    match t {
        TAtomic::Null => true,
        TAtomic::Scalar(TScalar::Bool(b)) => !b.is_general(),
        TAtomic::Scalar(TScalar::Integer(i)) => i.is_literal(),
        TAtomic::Scalar(TScalar::Float(f)) => f.is_literal(),
        TAtomic::Scalar(TScalar::String(s)) => s.is_known_literal(),
        _ => is_constant_array(t),
    }
}

fn type_hash(t: &TAtomic) -> String {
    match t {
        TAtomic::Mixed(_) => "mixed".to_string(),
        TAtomic::Array(TArray::Keyed(TKeyedArray {
            parameters: Some((key, item)),
            non_empty: false,
            ..
        })) => format!(
            "{}{}{}[]",
            union_hash(item),
            union_hash(key),
            describe(item)
        ),
        TAtomic::Object(TObject::Named(n)) if n.type_parameters.is_none() => {
            String::from_utf8_lossy(n.name.as_bytes()).to_ascii_lowercase()
        }
        TAtomic::Object(TObject::Enum(e)) => {
            String::from_utf8_lossy(e.get_name().as_bytes()).to_ascii_lowercase()
        }
        TAtomic::Null => "NullType".to_string(),
        TAtomic::Scalar(TScalar::Bool(b)) if !b.is_general() => "ConstantBooleanType".to_string(),
        TAtomic::Scalar(TScalar::Integer(i)) if i.is_literal() => "ConstantIntegerType".to_string(),
        TAtomic::Scalar(TScalar::Float(f)) if f.is_literal() => "ConstantFloatType".to_string(),
        TAtomic::Scalar(TScalar::String(s)) if s.is_known_literal() => {
            "ConstantStringType".to_string()
        }
        _ if is_constant_value(t) => "ConstantArrayType".to_string(),
        _ => describe_atomic(t),
    }
}

fn union_hash(u: &TUnion) -> String {
    match u.types.as_ref() {
        [one] => type_hash(one),
        many if many.iter().all(is_constant_value) => "UnionType".to_string(),
        _ => describe(u),
    }
}

fn describe(u: &TUnion) -> String {
    let mut ids: Vec<String> = u.types.iter().map(describe_atomic).collect();
    ids.sort();
    ids.join("|")
}

fn describe_atomic(t: &TAtomic) -> String {
    t.get_id().to_string()
}

pub(crate) fn is_constant_array(t: &TAtomic) -> bool {
    match t {
        TAtomic::Array(TArray::Keyed(k)) => k.parameters.is_none(),
        TAtomic::Array(TArray::List(l)) => {
            l.known_elements.is_some()
                && l.element_type
                    .types
                    .iter()
                    .all(mago_codex::ttype::atomic::TAtomic::is_never)
        }
        _ => false,
    }
}

fn constant_entries(t: &TArray) -> Vec<(TAtomic, bool, &TUnion)> {
    match t {
        TArray::Keyed(k) => (k.known_items.iter().flat_map(|i| i.iter()))
            .map(|(key, (optional, item))| (key.to_atomic(), *optional, item))
            .collect(),
        TArray::List(l) => (l.known_elements.iter().flat_map(|e| e.iter()))
            .map(|(i, (optional, item))| {
                (
                    TAtomic::Scalar(TScalar::literal_int(*i as i64)),
                    *optional,
                    item,
                )
            })
            .collect(),
    }
}

fn unwrap_constant_array(t: &TArray) -> Vec<TAtomic> {
    let entries = constant_entries(t);
    if entries.is_empty() {
        return vec![array_of(TAtomic::Never, TAtomic::Never)];
    }
    let mut out = Vec::new();
    if entries.iter().all(|(_, optional, _)| *optional) {
        out.push(array_of(TAtomic::Never, TAtomic::Never));
    }
    let keys = combine(entries.iter().map(|(k, _, _)| k.clone()).collect());
    let items = combine(
        entries
            .iter()
            .flat_map(|(_, _, item)| union_order(item.types.as_ref()))
            .cloned()
            .collect(),
    );
    for (at, item) in items.into_iter().enumerate() {
        let key = keys
            .get(at)
            .cloned()
            .unwrap_or(TAtomic::Mixed(Default::default()));
        if let TAtomic::Array(a) = &item
            && is_constant_array(&item)
        {
            for inner in unwrap_constant_array(a) {
                out.push(array_of(key.clone(), inner));
            }
            continue;
        }
        out.push(array_of(key, item));
    }
    out
}

fn array_of(key: TAtomic, item: TAtomic) -> TAtomic {
    TAtomic::Array(TArray::Keyed(TKeyedArray::new_with_parameters(
        Arc::new(TUnion::from_atomic(key)),
        Arc::new(TUnion::from_atomic(item)),
    )))
}

fn combine(types: Vec<TAtomic>) -> Vec<TAtomic> {
    if let Some(mixed) = types.iter().find(|t| matches!(t, TAtomic::Mixed(_))) {
        return vec![mixed.clone()];
    }
    let has = |f: &dyn Fn(&TAtomic) -> bool| types.iter().any(f);
    let has_true = has(&|t| matches!(t, TAtomic::Scalar(TScalar::Bool(b)) if b.is_true()));
    let has_false = has(&|t| matches!(t, TAtomic::Scalar(TScalar::Bool(b)) if b.is_false()));
    let general_int =
        has(&|t| matches!(t, TAtomic::Scalar(TScalar::Integer(i)) if !i.is_literal()));
    let general_float =
        has(&|t| matches!(t, TAtomic::Scalar(TScalar::Float(f)) if !f.is_literal()));
    let general_string =
        has(&|t| matches!(t, TAtomic::Scalar(TScalar::String(s)) if !s.is_known_literal()));
    let general_bool = has(&|t| matches!(t, TAtomic::Scalar(TScalar::Bool(b)) if b.is_general()));
    let mut out: Vec<TAtomic> = Vec::new();
    for t in types {
        let t = match &t {
            TAtomic::Scalar(TScalar::Bool(b))
                if !b.is_general() && (general_bool || (has_true && has_false)) =>
            {
                TAtomic::Scalar(TScalar::bool())
            }
            TAtomic::Scalar(TScalar::Integer(i)) if i.is_literal() && general_int => continue,
            TAtomic::Scalar(TScalar::Float(f)) if f.is_literal() && general_float => continue,
            TAtomic::Scalar(TScalar::String(s)) if s.is_known_literal() && general_string => {
                continue;
            }
            _ => t,
        };
        if !out.iter().any(|o| o == &t) {
            out.push(t);
        }
    }
    out
}

fn corrected(t: &TAtomic) -> TAtomic {
    match t {
        TAtomic::Scalar(TScalar::String(s)) if !s.is_known_literal() && s.is_non_empty() => {
            TAtomic::Scalar(TScalar::string())
        }
        TAtomic::Array(TArray::Keyed(k)) if !is_constant_array(t) && k.non_empty => array_of(
            TAtomic::Mixed(Default::default()),
            TAtomic::Mixed(Default::default()),
        ),
        TAtomic::Array(TArray::List(l)) if !is_constant_array(t) => {
            if l.non_empty {
                array_of(
                    TAtomic::Mixed(Default::default()),
                    TAtomic::Mixed(Default::default()),
                )
            } else {
                TAtomic::Array(TArray::Keyed(TKeyedArray::new_with_parameters(
                    Arc::new(TUnion::from_atomic(TAtomic::Scalar(TScalar::int()))),
                    l.element_type.clone(),
                )))
            }
        }
        _ => t.clone(),
    }
}

fn returned_native_type(
    place: &Place<'_, '_, '_>,
    codebase: &CodebaseMetadata,
    value: &Expression<'_>,
) -> Option<Vec<TAtomic>> {
    if let Expression::AnonymousClass(class) = unparenthesized(value)
        && let Some(parent) = anonymous_parents(place.ancestors, class).first()
    {
        let class = codebase.get_class_like(parent.as_bytes())?;
        return Some(vec![TAtomic::Object(TObject::Named(TNamedObject::new(
            class.name,
        )))]);
    }
    let t = native_type(place.facts, value)?;
    Some(match t.types.as_ref() {
        [one] => vec![corrected(one)],
        many => merged_arrays(union_order(many).into_iter().cloned().collect()),
    })
}

pub(crate) fn infer_returns(
    place: &Place<'_, '_, '_>,
    body: &Block<'_>,
    returns: &[&Return<'_>],
) -> Created {
    if returns.is_empty() {
        return Created::Mixed;
    }
    let codebase = &place.facts.codebase;
    let mut types = Vec::new();
    let mut has_void = false;
    for r in returns {
        match r.value {
            None => {
                has_void = true;
                types.push(TAtomic::Void);
            }
            Some(value) => match returned_native_type(place, codebase, value) {
                Some(t) => types.extend(t),
                None => return Created::Mixed,
            },
        }
    }
    if !has_void && has_silent_void(place, body.statements.iter()) {
        types.push(TAtomic::Void);
    }
    match create_keep_constant(types) {
        Created::Single(TAtomic::Void) => Created::Mixed,
        created => created,
    }
}

pub(crate) fn is_union_without_null(created: &Created) -> bool {
    matches!(created, Created::Union(types) if types.iter().filter(|t| !matches!(t, TAtomic::Null)).count() > 1)
}

pub(crate) fn without_covered_objects<'a>(
    types: Vec<&'a TAtomic>,
    codebase: &CodebaseMetadata,
) -> Vec<&'a TAtomic> {
    let object = |t: &TAtomic| match t {
        TAtomic::Object(TObject::Named(n)) if n.type_parameters.is_none() => Some(n.clone()),
        _ => None,
    };
    let covers = |b: &TNamedObject, a: &TNamedObject| {
        let late = |n: &TNamedObject| n.is_this || n.is_static;
        if late(b) {
            return b.is_static && a.is_this && a.name == b.name;
        }
        codebase.is_instance_of(a.name.as_bytes(), b.name.as_bytes())
            && (late(a) || a.name != b.name)
    };
    let objects: Vec<Option<TNamedObject>> = types.iter().map(|t| object(t)).collect();
    types
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            let Some(a) = &objects[*i] else {
                return true;
            };
            !objects
                .iter()
                .enumerate()
                .any(|(j, b)| j != *i && b.as_ref().is_some_and(|b| covers(b, a)))
        })
        .map(|(_, t)| *t)
        .collect()
}

pub(crate) fn merged_arrays(types: Vec<TAtomic>) -> Vec<TAtomic> {
    let general = |t: &TAtomic| matches!(t, TAtomic::Array(_)) && !is_constant_array(t);
    if types.iter().filter(|t| general(t)).count() < 2 {
        return types;
    }
    let non_empty = types.iter().filter(|t| general(t)).all(|t| {
        matches!(
            t,
            TAtomic::Array(TArray::Keyed(TKeyedArray {
                non_empty: true,
                ..
            }))
        ) || matches!(t, TAtomic::Array(TArray::List(l)) if l.non_empty)
    });
    let mut out = Vec::new();
    let mut merged = false;
    for t in types {
        if !general(&t) {
            out.push(t);
        } else if !merged {
            merged = true;
            let mut array = TKeyedArray::new_with_parameters(
                Arc::new(TUnion::from_atomic(TAtomic::Mixed(Default::default()))),
                Arc::new(TUnion::from_atomic(TAtomic::Mixed(Default::default()))),
            );
            array.non_empty = non_empty;
            out.push(TAtomic::Array(TArray::Keyed(array)));
        }
    }
    out
}
