use std::collections::HashMap;
use std::sync::Arc;

use crate::common::fixture;
use pun_phpstan::resolve::{resolve_str, with_type_nodes};
use pun_phpstan::symbols::{self, Class, Template as ClassTemplate};
use pun_phpstan::types::{
    Level, Template, Type, infer_template_types, is_super_type_of, resolve_template_types,
};

#[test]
fn object_shapes_compare_property_by_property() {
    let rows = fixture("object-shape-super.json");
    let mut failures = Vec::new();
    for row in rows.as_array().unwrap() {
        let [a, b, want] = [0, 1, 2].map(|i| row[i].as_str().unwrap());
        let got = format!(
            "{:?}",
            is_super_type_of(&resolve_str(a, None), &resolve_str(b, None))
        );
        if got != want {
            failures.push(format!("{a} isSuperTypeOf {b}: want {want}, got {got}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn class(name: &str) -> Option<Arc<Class>> {
    let template = |name: &str| ClassTemplate {
        name: name.to_string(),
        bound: "mixed".to_string(),
        default: None,
        variance: "invariant",
    };
    let inherited = |name: &str| format!("__template<{name}, 'class PunProbe\\Base', mixed>");
    let class = |ancestors: &[&str], templates, generics| Class {
        name: name.to_string(),
        ancestors: ancestors
            .iter()
            .map(std::string::ToString::to_string)
            .collect(),
        templates,
        cases: Vec::new(),
        case_values: Vec::new(),
        is_interface: false,
        is_trait: false,
        is_enum: false,
        is_final: false,
        properties: Vec::new(),
        allows_dynamic_properties: false,
        constants: Vec::new(),
        methods: HashMap::new(),
        iterable: Some((inherited("TKey"), inherited("TValue"))),
        generics,
        property_docs: HashMap::new(),
    };
    match name {
        "PunProbe\\Base" => Some(Arc::new(class(
            &[],
            vec![template("TKey"), template("TValue")],
            HashMap::new(),
        ))),
        "PunProbe\\Kid" => Some(Arc::new(class(
            &["PunProbe\\Base"],
            vec![template("TModel")],
            HashMap::from([(
                "PunProbe\\Base".to_string(),
                vec![
                    "int".to_string(),
                    "__template<TModel, 'class PunProbe\\Kid', mixed>".to_string(),
                ],
            )]),
        ))),
        _ => None,
    }
}

#[test]
fn inherited_iterator_takes_the_ancestor_arguments() {
    symbols::with_classes(&class, || {
        let kid = resolve_str("PunProbe\\Kid<string>", None);
        assert_eq!(kid.iterable_key_type().describe(Level::Precise), "int");
        assert_eq!(kid.iterable_value_type().describe(Level::Precise), "string");
    });
}

/// Larastan's union resolvers read the type nodes: two plain names (`Builder|User`), or a plain name and an array of a
/// plain name in either order (`Collection|User[]`). Any other union never reaches them.
#[test]
fn extension_type_node_reads_two_member_unions() {
    let extension = |name: &str, args: &[Type]| {
        matches!(name, "|" | "|[]").then(|| Type::Object(name.into(), args.to_vec(), false))
    };
    let resolve = |input: &str| {
        with_type_nodes(&extension, || resolve_str(input, None)).describe(Level::Precise)
    };
    assert_eq!(resolve("Foo|Bar"), "|<Foo, Bar>");
    assert_eq!(resolve("Foo|Bar[]"), "|[]<Foo, Bar>");
    assert_eq!(resolve("Bar[]|Foo"), "|[]<Foo, Bar>");
    for other in [
        "Foo|Bar[][]",
        "Foo|array<Bar>",
        "Foo|Bar|Baz[]",
        "Foo[]|Bar[]",
    ] {
        assert_eq!(
            resolve(other),
            resolve_str(other, None).describe(Level::Precise),
            "{other}"
        );
    }
}

#[test]
fn extension_type_node_resolves_with_its_template() {
    let t = Type::Template(Box::new(Template {
        scope: "function probe".into(),
        name: "T".into(),
        bound: resolve_str("mixed", None),
        default: None,
        argument: false,
    }));
    let map = infer_template_types(&t, &resolve_str("int", None));
    let node = Type::Object("probe-of".into(), vec![t], false);
    let extension = |name: &str, args: &[Type]| match (name, args) {
        ("probe-of", [arg]) if !matches!(arg, Type::Template(_)) => {
            Some(Type::Object("Probe".into(), vec![arg.clone()], false))
        }
        _ => None,
    };
    let resolved = with_type_nodes(&extension, || resolve_template_types(&node, &map));
    assert_eq!(resolved.describe(Level::Precise), "Probe<int>");
    assert_eq!(
        resolve_template_types(&node, &map).describe(Level::Precise),
        "probe-of<int>"
    );
}
