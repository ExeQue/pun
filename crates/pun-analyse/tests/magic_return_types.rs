use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// PhpMethodFromParserNodeReflection replaces the declared return type of __serialize() and __sleep(), so their bare
// `array` has value types, also in a trait. PHPStan 2.3.5's errors on magic.php at level 6, in either order
#[test]
fn magic_methods_have_their_real_return_types() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/magic-return-types");
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
    let mut project = Project::new(vec![root.join("magic.php")], Vec::new());
    project.level = 6;
    let mut want = want;
    want.sort();
    let mut got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    got.sort();
    assert_eq!(got, want);
}
