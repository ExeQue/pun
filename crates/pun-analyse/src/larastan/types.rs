//! Larastan's `GenericEloquentBuilderTypeNodeResolverExtension`, applied to the scanned PHPDoc types before the
//! codebase is populated: the IDE-helper union `Builder|User` is `Builder<User>`.

use mago_codex::metadata::CodebaseMetadata;
use mago_codex::metadata::ttype::TypeMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::reference::TReference;
use mago_codex::ttype::union::TUnion;
use mago_word::Word;

const MODEL: &[u8] = b"Illuminate\\Database\\Eloquent\\Model";
const BUILDER: &[u8] = b"Illuminate\\Database\\Eloquent\\Builder";

/// Rewrites every PHPDoc return, parameter and property type that is a model and a builder class.
pub(crate) fn generic_eloquent_builders(codebase: &mut CodebaseMetadata) {
    let mut functions = Vec::new();
    for (key, f) in codebase.function_likes.iter() {
        let ret = (f.return_type_metadata.as_ref()).and_then(|t| resolve(codebase, t));
        let params: Vec<_> = (f.parameters.iter().enumerate())
            .filter_map(|(i, p)| Some((i, resolve(codebase, p.type_metadata.as_ref()?)?)))
            .collect();
        if ret.is_some() || !params.is_empty() {
            functions.push((*key, ret, params));
        }
    }
    let mut properties = Vec::new();
    for (key, k) in &codebase.class_likes {
        for (magic, map) in [(false, &k.properties), (true, &k.magic_properties)] {
            for (name, p) in map {
                let read = p.type_metadata.as_ref().and_then(|t| resolve(codebase, t));
                let write = (p.write_type_metadata.as_ref()).and_then(|t| resolve(codebase, t));
                if read.is_some() || write.is_some() {
                    properties.push((*key, magic, *name, read, write));
                }
            }
        }
    }
    for (key, ret, params) in functions {
        let f = codebase
            .function_likes
            .get_mut(&key)
            .expect("collected above");
        if let Some((t, ret)) = f.return_type_metadata.as_mut().zip(ret) {
            t.type_union = ret;
        }
        for (i, union) in params {
            if let Some(t) = f.parameters[i].type_metadata.as_mut() {
                t.type_union = union;
            }
        }
    }
    for (key, magic, name, read, write) in properties {
        let k = codebase.class_likes.get_mut(&key).expect("collected above");
        let map = if magic {
            &mut k.magic_properties
        } else {
            &mut k.properties
        };
        let p = map.get_mut(&name).expect("collected above");
        for (t, union) in [
            (&mut p.type_metadata, read),
            (&mut p.write_type_metadata, write),
        ] {
            if let Some((t, union)) = t.as_mut().zip(union) {
                t.type_union = union;
            }
        }
    }
}

/// `GenericEloquentBuilderTypeNodeResolverExtension::resolve()`: a PHPDoc union of two plain class names, one a
/// model and one an Eloquent builder, is the builder of that model (or the builder alone when it has no template).
/// Before population a class name is still a symbol reference.
fn resolve(codebase: &CodebaseMetadata, t: &TypeMetadata) -> Option<TUnion> {
    if !t.from_docblock {
        return None;
    }
    let [a, b] = t.type_union.types.as_ref() else {
        return None;
    };
    let (mut model, mut builder) = (None, None);
    for atomic in [a, b] {
        let TAtomic::Reference(TReference::Symbol {
            name,
            parameters: None,
            intersection_types: None,
            ..
        }) = atomic
        else {
            return None;
        };
        if is_a(codebase, *name, MODEL) {
            model = model.or(Some(*name));
        } else if is_a(codebase, *name, BUILDER) {
            builder = builder.or(Some(*name));
        }
    }
    let (model, builder) = (model?, builder?);
    let generic = !(codebase.get_class_like(builder.as_bytes()))?
        .template_types
        .is_empty();
    let symbol = |name, parameters| {
        TAtomic::Reference(TReference::Symbol {
            name,
            parameters,
            variances: None,
            intersection_types: None,
        })
    };
    let parameters = generic.then(|| vec![TUnion::from_atomic(symbol(model, None))]);
    Some(TUnion::from_atomic(symbol(builder, parameters)))
}

/// `ClassReflection::is()` on the scanned, not yet populated, classes: the class or one of its parents.
fn is_a(codebase: &CodebaseMetadata, name: Word, target: &[u8]) -> bool {
    let mut current = codebase.get_class_like(name.as_bytes());
    for _ in 0..64 {
        let Some(k) = current else {
            return false;
        };
        if k.name.as_bytes().eq_ignore_ascii_case(target) {
            return true;
        }
        current = (k.direct_parent_class).and_then(|p| codebase.get_class_like(p.as_bytes()));
    }
    false
}
