//! `composer install`'s platform check. Each fixture is a project Composer 2.11.2 refused to install on this PHP
//! 8.5.12 (`composer install --no-scripts`, exit 2), with its output in `composer-output*.txt` and the platform
//! probe of the same PHP in `probe.json`. `R` and `S` ran on `php -n` with only
//! the extensions Composer needs (iconv, ctype, tokenizer, mbstring), which loads no ini file and no intl; their
//! probe, `probe-n.json`, is Pun's probe of that same command.

use pun_packages::lock::Lock;
use pun_packages::verify::{missing_requirements, verify};
use pun_packages::version::IgnorePlatform;
use serde_json::{Map, Value};
use std::path::Path;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/verify");

fn check(case: &str, ignore: &IgnorePlatform) -> Option<String> {
    check_on(case, "probe.json", ignore)
}

fn check_on(case: &str, probe: &str, ignore: &IgnorePlatform) -> Option<String> {
    let dir = Path::new(FIXTURES).join(case);
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();
    let manifest: Map<String, Value> = serde_json::from_str(&read("composer.json")).unwrap();
    let lock = Lock::from_composer_lock(&read("composer.lock")).unwrap();
    let probe: Value =
        serde_json::from_str(&std::fs::read_to_string(Path::new(FIXTURES).join(probe)).unwrap())
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
    // J: the root provides a platform package. K: an alias of a package the lock doesn't have. L: an upper bound
    // that is infinite. M: a platform requirement only the lock has. N: two packages that conflict both ways (one
    // problem, the target named by its version). O: a `false` override of php-64bit, which Composer ignores. P: a
    // php-* package this PHP doesn't have. Q: an alias that needs its missing requirement too. W to AC: after the
    // assertions' problems, the solver reports only the first conflict propagation finds: W and X, a locked package
    // whose requirement only a failed package meets (its requirement before and after the other's); Y, the later of
    // two such packages; Z, that one before a conflict; AA, one of two conflicts; AB, one of two failed requirements;
    // AC, one level of a chain.
    let cases = [
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "W",
        "X", "Y", "Z", "AA", "AB", "AC",
    ];
    for case in cases {
        assert_eq!(
            check(case, &none).as_deref(),
            Some(composer(case, "composer-output.txt").as_str()),
            "{case}"
        );
    }
    // R: no ini file loaded, so no extension hint but its empty line. S: lib-icu without intl.
    for case in ["R", "S"] {
        assert_eq!(
            check_on(case, "probe-n.json", &none).as_deref(),
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
    // `php+` with no upper bound to lift leaves the requirement as it is.
    assert_eq!(
        check("L", &listed(&["php+"])).as_deref(),
        Some(composer("L", "composer-output-ignore-php+.txt").as_str())
    );
    assert_eq!(
        check("B", &listed(&["ext-foo"])).as_deref(),
        Some(composer("B", "composer-output-ignore-ext-foo.txt").as_str())
    );
    // With both ignored Composer goes on to download the package.
    assert_eq!(check("B", &listed(&["ext-foo", "php"])), None);
}

/// `composer install`'s check that the lock meets composer.json: fixture `T` (exit 4, with and without
/// `--no-dev`) has a branch alias, a missing package, a replaced and a provided one, and requirements that are met
/// (by a locked version, by the root's `provide`, by `self.version`, and in `require-dev` by a dev package `require`
/// can't see). `U` sets `allow-missing-requirements`: Composer prints the same lines and installs (exit 0); without
/// it, exit 4. Composer prints the lines after its stale-lock warning.
#[test]
fn reports_missing_requirements_as_composer() {
    let lines = |case: &str, dev: bool| {
        let dir = Path::new(FIXTURES).join(case);
        let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();
        let manifest: Map<String, Value> = serde_json::from_str(&read("composer.json")).unwrap();
        let lock = Lock::from_composer_lock(&read("composer.lock")).unwrap();
        missing_requirements(&manifest, &lock, dev).unwrap()
    };
    let composer = |case: &str, file: &str| -> Vec<String> {
        let text = std::fs::read_to_string(Path::new(FIXTURES).join(case).join(file)).unwrap();
        (text.lines())
            .skip_while(|l| !l.starts_with("Warning: The lock file is not up to date"))
            .skip(1)
            .take_while(|l| !l.starts_with("Package operations:"))
            .map(String::from)
            .collect()
    };
    assert_eq!(lines("T", true), composer("T", "composer-output.txt"));
    assert_eq!(
        lines("T", false),
        composer("T", "composer-output-no-dev.txt")
    );
    assert_eq!(lines("U", true), composer("U", "composer-output.txt"));
    assert_eq!(
        lines("U", true),
        composer("U", "composer-output-strict.txt")
    );
    // V: met only through the branch alias, a replacer and a provider; Composer printed no line and went on.
    assert_eq!(lines("V", true), composer("V", "composer-output.txt"));
    assert_eq!(lines("V", true), Vec::<String>::new());
    assert_eq!(lines("A", true), Vec::<String>::new());
}
