use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

/// PhpClassReflectionExtension takes a trait's `@method` tag over a sibling trait's native method and over an
/// interface's method, as for Carbon's `toJSON()` (#19); a class without the tag keeps the native return type.
#[test]
fn trait_tags_win_over_sibling_traits_and_interfaces() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/trait-method-tags");
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
    assert_eq!(want.len(), 5);
    assert_eq!(got, want);
}
