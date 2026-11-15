use super::{
    Accessory, NeverKind, Template, Tri, Type, contains_template, intersect_object_exclusion,
    is_super_type_of, mixed, to_benevolent, union, unresolvable,
};

pub fn intersect(types: Vec<Type>) -> Type {
    if types.len() <= 1 {
        return types
            .into_iter()
            .next()
            .unwrap_or(Type::Never(NeverKind::Implicit));
    }
    if let Some(never) = types.iter().find(|t| t.is_implicit_never()) {
        return never.clone();
    }
    if types.iter().any(|t| {
        matches!(t, Type::LateArrayShape(_) | Type::OffsetAccess(..)) && contains_template(t)
    }) {
        return Type::Intersection(types);
    }
    let members = |t: &Type| -> Option<Vec<Type>> {
        match t {
            Type::Union(inner) | Type::Benevolent(inner) => Some(inner.clone()),
            Type::Template(tpl) => match &tpl.bound {
                Type::Union(inner) | Type::Benevolent(inner) => Some(inner.clone()),
                _ => None,
            },
            _ => None,
        }
    };
    let unions: Vec<usize> = (0..types.len())
        .filter(|&i| members(&types[i]).is_some())
        .collect();
    let pick = if unions.len() >= 2 {
        unions
            .iter()
            .find(|&&i| matches!(types[i], Type::Template(_)))
            .or_else(|| {
                unions
                    .iter()
                    .find(|&&i| matches!(types[i], Type::Benevolent(_)))
            })
            .or(unions.first())
            .copied()
    } else {
        unions.first().copied()
    };
    if let Some(i) = pick {
        let inner = members(&types[i]).expect("union");
        let others: Vec<Type> = types
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, t)| t.clone())
            .collect();
        let parts = inner
            .into_iter()
            .map(|t| {
                let mut v = vec![t];
                v.extend(others.iter().cloned());
                intersect(v)
            })
            .collect();
        let mut u = union(parts);
        if u.is_never() {
            return u;
        }
        let benevolent = match &types[i] {
            Type::Template(tpl) => matches!(tpl.bound, Type::Benevolent(_)),
            t => matches!(t, Type::Benevolent(_)),
        };
        if benevolent {
            u = to_benevolent(u);
        }
        if let Type::Template(tpl) = &types[i] {
            return Type::Template(Box::new(Template {
                bound: template_bound(u),
                ..(**tpl).clone()
            }));
        }
        return u;
    }
    let mut flat = Vec::new();
    for t in types {
        match t {
            Type::Intersection(inner) => flat.extend(inner),
            t => flat.push(t),
        }
    }
    let mut types = flat;
    types.sort_by_key(|t| !matches!(t.array_class(), Type::ConstArray(_)));
    let mut i = 0;
    'outer: while i < types.len() {
        let mut j = i + 1;
        'inner: while j < types.len() {
            if let Some(t) = intersect_object_exclusion(&types[j], &types[i]) {
                types[i] = t;
                types.remove(j);
                continue;
            }
            if let Some(t) = intersect_object_exclusion(&types[i], &types[j]) {
                types[j] = t;
                types.remove(i);
                continue 'outer;
            }
            if let Some(t) = mixed::intersection(&types[i], &types[j])
                .or_else(|| mixed::intersection(&types[j], &types[i]))
            {
                types[i] = t;
                types.remove(j);
                continue;
            }
            if let Type::IntRange(min, max) = types[i]
                && let Some(t) = range_try_intersect(min, max, &types[j])
            {
                types[j] = t;
                types.remove(i);
                continue 'outer;
            }
            let mixedish = |t: &Type| matches!(t, Type::Mixed { .. } | Type::Error);
            if mixedish(&types[i]) && mixedish(&types[j]) {
                let explicit = Type::Mixed { explicit: true };
                if types[j] != explicit || types[i] == explicit {
                    types.remove(j);
                    continue;
                }
                types.remove(i);
                continue 'outer;
            }
            let (a_super, a_reasons) = if let Type::Iterable(key, value) = types[j].array_class() {
                (iterable_super_of_mixed(key, value, &types[i]), Vec::new())
            } else {
                unresolvable::compare(&types[j], &types[i])
            };
            if a_super.yes() {
                types.remove(j);
                continue;
            }
            let b_super = if let Type::Iterable(key, value) = types[i].array_class() {
                iterable_super_of_mixed(key, value, &types[j])
            } else {
                unresolvable::compare(&types[i], &types[j]).0
            };
            if b_super.maybe() {
                if let (Type::ClassString(Some(x)), Type::ClassString(Some(y))) =
                    (&types[i], &types[j])
                {
                    types[i] = Type::ClassString(Some(Box::new(intersect(vec![
                        (**x).clone(),
                        (**y).clone(),
                    ]))));
                    types.remove(j);
                    continue;
                }
                if let Type::ConstArray(ca) = types[i].array_class() {
                    let merged = match &types[j] {
                        Type::Accessory(Accessory::List) => Some(ca.make_list()),
                        Type::Accessory(Accessory::NonEmptyArray)
                            if (ca.keys.len() == 1 || ca.is_list.yes())
                                && ca.is_optional_key(0)
                                && !ca.is_unsealed().yes() =>
                        {
                            Some(ca.make_first_offset_required())
                        }
                        _ => None,
                    };
                    if let Some(merged) = merged {
                        let bound = Type::ConstArray(Box::new(merged));
                        types[i] = match &types[i] {
                            Type::Template(t) => Type::Template(Box::new(Template {
                                bound,
                                ..(**t).clone()
                            })),
                            _ => bound,
                        };
                        types.remove(j);
                        continue 'inner;
                    }
                }
                if let (Type::ConstArray(ca), Type::Array(..)) =
                    (types[i].array_class(), types[j].array_class())
                {
                    types[i] = ca.intersect_array(&types[j]);
                    types.remove(j);
                    i += 1;
                    continue 'outer;
                }
                if matches!(
                    types[i].array_class(),
                    Type::Array(..) | Type::Iterable(..) | Type::ConstArray(_)
                ) && let Type::Array(jk, _) | Type::Iterable(jk, _) = types[j].array_class()
                {
                    let key = intersect(vec![types[i].iterable_key_type(), (**jk).clone()]);
                    let item = intersect(vec![
                        types[i].iterable_value_type(),
                        types[j].iterable_value_type(),
                    ]);
                    types[j] = if matches!(
                        (types[i].array_class(), types[j].array_class()),
                        (Type::Iterable(..), Type::Iterable(..))
                    ) {
                        Type::Iterable(Box::new(key), Box::new(item))
                    } else {
                        Type::array(key, item)
                    };
                    types.remove(i);
                    continue 'outer;
                }
                if let (Type::Array(..), Type::Accessory(Accessory::List)) = (&types[i], &types[j])
                    && !is_super_type_of(
                        &types[j].iterable_key_type(),
                        &types[i].iterable_key_type(),
                    )
                    .yes()
                {
                    let key = intersect(vec![
                        types[i].iterable_key_type(),
                        types[j].iterable_key_type(),
                    ]);
                    let item = types[i].iterable_value_type();
                    types[i] = Type::array(key, item);
                    j += 1;
                    continue;
                }
                j += 1;
                continue;
            }
            if b_super.yes() {
                types.remove(i);
                continue 'outer;
            }
            if a_super.no() {
                let never = Type::Never(NeverKind::Implicit);
                return a_reasons
                    .into_iter()
                    .next()
                    .map_or(never.clone(), |reason| {
                        never.with_unresolvable_reason(reason)
                    });
            }
            j += 1;
        }
        i += 1;
    }
    if types.len() == 1 {
        return types.pop().expect("one type");
    }
    Type::Intersection(types)
}

