//! nesbot/carbon 3.15.1's PHPStan extension. tests/fixtures/carbon is a project whose bootstrap file registers
//! macros (a closure, a static closure, a static method, a function); its expected-phpstan.json is PHPStan 2.3.5's
//! output on it at level 5 with `includes: [vendor/nesbot/carbon/extension.neon]`, and
//! expected-phpstan-without-extension.json the same run without the include. Pun knows Carbon through carbon.php.

use std::path::Path;

use pun_analyse::{Project, run};
use pun_phpstan::{Array, Key, Value as Parameter};
use serde_json::Value;

type Found = Vec<(String, i64, String, String)>;

fn analyse(extension: bool) -> Found {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/carbon");
    let path = |p: &str| Parameter::str(root.join(p).to_string_lossy().into_owned());
    let mut bootstrap = vec![path("bootstrap/macros.php")];
    if extension {
        bootstrap.push(path(
            "vendor/nesbot/carbon/lazy/Carbon/UnprotectedDatePeriod.php",
        ));
    }
    let mut parameters = Array::new();
    parameters.set(Key::from("level"), Parameter::Int(5));
    parameters.set(
        Key::from("fileExtensions"),
        Parameter::Array(Array::list([Parameter::str("php")])),
    );
    parameters.set(
        Key::from("paths"),
        Parameter::Array(Array::list([path("src")])),
    );
    parameters.set(
        Key::from("scanFiles"),
        Parameter::Array(Array::list([path("carbon.php")])),
    );
    parameters.set(
        Key::from("bootstrapFiles"),
        Parameter::Array(Array::list(bootstrap)),
    );
    let project = Project::from_config(&root, &parameters);
    let prefix = format!("{}/", root.display());
    let mut got: Found = run(&project)
        .unwrap()
        .into_iter()
        .map(|e| {
            let file = e.file.strip_prefix(&prefix).unwrap_or(&e.file).to_string();
            (file, e.line, e.identifier, e.message)
        })
        .collect();
    got.sort();
    got
}

fn oracle(json: &str) -> Found {
    let oracle: Value = serde_json::from_str(json).unwrap();
    let mut want: Found = (oracle.as_array().unwrap().iter())
        .map(|e| {
            (
                e["file"].as_str().unwrap().to_string(),
                e["line"].as_i64().unwrap(),
                e["identifier"].as_str().unwrap().to_string(),
                e["message"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    want.sort();
    want
}

#[test]
fn carbon_macros_and_date_period_base() {
    assert_eq!(
        analyse(true),
        oracle(include_str!("fixtures/carbon/expected-phpstan.json"))
    );
}

/// Without the extension the macros and `DatePeriodBase` are unknown: the errors that says, compared on their own
/// identifiers.
#[test]
fn without_carbon_extension() {
    let unknown = |found: Found| -> Found {
        (found.into_iter())
            .filter(|e| {
                ["method.notFound", "staticMethod.notFound", "class.notFound"].contains(&&*e.2)
            })
            .collect()
    };
    assert_eq!(
        unknown(analyse(false)),
        unknown(oracle(include_str!(
            "fixtures/carbon/expected-phpstan-without-extension.json"
        )))
    );
}
