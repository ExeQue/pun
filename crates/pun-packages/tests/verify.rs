//! `composer install`'s platform check. Each fixture is a project Composer 2.11.2 refused to install on this PHP
//! 8.5.12 (`composer install --no-scripts`, exit 2), with its output in `composer-output*.txt` and the platform
//! probe of the same PHP in `probe.json`.

use pun_packages::lock::Lock;
use pun_packages::verify::verify;
use pun_packages::version::IgnorePlatform;
use serde_json::{Map, Value};
use std::path::Path;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/verify");

fn check(case: &str, ignore: &IgnorePlatform) -> Option<String> {
    let dir = Path::new(FIXTURES).join(case);
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();
    let manifest: Map<String, Value> = serde_json::from_str(&read("composer.json")).unwrap();
    let lock = Lock::from_composer_lock(&read("composer.lock")).unwrap();
    let probe: Value = serde_json::from_str(
        &std::fs::read_to_string(Path::new(FIXTURES).join("probe.json")).unwrap(),
    )
    .unwrap();
    verify(&manifest, &lock, &probe, true, ignore).unwrap()
}

/// Composer's report after its three heading lines, without the newline `writeError` ends it with (Pun's
/// `eprintln!` adds that one), and with the command it names in the extension hint as Pun's.
fn composer(case: &str, file: &str) -> String {
    let text = std::fs::read_to_string(Path::new(FIXTURES).join(case).join(file)).unwrap();
    let heading = "Your lock file does not contain a compatible set of packages. Please run composer update.\n";
    let report = &text[text.find(heading).unwrap() + heading.len()..];
    report
        .strip_suffix('\n')
        .unwrap()
        .replace("run Composer with", "run pun with")
}

#[test]
fn reports_what_composer_reports() {
    let none = IgnorePlatform::default();
    for case in ["A", "B", "C", "D", "E", "F", "G", "H", "I"] {
        assert_eq!(
            check(case, &none).as_deref(),
            Some(composer(case, "composer-output.txt").as_str()),
            "{case}"
        );
    }
}

#[test]
fn ignored_requirements_are_left_out() {
    let listed =
        |names: &[&str]| IgnorePlatform::Listed(names.iter().map(|n| (*n).to_string()).collect());
    assert_eq!(
        check("A", &listed(&["ext-n*"])).as_deref(),
        Some(composer("A", "composer-output-ignore-ext-n.txt").as_str())
    );
    // Composer installs these (exit 0): `--ignore-platform-reqs`, and `php+` lifting the upper bound only.
    assert_eq!(check("A", &IgnorePlatform::All), None);
    assert_eq!(check("D", &listed(&["php+"])), None);
    assert_eq!(
        check("B", &listed(&["ext-foo"])).as_deref(),
        Some(composer("B", "composer-output-ignore-ext-foo.txt").as_str())
    );
    // With both ignored Composer goes on to download the package.
    assert_eq!(check("B", &listed(&["ext-foo", "php"])), None);
}
