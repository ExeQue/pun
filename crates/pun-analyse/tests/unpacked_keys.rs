use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// Unpacking an array whose keys may be strings keeps the keys, so the literal is not a list: PHPStan 2.3.5's
// errors on spread.php at level 5
#[test]
fn unpacking_keys_that_may_be_strings_makes_no_list() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unpacked-keys");
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
    let mut project = Project::new(vec![root.join("spread.php")], Vec::new());
    project.level = 5;
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
