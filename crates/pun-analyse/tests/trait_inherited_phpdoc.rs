use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

/// PHPStan reflects a trait method per class that uses the trait, so the method inherits that class's interface and
/// parent PHPDoc, as spatie/laravel-data's `Data::collect()` takes the `BaseData` contract's conditional return type
/// (#17). Each user of the trait gets its own: a class implementing another interface or extending a documented parent
/// takes that one's types, a subclass of the user inherits them, and a class with nothing to inherit keeps the trait's.
#[test]
fn trait_methods_inherit_the_using_class_phpdoc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trait-inherited-phpdoc");
    let mut project = Project::new(vec![root.join("types.php")], Vec::new());
    project.level = 5;
    let got: Vec<_> = run(&project)
        .unwrap()
        .into_iter()
        .map(|e| (e.line, e.message))
        .collect();
    let oracle: Value =
        serde_json::from_slice(&std::fs::read(root.join("expected-phpstan.json")).unwrap())
            .unwrap();
    let want: Vec<_> = oracle["files"]["types.php"]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["line"].as_i64().unwrap(),
                e["message"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(want.len(), 11);
    assert_eq!(got, want);
}
