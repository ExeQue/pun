use std::cmp::Ordering;

use super::{
    Accessory, ConstArray, Level, NeverKind, Tri, Type, intersect, is_super_type_of, mixed, sort,
};

pub fn sort_types(types: &[Type]) -> Vec<Type> {
    if types.len() > 1024 {
        return types.to_vec();
    }
    sort::types(types)
}

pub(super) fn compare_strings(a: impl AsRef<[u8]>, b: impl AsRef<[u8]>) -> Ordering {
    let (a, b) = (a.as_ref(), b.as_ref());
    let cmp = a.to_ascii_lowercase().cmp(&b.to_ascii_lowercase());
    if cmp != Ordering::Equal {
        cmp
    } else {
        a.cmp(b)
    }
}

pub(super) fn compare_in_union(a: &Type, b: &Type) -> Ordering {
    let a_class = a.scalar_class_type();
    let b_class = b.scalar_class_type();
    if matches!(a_class, Type::Null) {
        return Ordering::Greater;
    } else if matches!(b_class, Type::Null) {
        return Ordering::Less;
    }
    match (a, b) {
        (Type::Accessory(_), Type::Accessory(_)) => {
            return compare_strings(
                a.describe_bytes(Level::Value),
                b.describe_bytes(Level::Value),
            );
        }
        (Type::Accessory(_), _) => return Ordering::Greater,
        (_, Type::Accessory(_)) => return Ordering::Less,
        _ => {}
    }
    let a_bool = matches!(a, Type::ConstBool(_));
    let b_bool = matches!(b, Type::ConstBool(_));
    if a_bool != b_bool {
        return a_bool.cmp(&b_bool);
    }
    let constant_class = |t: &Type| {
        matches!(
            t,
            Type::ConstBool(_) | Type::ConstInt(_) | Type::ConstFloat(_) | Type::ConstString(_)
        )
    };
    let a_scalar = constant_class(a_class);
    let b_scalar = constant_class(b_class);
    if a_scalar != b_scalar {
        return b_scalar.cmp(&a_scalar);
    }
    let number = |t: &Type| match t.scalar_class_type() {
        Type::ConstInt(i) => Some(*i as f64),
        Type::ConstFloat(f) => Some(*f),
        _ => None,
    };
    if let (Some(x), Some(y)) = (number(a), number(b)) {
        let cmp = x.partial_cmp(&y).unwrap_or(Ordering::Equal);
        if cmp != Ordering::Equal {
            return cmp;
        }
        return matches!(b_class, Type::ConstInt(_)).cmp(&matches!(a_class, Type::ConstInt(_)));
    }
    match (a_class, b_class) {
        (Type::IntRange(a_min, _), Type::IntRange(b_min, _)) => {
            return a_min.unwrap_or(i64::MIN).cmp(&b_min.unwrap_or(i64::MIN));
        }
        (Type::IntRange(..), Type::Int | Type::ConstInt(_)) => return Ordering::Greater,
        (Type::Int | Type::ConstInt(_), Type::IntRange(..)) => return Ordering::Less,
        (Type::ConstString(x), Type::ConstString(y)) => {
            return x
                .to_ascii_lowercase()
                .cmp(&y.to_ascii_lowercase())
                .then_with(|| x.cmp(y));
        }
        _ => {}
    }
    let callable = |t: &Type| matches!(t, Type::Callable(_) | Type::Closure(_));
    if callable(a) && callable(b) {
        return compare_strings(
            a.describe_bytes(Level::Value),
            b.describe_bytes(Level::Value),
        );
    }
    if a.is_constant_array().yes() && b.is_constant_array().yes() {
        let empty = |t: &Type| t.is_iterable_at_least_once().no();
        return empty(b).cmp(&empty(a)).then_with(|| {
            compare_strings(
                a.describe_bytes(Level::Value),
                b.describe_bytes(Level::Value),
            )
        });
    }
    if a.is_string().yes() && b.is_string().yes() {
        return compare_strings(
            a.describe_bytes(Level::Precise),
            b.describe_bytes(Level::Precise),
        );
    }
    compare_strings(
        a.describe_bytes(Level::TypeOnly),
        b.describe_bytes(Level::TypeOnly),
    )
}

