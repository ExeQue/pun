use std::sync::Arc;

use mago_analyzer::plugin::context::{InvocationInfo, ProviderContext};
use mago_analyzer::plugin::provider::method::{MethodReturnTypeProvider, MethodTarget};
use mago_analyzer::plugin::provider::{Provider, ProviderMeta};
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::object::named::TNamedObject;
use mago_codex::ttype::expander::StaticClassType;
use mago_codex::ttype::union::TUnion;

use crate::larastan::app::App;
use crate::larastan::macros::Macros as Registered;

static META: ProviderMeta = ProviderMeta::new(
    "pun-larastan-macros",
    "pun-larastan-macros",
    "Larastan's macros",
);

pub(crate) struct Macros(pub(crate) Arc<App>, pub(crate) Arc<Registered>);

impl Provider for Macros {
    fn meta() -> &'static ProviderMeta {
        &META
    }
}

impl MethodReturnTypeProvider for Macros {
    fn targets() -> &'static [MethodTarget] {
        static TARGETS: [MethodTarget; 1] = [MethodTarget::any_class(b"*")];
        &TARGETS
    }

    fn get_return_type(
        &self,
        context: &ProviderContext<'_, '_, '_>,
        class: &[u8],
        method: &[u8],
        invocation: &InvocationInfo<'_, '_, '_>,
    ) -> Option<TUnion> {
        if self.1.is_empty() {
            return None;
        }
        let codebase = context.codebase();
        let target = invocation.inner().target.get_method_context();
        let receiver = target.map_or(class, |t| t.class_like_metadata.name.as_bytes());
        if codebase.method_exists(receiver, method) {
            return None;
        }
        let homes = self.1.homes(codebase, &self.0, receiver);
        let found = self.1.get(&homes, &String::from_utf8_lossy(method))?;
        let object = match target.map(|t| &t.class_type) {
            Some(StaticClassType::Object(TObject::Named(n))) => n.clone(),
            _ => TNamedObject::new(
                codebase
                    .get_class_like(receiver)
                    .map_or_else(|| mago_word::word(receiver), |k| k.original_name),
            ),
        };
        // EloquentBuilderForwardsCallsExtension: a method callable is reflected as the native method, which the
        // Eloquent builder forwards and returns itself from
        if found.native
            && codebase.is_instance_of(receiver, b"Illuminate\\Database\\Eloquent\\Builder")
        {
            return Some(TUnion::from_atomic(TAtomic::Object(TObject::Named(object))));
        }
        // the macro's closure is bound to the class it is called on, so its `static` is the receiver
        let types = found.ty.types.iter().map(|t| match t {
            TAtomic::Object(TObject::Named(n)) if n.is_static => {
                TAtomic::Object(TObject::Named(object.clone()))
            }
            t => t.clone(),
        });
        Some(TUnion::from_vec(types.collect()))
    }
}

pub(crate) fn macro_method(
    c: &super::super::Checker,
    class: &mago_codex::metadata::class_like::ClassLikeMetadata,
    method: &[u8],
) -> Option<crate::larastan::macros::Macro> {
    let larastan = c.larastan?;
    if larastan.macros.is_empty() {
        return None;
    }
    let lower = mago_word::word(method.to_ascii_lowercase());
    if let Some(declaring) = c
        .codebase
        .get_declaring_method_class(class.name.as_bytes(), method)
        && let Some(k) = c.codebase.get_class_like(declaring.as_bytes())
        && !k.pseudo_methods.contains(&lower)
        && !k.static_pseudo_methods.contains(&lower)
    {
        return None;
    }
    let homes = larastan
        .macros
        .homes(c.codebase, &larastan.app, class.name.as_bytes());
    larastan
        .macros
        .get(&homes, &String::from_utf8_lossy(method))
        .cloned()
}
