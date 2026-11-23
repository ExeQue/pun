use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// A stub refines only PHPDoc types: the native types stay the vendor's. PHPStan 2.3.5's errors on calls.php at level
// 5, with vendor.php scanned and factory.stub as stubFiles
#[test]
fn a_stub_keeps_the_original_native_types() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/stub-native-types");
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
    let mut project = Project::new(vec![root.join("calls.php")], Vec::new());
    project.scanned.push(root.join("vendor.php"));
    project.stubs.push(root.join("factory.stub"));
    project.level = 5;
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
