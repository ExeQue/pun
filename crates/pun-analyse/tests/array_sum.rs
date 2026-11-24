use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

/// PHPStan's ArraySumFunctionDynamicReturnTypeExtension: `null` and `bool` values add 0 or 1, so `list<int|null>`
/// sums to `int` (#30), constant arrays sum to their constant total, and a possibly empty array adds 0.
#[test]
fn nullable_bool_constant_and_empty_values() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/array-sum");
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
    assert_eq!(want.len(), 22);
    assert_eq!(got, want);
}
