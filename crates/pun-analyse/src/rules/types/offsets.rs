use mago_span::{HasSpan, Span};
use mago_syntax::cst::*;
use pun_phpstan::symbols;
use pun_phpstan::types::{Level, Tri, Type, is_super_type_of, mixed};

use mago_codex::identifier::method::MethodIdentifier;
use mago_word::word;

use super::super::{Checker, declared};
use super::flow::unparen;
use super::properties::with_args;

pub(super) const IDENTIFIERS: &[&str] = &["offsetAssign.dimType", "offsetAssign.valueType"];

fn bounds(c: &Checker, t: Type) -> Type {
    match t {
        Type::Object(name, args, exact) if args.iter().any(|a| matches!(a, Type::Never(_))) => {
            let class = c.codebase.get_class_like(name.as_bytes());
            let bound = |i: usize| {
                class
                    .and_then(|k| k.template_types.get_index(i))
                    .map(|(_, t)| c.to_phpstan(&t.constraint))
            };
            let args = args
                .into_iter()
                .enumerate()
                .map(|(i, a)| {
                    if matches!(a, Type::Never(_)) {
                        bound(i).unwrap_or(a)
                    } else {
                        a
                    }
                })
                .collect();
            Type::Object(name, args, exact)
        }
        t => t,
    }
}

fn dim<'e, 'arena>(
    e: &'e Expression<'arena>,
) -> Option<(&'e Expression<'arena>, Option<&'e Expression<'arena>>)> {
    match e {
        Expression::ArrayAccess(a) => Some((a.array, Some(a.index))),
        Expression::ArrayAppend(a) => Some((a.array, None)),
        _ => None,
    }
}

pub(super) fn typed(e: &Expression, spans: &mut Vec<Span>) {
    let Expression::Assignment(a) = e else { return };
    let mut at = a.lhs;
    while let Some((holder, offset)) = dim(at) {
        spans.extend(offset.map(|o| o.span()).into_iter().chain([holder.span()]));
        at = holder;
    }
    spans.extend([a.rhs.span(), e.span()]);
    if let Expression::UnaryPrefix(u) = a.rhs {
        spans.push(u.operand.span());
    }
}

fn class_accessible(t: &Type) -> Tri {
    const EXTRA: [&str; 11] = [
        "DOMNamedNodeMap",
        "Dom\\NamedNodeMap",
        "DOMNodeList",
        "Dom\\NodeList",
        "Dom\\HTMLCollection",
        "Dom\\DtdNamedNodeMap",
        "PDORow",
        "ResourceBundle",
        "FFI\\CData",
        "SimpleXMLElement",
        "Threaded",
    ];
    let is = |class: &str| is_super_type_of(&Type::Object(class.into(), vec![], false), t).yes();
    let class = t
        .object_class_names()
        .first()
        .and_then(|n| symbols::lookup(n));
    let Some(class) = class else {
        return Tri::Maybe;
    };
    if is("ArrayAccess") || EXTRA.iter().any(|c| is(c)) {
        Tri::Yes
    } else if class.is_interface
        || !(class.is_final || class.is_enum || matches!(t, Type::Object(_, _, true)))
    {
        Tri::Maybe
    } else {
        Tri::No
    }
}

pub(in super::super) fn accessible(t: &Type) -> Tri {
    match t {
        Type::Union(ts) | Type::Benevolent(ts) => {
            let all: Vec<Tri> = ts.iter().map(accessible).collect();
            if all.iter().all(|r| r.yes()) {
                Tri::Yes
            } else if all.iter().all(|r| r.no()) {
                Tri::No
            } else {
                Tri::Maybe
            }
        }
        Type::Intersection(ts) => ts.iter().map(accessible).fold(Tri::No, |a, b| {
            if a.yes() || b.yes() {
                Tri::Yes
            } else {
                a.or(b)
            }
        }),
        Type::Template(tt) if matches!(&tt.bound, Type::SubtractedMixed { .. }) => Tri::Maybe,
        Type::Template(tt) => accessible(&tt.bound),
        Type::SubtractedMixed { excluded, .. } => {
            let domain = pun_phpstan::types::union(vec![
                Type::String,
                Type::array(mixed(), mixed()),
                Type::Object("ArrayAccess".into(), vec![], false),
            ]);
            is_super_type_of(excluded, &domain).negate().and(Tri::Maybe)
        }
        Type::Conditional(c) => accessible(&c.resolve()),
        Type::OffsetAccess(on, index) => super::super::unset::late_offset(on, index)
            .map_or(Tri::Maybe, |resolved| accessible(&resolved)),
        Type::Object(..) | Type::Static(..) | Type::This(_) | Type::EnumCase(..) => {
            class_accessible(t)
        }
        Type::Accessory(pun_phpstan::types::Accessory::HasOffsetValue(..)) => Tri::Yes,
        Type::Mixed { .. }
        | Type::Error
        | Type::Wildcard
        | Type::ObjectWithoutClass
        | Type::ObjectShape(_)
        | Type::Iterable(..)
        | Type::Callable(_) => Tri::Maybe,
        _ if matches!(t, Type::Never(_))
            || t.is_string().yes()
            || t.is_array().yes()
            || t.is_null().yes() =>
        {
            Tri::Yes
        }
        _ => Tri::No,
    }
}