pub fn union(types: Vec<Type>) -> Type {
    let mut types = types;
    if types.len() == 1
        && !matches!(types[0], Type::Union(_) | Type::Benevolent(_))
        && !types[0].is_array().yes()
    {
        return types.pop().expect("one type");
    }
    let mut benevolent: Vec<String> = Vec::new();
    let mut flat = Vec::new();
    for t in types {
        match t {
            Type::Mixed { explicit: false } => return t,
            _ if t.is_implicit_never() => {}
            Type::Benevolent(inner) => {
                for i in &inner {
                    benevolent.push(i.describe(Level::Value));
                }
                flat.extend(inner);
            }
            Type::Union(inner) => flat.extend(inner),
            t => flat.push(t),
        }
    }
    let mut types = flat;
    if types.is_empty() {
        return Type::Never(NeverKind::Implicit);
    }
    if types.len() == 1 && !types[0].is_array().yes() {
        return types.pop().expect("one type");
    }
    let mut arrays = Vec::new();
    let mut scalars: Vec<(u8, Vec<Type>)> = Vec::new();
    let mut generic = [false; 5];
    let mut ranges = Vec::new();
    let mut rest = Vec::new();
    for t in types {
        if t.is_constant_scalar() {
            let class = scalar_class(&t);
            match scalars.iter_mut().find(|(c, _)| *c == class) {
                Some((_, items)) => {
                    if !items.iter().any(|other| {
                        if matches!(t, Type::Template(_)) {
                            other.describe(Level::Precise) == t.describe(Level::Precise)
                        } else {
                            other == &t
                        }
                    }) {
                        items.push(t);
                    }
                }
                None => scalars.push((class, vec![t])),
            }
            continue;
        }
        if t.is_boolean().yes() {
            generic[0] |= is_super_type_of(&t, &Type::Bool).yes();
        } else if t.is_float().yes() {
            generic[2] |= is_super_type_of(&t, &Type::Float).yes();
        } else if t.is_integer().yes() && !matches!(t, Type::IntRange(..)) {
            generic[1] |= is_super_type_of(&t, &Type::Int).yes();
        }
        if let Type::IntRange(..) = t {
            ranges.push(t);
            continue;
        }
        if t.is_array().yes() {
            arrays.push(t);
            continue;
        }
        rest.push(t);
    }
    ranges.sort_by_key(|t| {
        let (min, max) = t.int_bounds().unwrap_or_default();
        (min.unwrap_or(i64::MIN), max.unwrap_or(i64::MAX))
    });
    let mut types = rest;
    types.extend(ranges);
    let mut kept_scalars: Vec<Type> = Vec::new();
    for (class, mut items) in scalars {
        if class < 4 && generic[class as usize] {
            continue;
        }
        if class == 0 && items.len() == 2 {
            types.push(Type::Bool);
            continue;
        }
        let mut i = 0;
        while i < types.len() {
            let mut j = 0;
            let mut removed_i = false;
            while j < items.len() {
                match compare_in_union_merge(&types[i], &items[j]) {
                    Some(Merge::A(t)) => {
                        types[i] = t;
                        items.remove(j);
                        j = 0;
                    }
                    Some(Merge::B(t)) => {
                        items[j] = t;
                        types.remove(i);
                        removed_i = true;
                        break;
                    }
                    None => j += 1,
                }
            }
            if !removed_i {
                i += 1;
            }
        }
        kept_scalars.extend(items);
    }
    types.extend(process_arrays(arrays));
    let mut i = 0;
    'outer: while i < types.len() {
        let mut j = i + 1;
        while j < types.len() {
            match compare_in_union_merge(&types[i], &types[j]) {
                Some(Merge::A(t)) => {
                    types[i] = t;
                    types.remove(j);
                }
                Some(Merge::B(t)) => {
                    types[j] = t;
                    types.remove(i);
                    continue 'outer;
                }
                None => j += 1,
            }
        }
        i += 1;
    }
    types.extend(kept_scalars);
    if types.len() == 1 {
        return types.pop().expect("one type");
    }
    if !benevolent.is_empty()
        && types
            .iter()
            .all(|t| benevolent.contains(&t.describe(Level::Value)))
    {
        return Type::Benevolent(types);
    }
    Type::Union(types)
}

