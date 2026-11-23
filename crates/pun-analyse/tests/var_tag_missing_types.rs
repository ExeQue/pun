use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// InvalidPhpDocVarTagTypeRule with checkMissingVarTagTypehint (level 6): an inline @var tag reports iterables without
// a value type and generic classes without types. PHPStan 2.3.5's errors on vars.php at level 6
#[test]
fn var_tags_need_value_types_and_generics_at_level_6() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/var-tag-missing-types");
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
    let mut project = Project::new(vec![root.join("vars.php")], Vec::new());
    project.level = 6;
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
