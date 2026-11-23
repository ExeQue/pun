use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// Writing an offset of an undefined variable creates an array: PHPStan 2.3.5's errors on offsets.php at level 9
#[test]
fn an_offset_write_on_an_undefined_variable_creates_an_array() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/undefined-offset-writes");
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
    let mut project = Project::new(vec![root.join("offsets.php")], Vec::new());
    project.level = 9;
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