fn scalar_class(t: &Type) -> u8 {
    match t {
        Type::ConstBool(_) => 0,
        Type::ConstInt(_) => 1,
        Type::ConstFloat(_) => 2,
        Type::ConstString(_) => 3,
        Type::Template(t) if matches!(t.bound, Type::ConstInt(_)) => 5,
        Type::Template(t) if matches!(t.bound, Type::ConstString(_)) => 6,
        Type::Template(_) if matches!(t.scalar_class_type(), Type::Union(_)) => 8,
        Type::Template(_) if matches!(t.scalar_class_type(), Type::Benevolent(_)) => 9,
        Type::Template(_) => 7,
        _ => 4,
    }
}

enum Merge {
    A(Type),
    B(Type),
}

pub fn remove_object(from: &Type, removed: &Type) -> Type {
    if let Type::Union(parts) = removed {
        return parts
            .iter()
            .fold(from.clone(), |from, part| remove_object(&from, part));
    }
    match is_super_type_of(removed, from) {
        Tri::Yes => Type::Never(NeverKind::Implicit),
        Tri::No => from.clone(),
        Tri::Maybe => remove_object_exclusion(from, removed).unwrap_or_else(|| from.clone()),
    }
}

fn remove_object_exclusion(from: &Type, removed: &Type) -> Option<Type> {
    if removed.is_compound() {
        return None;
    }
    if let Type::Union(parts) | Type::Benevolent(parts) = from {
        return Some(union(
            parts
                .iter()
                .map(|part| remove_object_exclusion(part, removed))
                .collect::<Option<_>>()?,
        ));
    }
    let (object, previous) = match from {
        Type::SubtractedObject { object, excluded } => (object.as_ref(), Some(excluded.as_ref())),
        from => (from, None),
    };
    let Type::Object(class, args, _) = object else {
        return None;
    };
    match is_super_type_of(removed, from) {
        Tri::Yes => return Some(Type::Never(NeverKind::Implicit)),
        Tri::No => return Some(from.clone()),
        Tri::Maybe => {}
    }
    let removed_object = match removed {
        Type::SubtractedObject { object, .. } => object.as_ref(),
        removed => removed,
    };
    if let Type::Object(removed_class, _, _) = removed_object {
        let complement = match (class.as_str(), removed_class.as_str()) {
            ("Throwable", "Exception") => Some("Error"),
            ("Throwable", "Error") => Some("Exception"),
            ("DateTimeInterface", "DateTime") => Some("DateTimeImmutable"),
            ("DateTimeInterface", "DateTimeImmutable") => Some("DateTime"),
            _ => None,
        };
        if let Some(complement) = complement {
            return Some(Type::Object(complement.into(), vec![], false));
        }
    }
    if !is_super_type_of(from, removed).yes() {
        return Some(from.clone());
    }
    let excluded = match previous {
        Some(previous) => union(vec![previous.clone(), removed.clone()]),
        None => removed.clone(),
    };
    Some(Type::object_subtracted(
        Type::Object(class.clone(), args.clone(), false),
        excluded,
    ))
}

fn restore_object_exclusion(positive: &Type, excluded: &Type, other: &Type) -> Option<Type> {
    if !is_super_type_of(excluded, other).yes() {
        return None;
    }
    let remaining = remove_object_exclusion(excluded, other)?;
    let remaining = if remaining.is_never() || !is_super_type_of(&remaining, other).no() {
        Type::Never(NeverKind::Implicit)
    } else {
        remaining
    };
    Some(match positive {
        Type::Mixed { explicit } => Type::mixed_subtracted(*explicit, remaining),
        _ => Type::object_subtracted(positive.clone(), remaining),
    })
}

pub(super) fn intersect_object_exclusion(a: &Type, b: &Type) -> Option<Type> {
    let Type::SubtractedObject { object, excluded } = a else {
        return None;
    };
    if !is_super_type_of(object, b).yes() {
        return None;
    }
    mixed::intersection(
        &Type::SubtractedMixed {
            explicit: false,
            excluded: excluded.clone(),
        },
        b,
    )
}

