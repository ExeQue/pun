use std::path::Path;

use pun_analyse::{Project, ProjectOptions, run};
use serde_json::Value;

// TemplateArgumentObserver::observeLowerBound() reads an argument that is another site's unresolved template argument
// through its delegate, so a list read out of one mapWithKeys() result widens the keys of another. PHPStan 2.3.5's
// errors on keys.php at level max with only the unresolvedTemplateArguments toggle (phpstan.neon)
#[test]
fn a_marker_argument_bounds_the_receiver_template() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/template-argument-marker-bounds");
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
    let mut project = Project::new(vec![root.join("keys.php")], Vec::new());
    project.level = 10;
    project.options |= ProjectOptions::UNRESOLVED_TEMPLATE_ARGUMENTS;
    let got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| (e.line, e.identifier, e.message))
        .collect();
    assert_eq!(got, want);
}
