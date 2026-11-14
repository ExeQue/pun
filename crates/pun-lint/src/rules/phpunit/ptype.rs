use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::array::TArray;
use mago_codex::ttype::atomic::array::key::ArrayKey;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{Expression, Literal};

use crate::Context;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Key {
    Int(i64),
    Str(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PType {
    Integer { constant: bool },
    Float { constant: bool },
    String { constant: Option<String> },
    Boolean,
    ConstantBoolean,
    ConstantArray { keys: Vec<Key>, values: Vec<PType> },
    Array,
    Object(String),
    Null,
    Mixed,
    Never,
    Union,
    Other,
}

impl PType {
    pub(crate) fn is_integer(&self) -> bool {
        matches!(self, PType::Integer { .. })
    }

    pub(crate) fn is_float(&self) -> bool {
        matches!(self, PType::Float { .. })
    }

    pub(crate) fn is_string(&self) -> bool {
        matches!(self, PType::String { .. })
    }
}

pub(crate) fn of(t: &TUnion) -> PType {
    let types: Vec<&TAtomic> = t.types.iter().collect();
    match types.as_slice() {
        [] => PType::Never,
        [atomic] => atomic_of(atomic),
        many if many
            .iter()
            .all(|a| matches!(a, TAtomic::Scalar(s) if s.is_bool())) =>
        {
            PType::Boolean
        }
        _ => PType::Union,
    }
}

pub(crate) fn without_null(t: &TUnion) -> PType {
    let mut kept = t.clone();
    let types: Vec<TAtomic> = t
        .types
        .iter()
        .filter(|a| !matches!(a, TAtomic::Null))
        .cloned()
        .collect();
    kept.types = types.into();
    of(&kept)
}

fn atomic_of(atomic: &TAtomic) -> PType {
    match atomic {
        TAtomic::Scalar(TScalar::Integer(i)) => PType::Integer {
            constant: i.is_literal(),
        },
        TAtomic::Scalar(TScalar::Float(f)) => PType::Float {
            constant: f.is_literal(),
        },
        TAtomic::Scalar(TScalar::String(s)) => PType::String {
            constant: s
                .get_known_literal_value()
                .map(|v| String::from_utf8_lossy(v).into_owned()),
        },
        TAtomic::Scalar(TScalar::ClassLikeString(_)) => PType::String { constant: None },
        TAtomic::Scalar(TScalar::Bool(b)) if b.is_general() => PType::Boolean,
        TAtomic::Scalar(TScalar::Bool(_)) => PType::ConstantBoolean,
        TAtomic::Array(TArray::Keyed(k)) if k.parameters.is_none() => {
            let (mut keys, mut values) = (Vec::new(), Vec::new());
            for (key, (_, value)) in k.known_items.iter().flat_map(|m| m.iter()) {
                keys.push(match key {
                    ArrayKey::Integer(i) => Key::Int(*i),
                    key => {
                        Key::Str(String::from_utf8_lossy(key.get_string().unwrap()).into_owned())
                    }
                });
                values.push(of(value));
            }
            PType::ConstantArray { keys, values }
        }
        TAtomic::Array(TArray::List(l)) if l.element_type.is_never() => {
            let (mut keys, mut values) = (Vec::new(), Vec::new());
            for (index, (_, value)) in l.known_elements.iter().flat_map(|m| m.iter()) {
                keys.push(Key::Int(*index as i64));
                values.push(of(value));
            }
            PType::ConstantArray { keys, values }
        }
        TAtomic::Array(_) => PType::Array,
        TAtomic::Object(TObject::Named(n))
            if n.name.as_bytes().starts_with(b"{anonymous-class:") =>
        {
            PType::Other
        }
        TAtomic::Object(TObject::Named(n)) if n.intersection_types.is_none() => {
            PType::Object(n.name.to_string())
        }
        TAtomic::Object(TObject::Enum(e)) => PType::Object(e.get_name().to_string()),
        TAtomic::Null => PType::Null,
        TAtomic::Mixed(_) => PType::Mixed,
        TAtomic::Never => PType::Never,
        _ => PType::Other,
    }
}

fn literal(expr: &Expression<'_>) -> Option<PType> {
    Some(match unparenthesize(expr) {
        Expression::Literal(Literal::Integer(_)) => PType::Integer { constant: true },
        Expression::Literal(Literal::Float(_)) => PType::Float { constant: true },
        Expression::Literal(Literal::String(s)) => PType::String {
            constant: s.value.map(|v| String::from_utf8_lossy(v).into_owned()),
        },
        Expression::Literal(Literal::True(_) | Literal::False(_)) => PType::ConstantBoolean,
        Expression::Literal(Literal::Null(_)) => PType::Null,
        _ => return None,
    })
}

pub(crate) fn native(ctx: &Context<'_>, expr: &Expression<'_>) -> Option<PType> {
    literal(expr).or_else(|| {
        facts(ctx)
            .native_type(node_span(unparenthesize(expr)))
            .map(of)
    })
}

pub(crate) fn native_without_null(ctx: &Context<'_>, expr: &Expression<'_>) -> Option<PType> {
    literal(expr)
        .map(|t| if t == PType::Null { PType::Never } else { t })
        .or_else(|| {
            facts(ctx)
                .native_type(node_span(unparenthesize(expr)))
                .map(without_null)
        })
}

pub(crate) fn native_is_integer(ctx: &Context<'_>, expr: &Expression<'_>) -> bool {
    native_parts(
        ctx,
        expr,
        mago_codex::ttype::atomic::scalar::TScalar::is_int,
    )
}

pub(crate) fn native_is_string(ctx: &Context<'_>, expr: &Expression<'_>) -> bool {
    native_parts(
        ctx,
        expr,
        mago_codex::ttype::atomic::scalar::TScalar::is_any_string,
    )
}

fn native_parts(ctx: &Context<'_>, expr: &Expression<'_>, part: fn(&TScalar) -> bool) -> bool {
    if let Some(t) = literal(expr) {
        return match t {
            PType::Integer { .. } => part(&TScalar::int()),
            PType::String { .. } => part(&TScalar::string()),
            _ => false,
        };
    }
    let facts = facts(ctx);
    let Some(t) = facts.native_type(node_span(unparenthesize(expr))) else {
        return false;
    };
    let parts: Vec<&TAtomic> = t
        .types
        .iter()
        .filter(|a| !matches!(a, TAtomic::Null))
        .collect();
    !parts.is_empty()
        && parts
            .iter()
            .all(|a| matches!(a, TAtomic::Scalar(s) if part(s)))
}