fn compare_in_union_merge(a: &Type, b: &Type) -> Option<Merge> {
    let super_type = |a: &Type, b: &Type| {
        if (a.is_constant_scalar() && !matches!(a, Type::Template(_)))
            || matches!(a, Type::IntRange(..))
        {
            is_super_type_of(a, b.scalar_class_type())
        } else {
            is_super_type_of(a, b)
        }
    };
    if let Type::SubtractedMixed { explicit, excluded } = a {
        return Some(Merge::A(
            restore_object_exclusion(
                &Type::Mixed {
                    explicit: *explicit,
                },
                excluded,
                b,
            )
            .unwrap_or_else(|| mixed::join(*explicit, excluded, b)),
        ));
    }
    if let Type::SubtractedMixed { explicit, excluded } = b {
        return Some(Merge::B(
            restore_object_exclusion(
                &Type::Mixed {
                    explicit: *explicit,
                },
                excluded,
                a,
            )
            .unwrap_or_else(|| mixed::join(*explicit, excluded, a)),
        ));
    }
    if let Type::SubtractedObject { object, excluded } = a
        && is_super_type_of(object, b).yes()
        && let Some(joined) = restore_object_exclusion(object, excluded, b)
            .or_else(|| mixed::join_object(object, excluded, b))
    {
        return Some(Merge::A(joined));
    }
    if let Type::SubtractedObject { object, excluded } = b
        && is_super_type_of(object, a).yes()
        && let Some(joined) = restore_object_exclusion(object, excluded, a)
            .or_else(|| mixed::join_object(object, excluded, a))
    {
        return Some(Merge::B(joined));
    }
    if let Type::IntRange(min, max) = a
        && let Some(t) = range_try_union(*min, *max, b)
    {
        return Some(Merge::A(t));
    }
    if let Type::IntRange(min, max) = b
        && let Some(t) = range_try_union(*min, *max, a)
    {
        return Some(Merge::B(t));
    }
    if matches!(a, Type::IntRange(..)) && matches!(b, Type::IntRange(..)) {
        return None;
    }
    if a.is_constant_array().yes() && b.is_constant_array().yes() {
        return None;
    }
    if let (Type::Iterable(ak, av), Type::Iterable(bk, bv)) = (a, b) {
        return Some(Merge::A(Type::Iterable(
            Box::new(union(vec![(**ak).clone(), (**bk).clone()])),
            Box::new(union(vec![(**av).clone(), (**bv).clone()])),
        )));
    }
    let subtractable = |t: &Type| {
        let mut t = t;
        while let Type::Template(template) = t {
            t = &template.bound;
        }
        matches!(
            t,
            Type::Object(..)
                | Type::Mixed { .. }
                | Type::ObjectWithoutClass
                | Type::Static(..)
                | Type::This(_)
                | Type::EnumCase(..)
        )
    };
    if subtractable(a) && super_type(a, b).yes() {
        return Some(Merge::A(a.clone()));
    }
    if subtractable(b) && super_type(b, a).yes() {
        return Some(Merge::B(b.clone()));
    }
    if super_type(b, a).yes() {
        return Some(Merge::B(b.clone()));
    }
    if super_type(a, b).yes() {
        return Some(Merge::A(a.clone()));
    }
    if let Type::ConstString(s) = b
        && s.is_empty()
        && matches!(
            a.describe(Level::Value).as_str(),
            "non-empty-string" | "non-falsy-string"
        )
    {
        return Some(Merge::A(intersect(case_accessories(a))));
    }
    if let Type::ConstString(s) = b
        && s.as_slice() == b"0"
        && let Some(t) = downgrade_non_falsy(a)
    {
        return Some(Merge::A(t));
    }
    None
}

fn downgrade_non_falsy(t: &Type) -> Option<Type> {
    let non_falsy = Type::Accessory(Accessory::NonFalsyString);
    let rest = match t {
        Type::Intersection(types) if types.contains(&non_falsy) => {
            types.iter().filter(|x| **x != non_falsy).cloned().collect()
        }
        _ => return None,
    };
    let without = intersect(rest);
    Some(if without.is_non_empty_string().yes() {
        without
    } else {
        intersect(vec![without, Type::Accessory(Accessory::NonEmptyString)])
    })
}

fn case_accessories(t: &Type) -> Vec<Type> {
    let mut v = vec![Type::String];
    if t.is_lowercase_string().yes() {
        v.push(Type::Accessory(Accessory::LowercaseString));
    }
    if t.is_uppercase_string().yes() {
        v.push(Type::Accessory(Accessory::UppercaseString));
    }
    v
}

