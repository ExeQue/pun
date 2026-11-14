use std::path::Path;

use pun_analyse::{Project, run_mago};
use serde_json::Value;

#[test]
fn issues_match_upstream_oracles() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mago");
    let file = dir.join("issues.php");
    let analysed = vec![
        dir.join("debug-panic.php"),
        file.clone(),
        dir.join("missing.php"),
    ];
    let run = run_mago(&Project::new(analysed, Vec::new()), None).unwrap();
    let got: Vec<_> = run
        .issues
        .iter()
        .filter(|i| i.file != dir.join("debug-panic.php").display().to_string())
        .map(|i| (i.file.clone(), i.line, i.code.clone(), i.message.clone()))
        .collect();
    let expected: Value =
        serde_json::from_slice(&std::fs::read(dir.join("expected.json")).unwrap()).unwrap();
    let phpstan: Value =
        serde_json::from_slice(&std::fs::read(dir.join("expected-phpstan.json")).unwrap()).unwrap();
    let phpstan = phpstan.as_array().unwrap();
    let want: Vec<_> = expected
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| {
            e["code"] != "mixed-argument"
                || !phpstan
                    .iter()
                    .any(|p| p["line"] == e["line"] && p["identifier"] == "variable.undefined")
                || phpstan
                    .iter()
                    .any(|p| p["line"] == e["line"] && p["identifier"] == "echo.nonString")
        })
        .map(|e| {
            let line = e["line"].as_u64().unwrap() as u32;
            let (code, message) = (e["code"].as_str().unwrap(), e["message"].as_str().unwrap());
            (
                file.display().to_string(),
                line,
                code.to_string(),
                message.to_string(),
            )
        })
        .collect();
    assert_eq!(got, want);
}
