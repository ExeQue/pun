use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

// TemplateIntegerType and TemplateConstantIntegerType inherit IntegerType::toInteger(), so `(int)` keeps a template
// bounded by an integer; any other template casts to its bound's integer. PHPStan 2.3.5's errors on cast.php at level
// max
#[test]
fn an_int_cast_keeps_an_integer_template() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/int-cast-templates");
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
    let mut project = Project::new(vec![root.join("cast.php")], Vec::new());
    project.level = 10;
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
