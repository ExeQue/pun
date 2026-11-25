use super::{Tri, Type};
use crate::scoped::{Family, Scoped};

/// What PHPStan's reflection answers for an instance property of an object type, which `ObjectShapeType` compares.
pub struct ObjectProperty {
    /// `hasInstanceProperty()`.
    pub has: Tri,
    /// `hasStaticProperty()->yes()`.
    pub has_static: bool,
    /// `getInstanceProperty()`, or `None` when the reflection has no property to give.
    pub found: Option<FoundProperty>,
}

/// An instance property's reflection.
pub struct FoundProperty {
    pub is_public: bool,
    pub is_static: bool,
    pub is_readable: bool,
    /// `getReadableType()`.
    pub readable_type: Type,
}

struct Answer;
impl Family for Answer {
    type Of<'a> = dyn Fn(&Type, &str) -> ObjectProperty + 'a;
}

thread_local! {
    static ANSWER: Scoped<Answer> = Scoped::new();
}

/// Runs `f` with an object type's properties answered by `answer`, which sees `@property` tags, native properties
/// and the properties class reflection extensions add.
pub fn with_object_properties<R>(
    answer: &dyn Fn(&Type, &str) -> ObjectProperty,
    f: impl FnOnce() -> R,
) -> R {
    ANSWER.with(|a| a.set(answer, f))
}

/// The property `name` of `t`, or `None` when no answer is installed.
pub(super) fn object_property(t: &Type, name: &str) -> Option<ObjectProperty> {
    ANSWER.with(|a| a.get(|answer| answer(t, name)))
}

/// ObjectShapeType's `accepts()` (with `compare` as `accepts`) and `isSuperTypeOf()` (with `is_super_type_of`) for a
/// class type `b`: every property the shape requires must be a public, readable instance property of `b` whose type
/// the shape's property type takes. `None` when no answer is installed.
pub(super) fn shape_takes(
    props: &[(String, Type, bool)],
    b: &Type,
    accepting: bool,
    compare: &dyn Fn(&Type, &Type) -> Tri,
) -> Option<Tri> {
    let mut result = Tri::Yes;
    for (name, ty, optional) in props {
        let property = object_property(b, name)?;
        if accepting && !property.has.yes() && property.has_static {
            result = result.and(Tri::No);
            continue;
        }
        match property.has {
            Tri::No => {
                if !optional {
                    result = result.and(Tri::No);
                }
                continue;
            }
            Tri::Maybe if !optional => {
                result = result.and(Tri::Maybe);
                continue;
            }
            _ => {}
        }
        let Some(found) = property.found else {
            continue;
        };
        if !found.is_public || found.is_static || !found.is_readable {
            return Some(Tri::No);
        }
        let value = compare(ty, &found.readable_type);
        if value.no() {
            return Some(Tri::No);
        }
        result = result.and(value);
    }
    Some(result)
}