fn range_try_union(min: Option<i64>, max: Option<i64>, other: &Type) -> Option<Type> {
    let (omin, omax) = other.int_bounds()?;
    let disjoint = matches!((min, omax), (Some(x), Some(y)) if x > y.saturating_add(1))
        || matches!((max, omin), (Some(x), Some(y)) if x.saturating_add(1) < y);
    if disjoint {
        return None;
    }
    Some(Type::int_range(
        min.zip(omin).map(|(x, y)| x.min(y)),
        max.zip(omax).map(|(x, y)| x.max(y)),
    ))
}

impl Type {
    fn general_arrays(&self) -> Vec<(Type, Type)> {
        match self {
            Type::Array(_, item) => vec![(self.iterable_key_type(), (**item).clone())],
            Type::Intersection(types) => types.iter().flat_map(Type::general_arrays).collect(),
            Type::Template(t) if matches!(t.bound, Type::Intersection(_)) => {
                t.bound.general_arrays()
            }
            _ => vec![],
        }
    }
}

fn process_arrays(arrays: Vec<Type>) -> Vec<Type> {
    if arrays.is_empty() {
        return vec![];
    }
    let accessories = common_array_accessories(&arrays);
    let with = |t: Type, accessories: &[Type]| {
        let mut all = vec![t];
        all.extend(accessories.iter().cloned());
        intersect(all)
    };
    if arrays.len() == 1 {
        return vec![with(
            arrays.into_iter().next().expect("one array"),
            &accessories,
        )];
    }
    let (mut keys, mut values, mut seen) = (Vec::new(), Vec::new(), Vec::<Type>::new());
    let mut general = false;
    for a in &arrays {
        let constant = a.constant_arrays();
        if constant.is_empty() {
            for (key, item) in a.general_arrays() {
                keys.push(key);
                values.push(item);
                general = true;
            }
            continue;
        }
        for ca in constant {
            for (key, value) in ca.keys.iter().zip(&ca.values) {
                values.push(value.clone());
                if !seen.contains(key) {
                    keys.push(key.clone());
                    seen.push(key.clone());
                }
            }
        }
    }
    if general {
        return vec![with(Type::array(union(keys), union(values)), &accessories)];
    }
    reduce_arrays(&arrays)
        .into_iter()
        .map(|a| {
            let applied: Vec<Type> = if a.is_iterable_at_least_once().no() {
                accessories
                    .iter()
                    .filter(|t| **t != Type::Accessory(Accessory::NonEmptyArray))
                    .cloned()
                    .collect()
            } else {
                accessories.clone()
            };
            with(a, &applied)
        })
        .collect()
}

fn reduce_arrays(arrays: &[Type]) -> Vec<Type> {
    let mut retained = Vec::new();
    let mut to_process: Vec<Option<ConstArray>> = Vec::new();
    let mut empty = None;
    for a in arrays {
        if a.is_iterable_at_least_once().no() {
            empty = Some(a.clone());
            continue;
        }
        to_process.extend(a.constant_arrays().into_iter().map(Some));
    }
    if let Some(empty) = empty {
        match empty {
            Type::ConstArray(c) => to_process.push(Some(*c)),
            empty => retained.push(empty),
        }
    }
    let mut per_key: Vec<(Type, Vec<usize>)> = Vec::new();
    for (i, a) in to_process.iter().enumerate() {
        for key in &a.as_ref().expect("array").keys {
            match per_key.iter_mut().find(|(k, _)| k == key) {
                Some((_, v)) => v.push(i),
                None => per_key.push((key.clone(), vec![i])),
            }
        }
    }
    let mut eligible: Vec<(usize, Vec<(usize, usize)>)> = Vec::new();
    for (_, idx) in &per_key {
        for x in 0..idx.len().saturating_sub(1) {
            for y in x + 1..idx.len() {
                let pos = match eligible.iter().position(|(i, _)| *i == idx[x]) {
                    Some(p) => p,
                    None => {
                        eligible.push((idx[x], vec![]));
                        eligible.len() - 1
                    }
                };
                match eligible[pos].1.iter_mut().find(|(j, _)| *j == idx[y]) {
                    Some((_, c)) => *c += 1,
                    None => eligible[pos].1.push((idx[y], 1)),
                }
            }
        }
    }
    for (i, others) in &eligible {
        for &(j, overlap) in others {
            let (Some(a), Some(b)) = (&to_process[*i], &to_process[j]) else {
                if to_process[*i].is_none() {
                    break;
                }
                continue;
            };
            if overlap == 1
                && a.keys.len() == 1
                && b.keys.len() == 1
                && !matches!(
                    union(vec![a.values[0].clone(), b.values[0].clone()]),
                    Type::Union(_)
                )
            {
                to_process[j] = Some(b.merge_with(a));
                to_process[*i] = None;
                break;
            }
            if overlap == a.keys.len() && b.is_keys_superset_of(a) {
                to_process[j] = Some(b.merge_with(a));
                to_process[*i] = None;
                break;
            }
            if overlap == b.keys.len() && a.is_keys_superset_of(b) {
                to_process[*i] = Some(a.merge_with(b));
                to_process[j] = None;
            }
        }
    }
    retained
        .into_iter()
        .chain(
            to_process
                .into_iter()
                .flatten()
                .map(|a| Type::ConstArray(Box::new(a))),
        )
        .collect()
}

