use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// symfony/config's TreeBuilder::getRootNode() returns an eight-level conditional type. Each conditional used to resolve
// again on every type query, so analysing one call took minutes; PHPStan resolves each conditional once.
#[test]
fn nested_conditional_returns_match_phpstan() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/nested-conditional-returns");
    let oracle: Value = serde_json::from_str(include_str!(
        "fixtures/nested-conditional-returns/expected-phpstan.json"
    ))
    .unwrap();
    let mut project = Project::new(vec![root.join("roots.php")], Vec::new());
    project.level = 9;
    let mut got: Vec<_> = run(&project)
        .unwrap()
        .into_iter()
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    let mut want: Vec<_> = oracle
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["line"].as_i64().unwrap(),
                e["identifier"].as_str().unwrap().to_string(),
                e["message"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    got.sort();
    want.sort();
    assert_eq!(got, want);
}
