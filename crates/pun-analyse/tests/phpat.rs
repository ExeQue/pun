//! phpat/phpat 0.12.5, ported natively. tests/fixtures/phpat is a project whose arch/ classes are phpat tests that use
//! every selector and assertion; expected-phpstan.json is PHPStan 2.3.5's output on src/ at level 0 with
//! `includes: [vendor/phpat/phpat/extension.neon]`, the two test classes registered as services tagged `phpat.test`,
//! and Composer autoloading `App\` from src/ and `Arch\` from arch/ (only the `phpat.*` errors).

use std::path::Path;

use pun_analyse::{Project, run};
use pun_phpstan::{Array, Key, Value as Parameter};
use serde_json::Value;

type Found = Vec<(String, i64, String, String, Option<String>, bool)>;

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phpat")
}

fn project(tests: &[&str], options: &[(&str, bool)]) -> Project {
    let root = root();
    let mut phpat = Array::new();
    phpat.set(
        Key::from("tests"),
        Parameter::Array(Array::list(tests.iter().map(|t| Parameter::str(*t)))),
    );
    for (k, v) in options {
        phpat.set(Key::from(*k), Parameter::Bool(*v));
    }
    let mut parameters = Array::new();
    parameters.set(
        Key::from("fileExtensions"),
        Parameter::Array(Array::list([Parameter::str("php")])),
    );
    parameters.set(
        Key::from("paths"),
        Parameter::Array(Array::list([Parameter::str(
            root.join("src").to_string_lossy().into_owned(),
        )])),
    );
    parameters.set(
        Key::from("scanDirectories"),
        Parameter::Array(Array::list([Parameter::str(
            root.join("arch").to_string_lossy().into_owned(),
        )])),
    );
    parameters.set(Key::from("phpat"), Parameter::Array(phpat));
    Project::from_config(&root, &parameters)
}

fn analyse(mut project: Project) -> Found {
    project.load_phpat().unwrap();
    let prefix = format!("{}/", root().display());
    let mut got: Found = run(&project)
        .unwrap()
        .into_iter()
        .filter(|e| e.identifier.starts_with("phpat."))
        .map(|e| {
            let file = e.file.strip_prefix(&prefix).unwrap_or(&e.file).to_string();
            (
                file,
                e.line,
                e.identifier,
                e.message,
                e.tip,
                e.can_be_ignored,
            )
        })
        .collect();
    got.sort();
    got
}

#[test]
fn every_selector_and_assertion() {
    let oracle: Value =
        serde_json::from_str(include_str!("fixtures/phpat/expected-phpstan.json")).unwrap();
    let mut want: Found = (oracle.as_array().unwrap().iter())
        .map(|e| {
            (
                e["file"].as_str().unwrap().to_string(),
                e["line"].as_i64().unwrap(),
                e["identifier"].as_str().unwrap().to_string(),
                e["message"].as_str().unwrap().to_string(),
                e["tip"].as_str().map(str::to_string),
                e["ignorable"].as_bool().unwrap(),
            )
        })
        .collect();
    want.sort();
    let got = analyse(project(&["Arch\\LayerTest", "Arch\\DeclarationTest"], &[]));
    assert_eq!(got, want);
}

