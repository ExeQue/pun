use super::{Level, NeverKind, Type, is_super_type_of, union};

pub fn decide_type(ty: Type, phpdoc: Option<Type>) -> Type {
    let phpdoc = phpdoc.map(|p| {
        if ty.is_null().no() {
            p.remove_null()
        } else {
            p
        }
    });
    if matches!(ty, Type::Benevolent(_)) {
        return ty;
    }
    let Some(mut doc) = phpdoc.filter(|p| !matches!(p, Type::Error)) else {
        return ty;
    };
    if matches!(
        doc,
        Type::Never(NeverKind::Explicit | NeverKind::NonAccepting)
    ) {
        return doc;
    }
    if ty.is_implicit_mixed() && doc.is_void().yes() {
        return doc;
    }
    let mixed_key_array = |t: &Type| match t {
        Type::Array(key, item) if key.describe(Level::TypeOnly) == "mixed" => {
            Some(Type::Iterable(key.clone(), item.clone()))
        }
        _ => None,
    };
    if matches!(ty.clone().remove_null(), Type::Iterable(..)) {
        if let Type::Union(members) = &doc {
            doc = Type::Union(
                members
                    .iter()
                    .map(|m| match mixed_key_array(m) {
                        Some(Type::Iterable(_, item)) => {
                            Type::Iterable(Box::new(m.iterable_key_type()), item)
                        }
                        _ => m.clone(),
                    })
                    .collect(),
            );
        } else if let Some(iterable) = mixed_key_array(&doc) {
            doc = iterable;
        }
    }
    let narrows = (ty.is_callable().yes() && doc.is_callable().yes())
        || ((!matches!(doc, Type::Never(_)) || ty.is_implicit_mixed())
            && is_super_type_of(&ty, &doc.resolve_to_bounds()).yes());
    let result = if narrows { doc } else { ty.clone() };
    match &ty {
        Type::Union(members) => {
            let add: Vec<Type> = members
                .iter()
                .filter(|m| is_super_type_of(m, &result).no())
                .cloned()
                .collect();
            if add.is_empty() {
                result
            } else {
                union(std::iter::once(result).chain(add).collect())
            }
        }
        Type::Null => result.add_null(),
        _ => result,
    }
}