fn template_bound(bound: Type) -> Type {
    match bound {
        Type::Object(..)
        | Type::EnumCase(..)
        | Type::ObjectWithoutClass
        | Type::Array(..)
        | Type::ConstArray(_)
        | Type::ObjectShape(_)
        | Type::String
        | Type::ConstString(_)
        | Type::Int
        | Type::IntRange(..)
        | Type::ConstInt(_)
        | Type::Float
        | Type::Bool
        | Type::ConstBool(_)
        | Type::Mixed { .. }
        | Type::Union(_)
        | Type::Benevolent(_)
        | Type::Intersection(_)
        | Type::Iterable(..)
        | Type::Null => bound,
        _ => Type::Mixed { explicit: true },
    }
}

fn iterable_super_of_mixed(key: &Type, value: &Type, b: &Type) -> Tri {
    let nested = |x: &Type, y: &Type| match (x, y) {
        (Type::Mixed { explicit: true }, Type::Mixed { explicit: false }) => Tri::Maybe,
        (Type::Mixed { .. }, Type::Mixed { .. }) => Tri::Yes,
        _ => is_super_type_of(x, y),
    };
    b.is_iterable()
        .and(nested(value, &b.iterable_value_type()))
        .and(nested(key, &b.iterable_key_type()))
}

fn range_try_intersect(min: Option<i64>, max: Option<i64>, other: &Type) -> Option<Type> {
    let (omin, omax) = other.int_bounds()?;
    let bound = |x: Option<i64>, y: Option<i64>, tighter: fn(i64, i64) -> i64| match (x, y) {
        (Some(x), Some(y)) => Some(tighter(x, y)),
        (x, y) => x.or(y),
    };
    Some(Type::int_range(
        bound(min, omin, i64::max),
        bound(max, omax, i64::min),
    ))
}