/// Loads a test class whose `test()` method has `body`, from a project of its own; the error when Pun can't read it.
fn load_error(body: &str) -> String {
    let dir = std::env::temp_dir().join(format!(
        "pun-phpat-{}-{}",
        std::process::id(),
        body.len() + body.bytes().map(usize::from).sum::<usize>()
    ));
    std::fs::create_dir_all(dir.join("arch")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let class = format!(
        "<?php\n\nnamespace Arch;\n\nuse PHPat\\Selector\\Selector;\nuse PHPat\\Test\\PHPat;\n\nfinal class T\n{{\n    public function test()\n    {{\n        {body}\n    }}\n}}\n"
    );
    std::fs::write(dir.join("arch/T.php"), class).unwrap();
    let mut parameters = Array::new();
    parameters.set(
        Key::from("scanDirectories"),
        Parameter::Array(Array::list([Parameter::str(
            dir.join("arch").to_string_lossy().into_owned(),
        )])),
    );
    let mut phpat = Array::new();
    phpat.set(
        Key::from("tests"),
        Parameter::Array(Array::list([Parameter::str("Arch\\T")])),
    );
    parameters.set(Key::from("phpat"), Parameter::Array(phpat));
    let mut project = Project::from_config(&dir, &parameters);
    project.scanned = vec![dir.join("arch/T.php")];
    let e = project.load_phpat().unwrap_err();
    std::fs::remove_dir_all(&dir).unwrap();
    e.replace(&format!("{}/", dir.display()), "")
}

/// A value only PHP can compute, a method phpat doesn't have, or a rule phpat's validator rejects stops the run with
/// an error that names the code; nothing is skipped.
#[test]
fn unreadable_tests_are_errors() {
    assert_eq!(
        load_error(
            "return PHPat::rule()->classes(Selector::inNamespace(getenv('NS')))->shouldBeFinal();"
        ),
        "arch/T.php:10: phpat test Arch\\T::test(): Pun reads phpat tests without running PHP and can't read `getenv('NS')` (line 12)"
    );
    assert_eq!(
        load_error("return PHPat::rule()->classes(Selector::all())->shouldBeFancy();"),
        "arch/T.php:10: phpat test Arch\\T::test(): Pun reads phpat tests without running PHP and can't read `PHPat::rule()->classes(Selector::all())->shouldBeFancy()` (line 12): phpat 0.12.5 has no shouldbefancy() at this point of the rule builder"
    );
    assert_eq!(
        load_error("return PHPat::rule()->classes(Selector::isFancy())->shouldBeFinal();"),
        "arch/T.php:10: phpat test Arch\\T::test(): Pun reads phpat tests without running PHP and can't read `Selector::isFancy()` (line 12): phpat 0.12.5 has no selector isfancy()"
    );
    assert_eq!(
        load_error(
            "return PHPat::rule()->classes(Selector::appliesAttribute('A', false, ['x' => 1]))->shouldBeFinal();"
        ),
        "arch/T.php:10: phpat test Arch\\T::test(): Pun reads phpat tests without running PHP and can't read `Selector::appliesAttribute('A', false, ['x' => 1])` (line 12): Pun does not support appliesAttribute()'s $arguments yet"
    );
    assert_eq!(
        load_error("foreach ([1] as $x) {}\n        return PHPat::rule();"),
        "arch/T.php:10: phpat test Arch\\T::test(): Pun reads phpat tests without running PHP and can't read `foreach ([1] as $x) {}` (line 12)"
    );
    // RuleValidator's messages
    assert_eq!(
        load_error("return PHPat::rule()->classes(Selector::all())->shouldNotDependOn();"),
        "The PHPat rule Arch\\T:test has no targets"
    );
    assert_eq!(
        load_error("return PHPat::rule()->classes()->shouldBeFinal();"),
        "The PHPat rule Arch\\T:test has no subjects"
    );
    assert_eq!(
        load_error("return PHPat::rule()->classes(Selector::all());"),
        "The PHPat rule Arch\\T:test has no assertion"
    );
}

#[test]
fn missing_test_class_is_an_error() {
    let mut project = project(&["Arch\\Missing"], &[]);
    assert_eq!(
        project.load_phpat().unwrap_err(),
        "phpat test class Arch\\Missing was not found: Pun looks for it in Composer's autoload (autoload-dev included) and in the analysed and scanned files"
    );
}

/// `show_rule_names` prefixes each message with the rule name, and `ignore_built_in_classes` drops PHP's own classes
/// from the names a rule checks: PHPStan 2.3.5 with phpat on arch/OptionsTest.php, `show_rule_names: true`, reports
/// the line below with `ignore_built_in_classes: false` and nothing with `true`. `Stringable` is not reported:
/// `RuntimeException` brings it, so it is no direct interface of `Failure`.
#[test]
fn options() {
    let run = |ignore: bool| {
        analyse(project(
            &["Arch\\OptionsTest"],
            &[
                ("show_rule_names", true),
                ("ignore_built_in_classes", ignore),
            ],
        ))
    };
    assert_eq!(
        run(false),
        [(
            "src/Domain/Failure.php".to_string(),
            5,
            "phpat.testFailureDependencies".to_string(),
            "testFailureDependencies: App\\Domain\\Failure should not depend on RuntimeException"
                .to_string(),
            None,
            true
        )]
    );
    assert_eq!(run(true), []);
}
