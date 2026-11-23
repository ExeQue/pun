use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// ConstantFunctionReturnTypeExtension: constant() with a literal name has the type of that global or class
// constant, so its cast is no cast.int. PHPStan 2.3.5's errors on constant.php at level 9
#[test]
fn constant_has_the_type_of_the_named_constant() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/constant-function");
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
    let mut project = Project::new(vec![root.join("constant.php")], Vec::new());
    project.level = 9;
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
