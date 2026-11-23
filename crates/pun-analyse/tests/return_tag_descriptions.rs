use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// `@return <type> <description>` declares the return type: PHPStan 2.3.5's errors on descriptions.php at level 6
#[test]
fn a_return_tag_with_a_description_declares_the_return_type() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/return-tag-descriptions");
    let oracle: Value =
        serde_json::from_slice(&std::fs::read(root.join("expected-phpstan.json")).unwrap())
            .unwrap();
    let want: Vec<_> = (oracle["errors"].as_array().unwrap().iter())
        .map(|e| {
            (
                e["line"].as_i64().unwrap(),
                e["identifier"].as_str().unwrap().to_owned(),
                e["message"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    let mut project = Project::new(vec![root.join("descriptions.php")], Vec::new());
    project.level = 6;
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
