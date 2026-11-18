use mago_analyzer::plugin::context::{InvocationInfo, ProviderContext};
use mago_analyzer::plugin::provider::method::{MethodReturnTypeProvider, MethodTarget};
use mago_analyzer::plugin::provider::{Provider, ProviderMeta};
use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_codex::metadata::enum_case::EnumCaseMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::array::TArray;
use mago_codex::ttype::atomic::array::list::TList;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::object::r#enum::TEnum;
use mago_codex::ttype::get_null;
use mago_codex::ttype::union::TUnion;

static META: ProviderMeta = ProviderMeta::new(
    "pun-enum-methods",
    "pun",
    "PHPStan's UnitEnum::cases(), BackedEnum::from() and tryFrom() return types",
);

/// PHPStan's return types of an enum's methods. `cases()` is the list of the enum's cases in declaration order
/// (ClassReflection's EnumCasesMethodReflection). `from()` and `tryFrom()` of a backed enum called with constant scalars
/// return the cases with those values, and `tryFrom()` adds null for a value no case has
/// (BackedEnumFromMethodDynamicReturnTypeExtension).
pub(super) struct EnumMethods;

fn case_type(enum_: &ClassLikeMetadata, case: &EnumCaseMetadata) -> TAtomic {
    TAtomic::Object(TObject::Enum(TEnum {
        name: enum_.original_name,
        case: Some(case.name),
    }))
}

impl Provider for EnumMethods {
    fn meta() -> &'static ProviderMeta {
        &META
    }
}

/// A backing value of a case or a constant scalar argument: PHP's `===` tells an int from a string.
#[derive(PartialEq)]
enum Scalar<'a> {
    Int(i64),
    String(&'a [u8]),
    Other,
}

fn constant_scalar(atom: &TAtomic) -> Option<Scalar<'_>> {
    if let Some(i) = atom.get_literal_int_value() {
        return Some(Scalar::Int(i));
    }
    if let Some(s) = atom.get_literal_string_value() {
        return Some(Scalar::String(s));
    }
    (atom.is_null() || atom.is_true() || atom.is_false() || atom.is_literal_float())
        .then_some(Scalar::Other)
}

impl MethodReturnTypeProvider for EnumMethods {
    fn targets() -> &'static [MethodTarget] {
        static TARGETS: [MethodTarget; 3] = [
            MethodTarget::any_class(b"cases"),
            MethodTarget::any_class(b"from"),
            MethodTarget::any_class(b"tryfrom"),
        ];
        &TARGETS
    }

    fn get_return_type(
        &self,
        context: &ProviderContext<'_, '_, '_>,
        _: &[u8],
        method: &[u8],
        invocation: &InvocationInfo<'_, '_, '_>,
    ) -> Option<TUnion> {
        let enum_ = (invocation.inner().target.get_method_context()?).class_like_metadata;
        if !enum_.kind.is_enum() {
            return None;
        }
        if method.eq_ignore_ascii_case(b"cases") {
            let mut cases: Vec<&EnumCaseMetadata> = enum_.enum_cases.values().collect();
            cases.sort_by_key(|case| case.span.start.offset);
            let known = (cases.into_iter().enumerate())
                .map(|(i, case)| (i, (false, TUnion::from_atomic(case_type(enum_, case)))))
                .collect();
            return Some(TUnion::from_atomic(TAtomic::Array(TArray::List(
                TList::from_known_elements(known),
            ))));
        }
        enum_.enum_type.as_ref()?;
        let try_from = method.eq_ignore_ascii_case(b"tryfrom");
        let value = context.get_expression_type(invocation.get_argument(0, &[b"value"])?)?;
        if enum_.enum_cases.is_empty() {
            return try_from.then(get_null);
        }
        let values: Vec<Scalar> = value
            .types
            .iter()
            .map(constant_scalar)
            .collect::<Option<_>>()?;
        let mut cases: Vec<TAtomic> = Vec::new();
        let mut add_null = false;
        for value in values {
            let found = enum_.enum_cases.values().find(|case| {
                case.value_type
                    .as_ref()
                    .and_then(constant_scalar)
                    .is_some_and(|v| v != Scalar::Other && v == value)
            });
            let Some(case) = found else {
                add_null = true;
                continue;
            };
            // a case has one backing value and a union no repeated literal, so no case is found twice
            cases.push(case_type(enum_, case));
        }
        if cases.is_empty() {
            return try_from.then(get_null);
        }
        if add_null && try_from {
            cases.push(TAtomic::Null);
        }
        Some(TUnion::from_vec(cases))
    }
}