impl ConstArray {
    fn is_keys_superset_of(&self, other: &ConstArray) -> bool {
        let mut fail_on_different = self.keys.len() != other.keys.len() || self.keys.len() < 2;
        for (j, key) in other.keys.iter().enumerate() {
            let i = self
                .keys
                .iter()
                .position(|k| k == key)
                .expect("a shared key");
            if !is_super_type_of(&other.values[j], &self.values[i]).no() {
                continue;
            }
            if fail_on_different {
                return false;
            }
            fail_on_different = true;
        }
        let required = self
            .keys
            .iter()
            .enumerate()
            .filter(|(i, k)| !other.keys.contains(k) && !self.is_optional_key(*i))
            .count();
        required <= 1
    }

    fn merge_with(&self, other: &ConstArray) -> ConstArray {
        let mut merged = self.clone();
        for (i, key) in self.keys.iter().enumerate() {
            match other.keys.iter().position(|k| k == key) {
                None => merged.optional.push(i),
                Some(j) => {
                    if other.is_optional_key(j) {
                        merged.optional.push(i);
                    }
                    merged.values[i] =
                        union(vec![merged.values[i].clone(), other.values[j].clone()]);
                }
            }
        }
        let mut seen = Vec::new();
        merged.optional.retain(|i| {
            !seen.contains(i) && {
                seen.push(*i);
                true
            }
        });
        merged
            .next_auto_indexes
            .extend(other.next_auto_indexes.iter().copied());
        merged.next_auto_indexes.sort_unstable();
        merged.next_auto_indexes.dedup();
        merged.is_list = self.is_list.and(other.is_list);
        merged
    }
}

fn common_array_accessories(arrays: &[Type]) -> Vec<Type> {
    let mut found: Vec<(Type, Vec<usize>)> = Vec::new();
    let mut add = |t: Type, i: usize| match found.iter_mut().find(|(x, _)| *x == t) {
        Some((_, idx)) => {
            if !idx.contains(&i) {
                idx.push(i);
            }
        }
        None => found.push((t, vec![i])),
    };
    for (i, a) in arrays.iter().enumerate() {
        let intersection = match a {
            Type::Template(t) => &t.bound,
            _ => a,
        };
        if let Type::Intersection(types) = intersection {
            for t in types {
                if matches!(t, Type::Template(_)) {
                    break;
                }
                if matches!(t, Type::Accessory(_) | Type::Callable(_)) {
                    add(t.clone(), i);
                }
            }
        }
        if !a.is_constant_array().yes() {
            continue;
        }
        for ca in a.constant_arrays() {
            if ca.is_list.yes() {
                add(Type::Accessory(Accessory::List), i);
            }
            if ca.is_iterable_at_least_once().yes() {
                add(Type::Accessory(Accessory::NonEmptyArray), i);
            }
        }
    }
    let mut common: Vec<Type> = found
        .into_iter()
        .filter(|(_, idx)| idx.len() == arrays.len())
        .map(|(t, _)| t)
        .collect();
    if arrays.iter().all(|a| a.is_iterable_at_least_once().yes()) {
        common.push(Type::Accessory(Accessory::NonEmptyArray));
    }
    common
}