pub(in super::super) fn offset_set(c: &Checker, t: &Type) -> Option<(Type, Type)> {
    let names = t.object_class_names();
    let name = names.first()?;
    let args = if let Type::Object(_, a, _) | Type::Static(_, a) = t {
        a.as_slice()
    } else {
        &[]
    };
    let id = MethodIdentifier::new(
        c.codebase.get_class_like(name.as_bytes())?.name,
        word(b"offsetset"),
    );
    let id = c.codebase.get_declaring_method_identifier(&id);
    let method = c.codebase.get_method_by_id(&id)?;
    let declaring = c.codebase.get_class_like(id.get_class_name().as_bytes())?;
    let scope = format!("class {}", c.reflected_name(declaring.name.as_bytes()));
    let class = symbols::class(&c.reflected_name(declaring.name.as_bytes()));
    let names: Vec<String> = match class {
        Some(k) => k.templates.iter().map(|t| t.name.clone()).collect(),
        None => declaring
            .get_template_type_names()
            .iter()
            .map(|n| n.to_string())
            .collect(),
    };
    let builtin = class
        .and_then(|k| k.methods.get("offsetset"))
        .and_then(|m| m.variants.first());
    let param = |i: usize| {
        let ty = match builtin {
            Some(v) => pun_phpstan::resolve::resolve_str(&v.params.get(i)?.ty, None),
            None => method
                .parameters
                .get(i)?
                .type_metadata
                .as_ref()
                .map_or_else(mixed, |t| c.to_phpstan(&t.type_union)),
        };
        let same = c.reflected_name(declaring.name.as_bytes()) == c.reflected_name(name.as_bytes());
        Some(
            if same {
                with_args(&ty, &scope, &names, args)
            } else {
                ty
            }
            .resolve_to_bounds(),
        )
    };
    Some((param(0)?, param(1)?))
}

fn set_fails(c: &Checker, t: &Type, offset: Option<&Type>, value: &Type) -> bool {
    let string = |offset: Option<&Type>| {
        pun_phpstan::types::cast_is_error(
            value,
            pun_phpstan::types::Cast::String,
            Default::default(),
        ) || !offset.is_some_and(|o| {
            matches!(o, Type::Mixed { .. }) || is_super_type_of(&Type::Int, o).yes()
        })
    };
    match t {
        Type::Union(ts) | Type::Benevolent(ts) => ts.iter().any(|i| set_fails(c, i, offset, value)),
        Type::Intersection(ts) => ts.iter().all(|i| set_fails(c, i, offset, value)),
        Type::Template(tt) => set_fails(c, &tt.bound, offset, value),
        _ if t.is_string().yes() => string(offset),
        Type::ConstArray(a) => offset.is_none() && a.next_auto_indexes.is_empty(),
        Type::Object(..) | Type::Static(..) | Type::This(_) | Type::EnumCase(..) => {
            if class_accessible(t).no() {
                return true;
            }
            let is_array_access =
                is_super_type_of(&Type::Object("ArrayAccess".into(), vec![], false), t).yes();
            let Some((key, item)) = offset_set(c, t).filter(|_| is_array_access) else {
                return false;
            };
            let offset = offset.cloned().unwrap_or(Type::Null);
            (!matches!(offset, Type::Mixed { .. }) && !is_super_type_of(&key, &offset).yes())
                || (!matches!(value, Type::Mixed { .. }) && !is_super_type_of(&item, value).yes())
        }
        _ => accessible(t).no() || matches!(t, Type::Never(_)),
    }
}

