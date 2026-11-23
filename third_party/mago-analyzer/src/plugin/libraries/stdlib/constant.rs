//! `constant()` return type provider.

use mago_codex::scanner::inference::get_literal_constant_type;
use mago_codex::scanner::inference::get_platform_constant_type;
use mago_codex::ttype::combine_union_types;
use mago_codex::ttype::combiner::CombinerOptions;
use mago_codex::ttype::expander;
use mago_codex::ttype::expander::TypeExpansionOptions;
use mago_codex::ttype::union::TUnion;

use crate::plugin::context::InvocationInfo;
use crate::plugin::context::ProviderContext;
use crate::plugin::provider::Provider;
use crate::plugin::provider::ProviderMeta;
use crate::plugin::provider::function::FunctionReturnTypeProvider;
use crate::plugin::provider::function::FunctionTarget;

static META: ProviderMeta =
    ProviderMeta::new("php::constant", "constant", "Returns the type of the constant a literal name refers to");

/// PHPStan's ConstantFunctionReturnTypeExtension: `constant('NAME')` and `constant('Class::NAME')` have the type of
/// the constant, read by its fully qualified name. An unknown name keeps the declared `mixed`.
#[derive(Default)]
pub struct ConstantProvider;

impl Provider for ConstantProvider {
    fn meta() -> &'static ProviderMeta {
        &META
    }
}

impl FunctionReturnTypeProvider for ConstantProvider {
    fn targets() -> FunctionTarget {
        FunctionTarget::Exact(b"constant")
    }

    fn get_return_type(
        &self,
        context: &ProviderContext<'_, '_, '_>,
        invocation: &InvocationInfo<'_, '_, '_>,
    ) -> Option<TUnion> {
        let names = context.get_expression_type(invocation.get_argument(0, &[b"name"])?)?;
        let codebase = context.codebase();
        let mut result: Option<TUnion> = None;
        for atomic in names.types.iter() {
            let name = atomic.get_literal_string_value()?;
            let name = name.strip_prefix(b"\\").unwrap_or(name);
            let mut ty = if let Some(at) = name.windows(2).position(|w| w == b"::") {
                let (class, constant) = (&name[..at], &name[at + 2..]);
                codebase.get_class_constant_type(class.strip_prefix(b"\\").unwrap_or(class), constant)?.into_owned()
            } else if let Some(meta) = codebase.get_constant(name) {
                match &meta.type_metadata {
                    Some(t) if !t.from_docblock => t.type_union.clone(),
                    _ if meta.flags.is_built_in() && get_platform_constant_type(name).is_some() => {
                        get_platform_constant_type(name)?
                    }
                    Some(t) => t.type_union.clone(),
                    None => meta.inferred_type.clone()?,
                }
            } else {
                get_literal_constant_type(name)?
            };
            expander::expand_union(codebase, &mut ty, &TypeExpansionOptions::default());
            result = Some(match result {
                Some(r) => combine_union_types(&r, &ty, codebase, CombinerOptions::default()),
                None => ty,
            });
        }

        result
    }
}
