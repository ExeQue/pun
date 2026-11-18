//! `checks` keeps a rule merged from several Rector rules to the ones rector.php runs (`pun migrate` sets it when
//! rector.php leaves some of them out). tests/fixtures/<rule>-checks/input.php holds one case per check; every other
//! file there is what Rector 2.8.4 wrote for it with the rule's Rector rules in `withRules()` and the ones not named
//! in the file (`<check>+<check>.php`) in `withSkip()`: `.oracle/rector-bin`, and `.oracle/rector-ecosystem` for the
//! Pest and PHPUnit rules, with tests/phpunit-stubs autoloaded for the PHPUnit tests.

use std::path::{Path, PathBuf};

use pun_lint::rules::facts::with_sources;
use pun_lint::rules::*;
use pun_lint::{Linter, Rule};

fn phpunit_stubs(dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            phpunit_stubs(&path, out);
        } else {
            let code = std::fs::read_to_string(&path).unwrap();
            out.push((path.to_string_lossy().into_owned(), code));
        }
    }
}

fn check(name: &str, rules: fn() -> Vec<Box<dyn Rule>>) {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let dir = tests.join(format!("fixtures/{name}-checks"));
    let input = std::fs::read_to_string(dir.join("input.php")).unwrap();
    let mut stubs = Vec::new();
    phpunit_stubs(&tests.join("phpunit-stubs"), &mut stubs);
    let mut expected: Vec<PathBuf> = (std::fs::read_dir(&dir).unwrap())
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap() != "input.php")
        .collect();
    expected.sort();
    assert!(!expected.is_empty(), "{name}: no expected files");
    for path in expected {
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let checks: Vec<&str> = stem.split('+').collect();
        let mut rules = rules();
        for rule in &mut rules {
            rule.configure(&serde_json::json!({ "checks": checks }))
                .unwrap();
        }
        let linter = Linter::new(rules, pun_lint::PhpVersion::from_id(80400));
        let fixed =
            with_sources(stubs.clone(), || linter.fix(&dir.join("input.php"), &input)).unwrap();
        let want = std::fs::read_to_string(&path).unwrap();
        assert_eq!(fixed.source, want, "{name}: checks = {checks:?}");
    }
    for bad in [serde_json::json!(["all"]), serde_json::json!("ands")] {
        for mut rule in rules() {
            assert!(
                rule.configure(&serde_json::json!({ "checks": bad }))
                    .is_err(),
                "{name}: checks = {bad}"
            );
        }
    }
}

#[test]
fn no_negated_group() {
    check("no-negated-group", || {
        vec![
            Box::new(NoNegatedAnd::default()),
            Box::new(NoNegatedGroup::default()),
        ]
    });
}

#[test]
fn no_redundant_null_check() {
    check("no-redundant-null-check", || {
        vec![
            Box::new(NoRedundantNullCheck::default()),
            Box::new(NoRedundantNullableCheck::default()),
        ]
    });
}

#[test]
fn prefer_empty_array_compare() {
    check("prefer-empty-array-compare", || {
        vec![
            Box::new(PreferEmptyArrayCompare::default()),
            Box::new(PreferEmptyArrayIdentical::default()),
        ]
    });
}

#[test]
fn prefer_in_array() {
    check("prefer-in-array", || {
        vec![
            Box::new(PreferInArray::default()),
            Box::new(PreferInArrayOverForeach::default()),
            Box::new(PreferInArrayOverSearch::default()),
        ]
    });
}

#[test]
fn prefer_null_coalesce() {
    check("prefer-null-coalesce", || {
        vec![
            Box::new(PreferNullCoalesce::default()),
            Box::new(PreferCoalesceChain::default()),
            Box::new(PreferCoalesceOverForeach::default()),
        ]
    });
}

#[test]
fn no_always_true_instanceof() {
    check("no-always-true-instanceof", || {
        vec![
            Box::new(NoAlwaysTrueInstanceof::default()),
            Box::new(NoAlwaysTrueInstanceofAssert::default()),
        ]
    });
}

#[test]
fn no_empty_block() {
    check("no-empty-block", || {
        vec![
            Box::new(NoEmptyBlock::default()),
            Box::new(NoEmptyLoop::default()),
        ]
    });
}

#[test]
fn no_redundant_default_argument() {
    check("no-redundant-default-argument", || {
        vec![
            Box::new(NoNullDefaultArgument::default()),
            Box::new(NoNullDefaultNamedArgument::default()),
            Box::new(NoRedundantDefaultArgument::default()),
        ]
    });
}

#[test]
fn no_unused_assignment() {
    check("no-unused-assignment", || {
        vec![
            Box::new(NoOverwrittenAssignment::default()),
            Box::new(NoUnusedAssignment::default()),
        ]
    });
}

#[test]
fn no_unused_private_method_param() {
    check("no-unused-private-method-param", || {
        vec![
            Box::new(NoUnusedPrivateMethodParam::default()),
            Box::new(NoUnusedTestMethodParam::default()),
        ]
    });
}

#[test]
fn prefer_to_throw() {
    check("prefer-to-throw", || {
        vec![
            Box::new(NoExpectException::default()),
            Box::new(PreferToThrow::default()),
        ]
    });
}

#[test]
fn prefer_will_return_callback() {
    check("prefer-will-return-callback", || {
        vec![
            Box::new(PreferMergedWillReturnCallback::default()),
            Box::new(PreferWillReturnCallback::default()),
        ]
    });
}

/// `lint/style/prefer-class-constant` is also PHP-CS-Fixer's `class_keyword`, so pun-config hands it `checks` under its
/// values' upstream names, the Rector rules (`namespaced-only`, which `pun migrate` sets beside it, turns
/// `class_keyword`'s own check off). The expected files are what Rector 2.8.4 wrote with
/// `StringClassNameToClassConstantRector` (`string-class-name`) and `UseClassKeywordForClassNameResolutionRector`
/// (`class-name-resolution`) in `withRules()` and the ones not named in the file in `withSkip()`.
#[test]
fn prefer_class_constant() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/prefer-class-constant-checks");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let input = read("input.php");
    let rules = || -> Vec<Box<dyn Rule>> {
        vec![
            Box::new(PreferClassConstant::default()),
            Box::new(PreferClassKeyword::default()),
            Box::new(PreferClassNameConstant::default()),
        ]
    };
    for (checks, expected) in [
        (
            vec!["StringClassNameToClassConstant"],
            "string-class-name.php",
        ),
        (
            vec!["UseClassKeywordForClassNameResolution"],
            "class-name-resolution.php",
        ),
        (
            vec![
                "StringClassNameToClassConstant",
                "UseClassKeywordForClassNameResolution",
            ],
            "class-name-resolution+string-class-name.php",
        ),
    ] {
        let mut rules = rules();
        for rule in &mut rules {
            let options = serde_json::json!({ "namespaced-only": true, "checks": checks });
            rule.configure(&options).unwrap();
        }
        let linter = Linter::new(rules, pun_lint::PhpVersion::from_id(80400));
        let fixed = linter.fix(&dir.join("input.php"), &input).unwrap();
        assert_eq!(fixed.source, read(expected), "{checks:?}");
    }
    for mut rule in rules() {
        let options = serde_json::json!({ "checks": ["string-class-name"] });
        assert!(rule.configure(&options).is_err());
    }
}