fn has_no_offset(c: &Checker, t: &Type, offset: &Type) -> bool {
    match t {
        Type::Union(ts) | Type::Benevolent(ts) => ts.iter().all(|i| has_no_offset(c, i, offset)),
        Type::Intersection(ts) => ts.iter().any(|i| has_no_offset(c, i, offset)),
        _ if t.is_string().yes() => is_super_type_of(&Type::Int, offset).no(),
        _ if accessible(t).no() => true,
        Type::Object(..) | Type::Static(..) | Type::This(_) | Type::EnumCase(..) => {
            match offset_set(c, t).filter(|_| {
                is_super_type_of(&Type::Object("ArrayAccess".into(), vec![], false), t).yes()
            }) {
                Some((key, _)) => is_super_type_of(&key, offset).no(),
                None => class_accessible(t).no(),
            }
        }
        _ => false,
    }
}

fn has_offset(c: &Checker, holder: &Expression) -> bool {
    let Expression::Variable(Variable::Direct(v)) = unparen(holder) else {
        return false;
    };
    c.guards
        .contains(&super::super::property_access::offset_guard(v.name))
}

fn dim_type(c: &mut Checker, holder: &Expression, offset: Option<&Expression>, op: bool) {
    if has_offset(c, holder) {
        return;
    }
    let Some(potential) = offset.map_or(Some(None), |o| c.phpstan_type(o).map(Some)) else {
        return;
    };
    let mixed = mixed();
    let found = declared::with_classes(c, || {
        c.find_type_to_check(holder, &|t| !set_fails(c, t, potential.as_ref(), &mixed))
    });
    let Some(var) = found.ty.map(|t| bounds(c, t)) else {
        return;
    };
    if !op && !declared::with_classes(c, || accessible(&var).yes()) {
        return;
    }
    let ty = match offset {
        Some(o) => {
            let find = || {
                c.find_type_to_check(o, &|d| !set_fails(c, &var, Some(d), &mixed))
                    .ty
            };
            let Some(found) = declared::with_classes(c, find) else {
                return;
            };
            if op && declared::with_classes(c, || has_no_offset(c, &var, &found)) {
                return;
            }
            Some(found)
        }
        None => None,
    };
    if !declared::with_classes(c, || set_fails(c, &var, ty.as_ref(), &mixed)) {
        return;
    }
    let message = match ty {
        None => format!(
            "Cannot assign new offset to {}.",
            var.describe(Level::TypeOnly)
        ),
        Some(d) => format!(
            "Cannot assign offset {} to {}.",
            d.describe(Level::Value),
            var.describe(Level::TypeOnly)
        ),
    };
    c.report(holder, "offsetAssign.dimType", message, None);
}

pub(in super::super) fn assignment(c: &mut Checker, a: &Assignment) {
    if c.level < 3 {
        return;
    }
    let Some((holder, offset)) = dim(a.lhs) else {
        return;
    };
    let op = !matches!(a.operator, AssignmentOperator::Assign(_));
    if op {
        dim_type(c, holder, offset, true);
    } else {
        let mut at = a.lhs;
        while let Some((holder, offset)) = dim(at) {
            dim_type(c, holder, offset, false);
            at = holder;
        }
    }
    if has_offset(c, holder) {
        return;
    }
    let Some(original) = c.phpstan_type(holder).map(|t| bounds(c, t)) else {
        return;
    };
    if declared::with_classes(c, || {
        is_super_type_of(&Type::ObjectWithoutClass, &original).no()
    }) {
        return;
    }
    let value = match a.rhs {
        _ if op => c.types.get(&a.span()).map(|t| c.to_phpstan(t)),
        Expression::UnaryPrefix(u) if u.operator.is_reference() => c.phpstan_type(u.operand),
        rhs => c.phpstan_type(rhs),
    };
    let Some(value) = value else { return };
    let mixed = mixed();
    let fails =
        |c: &Checker, t: &Type| declared::with_classes(c, || set_fails(c, t, Some(&mixed), &value));
    let Some(var) = declared::with_classes(c, || {
        c.find_type_to_check(holder, &|t| !set_fails(c, t, Some(&mixed), &value))
            .ty
            .map(|t| bounds(c, t))
    }) else {
        return;
    };
    if !declared::with_classes(c, || accessible(&var).yes()) || !fails(c, &var) {
        return;
    }
    let message = format!(
        "{} does not accept {}.",
        original.describe(Level::Value),
        value.describe(Level::TypeOnly)
    );
    c.report(holder, "offsetAssign.valueType", message, None);
}
