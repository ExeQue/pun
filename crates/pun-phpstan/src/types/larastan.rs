use super::{Accessory, StrictMixed, Tri, Type, accepts_with, is_super_type_of};
use crate::scoped::{Family, Scoped};

pub enum StringQuery<'a> {
    Property(&'a Type, &'a str),
    Column(&'a str, &'a str),
    View(&'a str),
}

struct Answer;
impl Family for Answer {
    type Of<'a> = dyn Fn(StringQuery<'_>) -> Tri + 'a;
}

thread_local! {
    static ANSWER: Scoped<Answer> = const { Scoped::new() };
}

pub fn with_larastan_strings<R>(
    answer: &dyn Fn(StringQuery<'_>) -> Tri,
    f: impl FnOnce() -> R,
) -> R {
    ANSWER.with(|a| a.set(answer, f))
}

fn ask(query: StringQuery<'_>) -> Tri {
    ANSWER
        .with(|a| a.get(|answer| answer(query)))
        .unwrap_or(Tri::Yes)
}

fn model_property(t: &Type) -> Option<&Type> {
    match t {
        Type::Accessory(Accessory::ModelProperty(m)) => Some(m),
        Type::Intersection(ts) => ts.iter().find_map(model_property),
        _ => None,
    }
}

fn is_view_string(t: &Type) -> bool {
    match t {
        Type::Accessory(Accessory::ViewString) => true,
        Type::Intersection(ts) => ts.iter().any(is_view_string),
        _ => false,
    }
}

fn single_constant(t: &Type) -> Option<String> {
    match t.constant_strings().as_slice() {
        [one] => Some(String::from_utf8_lossy(one).into_owned()),
        _ => None,
    }
}

pub(super) fn model_property_accepts(
    t: &Type,
    b: &Type,
    strict: bool,
    compound: impl FnOnce() -> Tri,
) -> Tri {
    if let Some(given) = model_property(b) {
        return accepts_with(t, given, strict, StrictMixed::default());
    }
    if b.is_compound() {
        return compound();
    }
    if let Some(given) = single_constant(b) {
        if matches!(t, Type::Mixed { .. }) || t.object_class_names().is_empty() {
            return Tri::Yes;
        }
        let given = given.split(" as ").next().unwrap_or_default();
        if given.contains('.') {
            return match given.split('.').collect::<Vec<_>>().as_slice() {
                [table, column] => ask(StringQuery::Column(table, column)),
                _ => Tri::Yes,
            };
        }
        return ask(StringQuery::Property(t, given));
    }
    if b.is_string().yes() {
        Tri::Yes
    } else {
        Tri::No
    }
}

pub(super) fn model_property_super(t: &Type, b: &Type) -> Tri {
    if let Some(given) = single_constant(b) {
        return Tri::from(ask(StringQuery::Property(t, &given)).yes());
    }
    match b {
        Type::Accessory(Accessory::ModelProperty(given)) => is_super_type_of(t, given),
        _ => Tri::No,
    }
}

pub(super) fn view_string_accepts(b: &Type, compound: impl FnOnce() -> Tri) -> Tri {
    if b.is_compound() && !is_view_string(b) {
        return compound();
    }
    view_string_super(b)
}

pub(super) fn view_string_super(b: &Type) -> Tri {
    if let Some(given) = single_constant(b) {
        return ask(StringQuery::View(&given));
    }
    if is_view_string(b) {
        Tri::Yes
    } else if b.is_string().yes() {
        Tri::Maybe
    } else {
        Tri::No
    }
}
