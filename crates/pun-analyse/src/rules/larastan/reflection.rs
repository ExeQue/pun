//! Larastan's class reflection extensions: Eloquent models forward to their builders, builders to the query
//! builder, relations to their related model's builder, and macros, scopes and magic methods exist.

use mago_codex::metadata::class_like::ClassLikeMetadata;
use pun_phpstan::types::Type;

use super::super::class_reflection::{ClassReflectionExtension, MethodAnswer, MethodPhase};
use super::super::reflection::Lookup;
use super::super::{Checker, declared, members};
use super::Larastan;

impl ClassReflectionExtension for Larastan {
    fn method(
        &self,
        c: &Checker,
        phase: MethodPhase,
        receiver: &Type,
        k: &ClassLikeMetadata,
        name: &[u8],
    ) -> Option<MethodAnswer> {
        if super::is_model(c, k) {
            return Some(MethodAnswer::Forward(model_builder(c, k)));
        }
        let original = declared::original_name(k);
        if phase == MethodPhase::BeforeMixins {
            return None;
        }
        if !super::is_a(
            c,
            k.name.as_bytes(),
            b"illuminate\\database\\eloquent\\builder",
        ) && super::macro_method(c, k, name).is_some()
        {
            return Some(MethodAnswer::Exists);
        }
        let args = match receiver {
            Type::Object(_, args, _) | Type::Static(_, args) => args.clone(),
            _ => Vec::new(),
        };
        let ancestor_args = |ancestor: &str| -> Vec<Type> {
            if original.eq_ignore_ascii_case(ancestor) {
                return args.clone();
            }
            declared::with_classes(c, || {
                pun_phpstan::symbols::lookup(&original)
                    .and_then(|k| pun_phpstan::types::ancestor_args(&k, ancestor, &args))
                    .unwrap_or_default()
            })
        };
        if super::is_a(
            c,
            k.name.as_bytes(),
            b"illuminate\\database\\eloquent\\builder",
        ) {
            if super::macro_method(c, k, name).is_some() {
                return Some(MethodAnswer::Exists);
            }
            let model = ancestor_args("Illuminate\\Database\\Eloquent\\Builder")
                .into_iter()
                .next()
                .map_or_else(
                    || {
                        Type::Object(
                            "Illuminate\\Database\\Eloquent\\Model".into(),
                            vec![],
                            false,
                        )
                    },
                    |m| m.resolve_to_bounds(),
                );
            for class in model.object_class_names() {
                if super::has_scope(c, &class, name) {
                    return Some(MethodAnswer::Exists);
                }
            }
            let query = "Illuminate\\Database\\Query\\Builder";
            if let Lookup::Found(q) = c.class_like(query) {
                if members::method(c, q, name).is_some() {
                    return Some(MethodAnswer::Forward(Type::Object(
                        query.into(),
                        vec![],
                        false,
                    )));
                }
                if super::macro_method(c, q, name).is_some() {
                    return Some(MethodAnswer::Exists);
                }
            }
            if let Some(finder) = name.strip_prefix(b"where") {
                let finder = String::from_utf8_lossy(finder);
                let segments = super::where_segments(&finder);
                if !self
                    .switches
                    .contains(crate::LarastanSwitches::CHECK_MODEL_PROPERTIES)
                    || segments.iter().all(|s| {
                        let property = format!("${}", super::snake(s));
                        c.has_property(&model, property.as_bytes(), false).yes()
                    })
                {
                    return Some(MethodAnswer::Exists);
                }
            }
            let soft_deletes = model.object_class_names().iter().any(|m| {
                c.codebase
                    .class_uses_trait(m.as_bytes(), b"Illuminate\\Database\\Eloquent\\SoftDeletes")
            });
            if soft_deletes
                && [
                    &b"withTrashed"[..],
                    b"onlyTrashed",
                    b"withoutTrashed",
                    b"restore",
                    b"createOrRestore",
                    b"restoreOrCreate",
                ]
                .contains(&name)
            {
                return Some(MethodAnswer::Exists);
            }
            return None;
        }
        if super::is_a(
            c,
            k.name.as_bytes(),
            b"illuminate\\database\\eloquent\\factories\\factory",
        ) && super::factory_magic_method(
            c.codebase,
            &mago_codex::ttype::atomic::object::named::TNamedObject::new(k.name),
            &String::from_utf8_lossy(name),
        ) {
            return Some(MethodAnswer::Exists);
        }
        if super::with_method_class(c.codebase, k.name.as_bytes(), name).is_some() {
            return Some(MethodAnswer::Exists);
        }
        if original.eq_ignore_ascii_case("Illuminate\\Support\\HigherOrderCollectionProxy") {
            let [proxied, value, _] = args.as_slice() else {
                return None;
            };
            if proxied.constant_strings().len() != 1
                || !c.can_call_methods(value).yes()
                || !c.has_method(value, name).yes()
            {
                return None;
            }
            return Some(MethodAnswer::Forward(value.clone()));
        }
        if original.eq_ignore_ascii_case("Illuminate\\Support\\HigherOrderTapProxy") {
            let [target] = args.as_slice() else {
                return None;
            };
            if target.object_class_names().is_empty() || !c.has_method(target, name).yes() {
                return None;
            }
            return Some(MethodAnswer::Forward(target.clone()));
        }
        if super::is_a(
            c,
            k.name.as_bytes(),
            b"illuminate\\database\\eloquent\\relations\\relation",
        ) {
            let related = ancestor_args("Illuminate\\Database\\Eloquent\\Relations\\Relation")
                .into_iter()
                .next()
                .filter(|r| !r.object_class_names().is_empty())
                .unwrap_or_else(|| {
                    Type::Object(
                        "Illuminate\\Database\\Eloquent\\Model".into(),
                        vec![],
                        false,
                    )
                });
            if !related.object_class_names().first().is_some_and(|m| {
                super::is_a(c, m.as_bytes(), b"illuminate\\database\\eloquent\\model")
            }) {
                return None;
            }
            let builder = match related.object_class_names().as_slice() {
                [model] => String::from_utf8_lossy(
                    super::builder_name(c.codebase, model.as_bytes()).as_bytes(),
                )
                .into_owned(),
                _ => "Illuminate\\Database\\Eloquent\\Builder".into(),
            };
            let builder = Type::Object(builder, vec![related], false);
            return Some(MethodAnswer::Forward(builder));
        }
        if k.kind.is_interface()
            && original.starts_with("Illuminate\\Contracts")
            && let Some(concrete) = self.app.resolve(&original)
            && let Lookup::Found(class) = c.class_like(concrete)
        {
            let concrete = Type::Object(declared::original_name(class), vec![], false);
            return Some(MethodAnswer::Forward(concrete));
        }
        None
    }

    fn has_property(&self, c: &Checker, class: &ClassLikeMetadata, property: &[u8]) -> bool {
        super::has_model_property(c, class, property)
    }

    fn calls_inaccessible_method(&self, c: &Checker, receiver: &Type, method: &str) -> bool {
        ["increment", "decrement"]
            .iter()
            .any(|m| method.eq_ignore_ascii_case(m))
            && receiver
                .object_class_names()
                .iter()
                .all(|n| (c.class_like(n).found()).is_some_and(|k| super::is_model(c, k)))
    }

    fn static_forward(
        &self,
        c: &Checker,
        class: &ClassLikeMetadata,
        declaring: &str,
    ) -> Option<Type> {
        (super::is_model(c, class)
            && !(c.codebase).is_instance_of(class.name.as_bytes(), declaring.as_bytes()))
        .then(|| model_builder(c, class))
    }
}

/// The builder a model's calls forward to, typed with the model.
fn model_builder(c: &Checker, model: &ClassLikeMetadata) -> Type {
    let builder = super::builder_name(c.codebase, model.name.as_bytes());
    Type::Object(
        String::from_utf8_lossy(builder.as_bytes()).into_owned(),
        vec![Type::Object(declared::original_name(model), vec![], false)],
        false,
    )
}
