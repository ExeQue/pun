//! Rector's own fixtures for lint-a's rules (dead-code, complexity, control-flow, operators, strings, arrays,
//! statements, comments, tags and refactor groups), laid out as in tests/rector.rs: Fixture/ verbatim from rector-src
//! 2.8.4, Oracle/ with what Rector 2.8.4 (`.oracle/rector-bin`, the rule alone) wrote, Pun/ derived by hand where Pun
//! keeps the meaning but not Rector's bytes. Each must pass as `format(pun lint --fix) == format(expected)`.

use std::path::{Path, PathBuf};

use pun_format::Pipeline;
use pun_lint::rules;
use pun_lint::{Linter, Rule};

fn fixtures(dir: &str) -> Vec<(PathBuf, String, String)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rector")
        .join(dir);
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no fixtures in {}", dir.display());
    paths
        .into_iter()
        .map(|path| {
            let content = std::fs::read_to_string(&path)
                .unwrap()
                .replace("\r\n", "\n");
            let (input, expected) = match content.split_once("-----\n") {
                Some((i, e)) => (i.to_string(), e.to_string()),
                None => (content.clone(), content),
            };
            (path, input, expected)
        })
        .collect()
}

fn format(pipeline: &Pipeline, code: &str) -> String {
    match pipeline
        .run(Path::new("fixture.php"), code.as_bytes())
        .unwrap()
    {
        Some(fixed) => String::from_utf8(fixed.code).unwrap(),
        None => code.to_string(),
    }
}

fn laravel() -> Pipeline {
    let rules = pun_config::rule_set(pun_config::preset_rules("laravel").unwrap()).unwrap();
    let pipeline = Pipeline::new(&rules, &pun_fixers::all).unwrap();
    assert!(pipeline.not_ported.is_empty(), "{:?}", pipeline.not_ported);
    pipeline
}

fn run(linter: &Linter, dir: &str) {
    run_formatter_too(linter, dir, &[]);
}

fn run_formatter_too(linter: &Linter, dir: &str, formatter_too: &[&str]) {
    let pipeline = laravel();
    for (path, input, expected) in fixtures(dir) {
        let name = path.file_name().unwrap().to_string_lossy();
        let fixed = linter.fix(&path, &input).unwrap();
        assert_eq!(fixed.source, expected, "{name}: lint --fix");
        assert_eq!(
            format(&pipeline, &fixed.source),
            format(&pipeline, &expected),
            "{name}: format(lint --fix)"
        );
        if input != expected && !formatter_too.contains(&name.as_ref()) {
            assert_ne!(
                format(&pipeline, &input),
                format(&pipeline, &expected),
                "{name}"
            );
            assert!(!fixed.applied.is_empty(), "{name}");
        } else if input == expected {
            assert!(fixed.applied.is_empty(), "{name}");
        }
    }
}

fn run_with_whitespace_exceptions(linter: &Linter, dir: &str, whitespace_only: &[&str]) {
    let pipeline = laravel();
    let squeeze = |s: &str| s.split_whitespace().collect::<String>();
    for (path, input, expected) in fixtures(dir) {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let fixed = linter.fix(&path, &input).unwrap();
        if whitespace_only.contains(&name.as_str()) {
            assert_ne!(fixed.source, expected, "{name}: no longer an exception");
            assert_eq!(
                squeeze(&fixed.source),
                squeeze(&expected),
                "{name}: lint --fix"
            );
        } else {
            assert_eq!(fixed.source, expected, "{name}: lint --fix");
        }
        assert_eq!(
            format(&pipeline, &fixed.source),
            format(&pipeline, &expected),
            "{name}: format(lint --fix)"
        );
        assert_eq!(input != expected, !fixed.applied.is_empty(), "{name}");
    }
}

fn only(rule: impl pun_lint::Rule + 'static, php_version: u32) -> Linter {
    Linter::new(
        vec![Box::new(rule)],
        pun_lint::PhpVersion::from_id(php_version),
    )
}

#[test]
fn unwrap_future_compatible_if_php_version() {
    run(
        &only(rules::NoFuturePhpVersionCheck, 100000),
        "UnwrapFutureCompatibleIfPhpVersionRector/Fixture",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone, PHP 8.4).
    run(
        &only(rules::NoFuturePhpVersionCheck, 80400),
        "UnwrapFutureCompatibleIfPhpVersionRector/Oracle",
    );
    run(
        &only(rules::NoFuturePhpVersionCheck, 80400),
        "UnwrapFutureCompatibleIfPhpVersionRector/Pun",
    );
}

#[test]
fn remove_php_version_id_check() {
    run(
        &only(rules::NoDeadPhpVersionCheck, 80000),
        "RemovePhpVersionIdCheckRector/Fixture",
    );
    run(
        &only(rules::NoDeadPhpVersionCheck, 80400),
        "RemovePhpVersionIdCheckRector/Pun",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone, PHP 8.4).
    run(
        &only(rules::NoDeadPhpVersionCheck, 80400),
        "RemovePhpVersionIdCheckRector/Oracle",
    );
}

#[test]
fn remove_unused_public_method_parameter() {
    run(
        &only(rules::NoUnusedPublicMethodParam, 80400),
        "RemoveUnusedPublicMethodParameterRector/Fixture",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone, PHP 8.4).
    run(
        &only(rules::NoUnusedPublicMethodParam, 80400),
        "RemoveUnusedPublicMethodParameterRector/Oracle",
    );
}

#[test]
fn remove_unused_private_method_parameter() {
    run(
        &only(rules::NoUnusedPrivateMethodParam::default(), 80400),
        "RemoveUnusedPrivateMethodParameterRector/Fixture",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone, PHP 8.4).
    run(
        &only(rules::NoUnusedPrivateMethodParam::default(), 80400),
        "RemoveUnusedPrivateMethodParameterRector/Oracle",
    );
    run(
        &only(rules::NoUnusedPrivateMethodParam::default(), 80400),
        "RemoveUnusedPrivateMethodParameterRector/Pun",
    );
}

#[test]
fn remove_unused_private_property() {
    run_with_whitespace_exceptions(
        &only(rules::NoUnusedPrivateProperty, 80400),
        "RemoveUnusedPrivatePropertyRector/Fixture",
        &["on_arg.php.inc"],
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone, PHP 8.4).
    run(
        &only(rules::NoUnusedPrivateProperty, 80400),
        "RemoveUnusedPrivatePropertyRector/Oracle",
    );
    run(
        &only(rules::NoUnusedPrivateProperty, 80400),
        "RemoveUnusedPrivatePropertyRector/Pun",
    );
}

#[test]
fn remove_default_value_from_assigned_property() {
    run(
        &only(rules::NoRedundantPropertyDefault, 80400),
        "RemoveDefaultValueFromAssignedPropertyRector/Fixture",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone, PHP 8.4).
    run(
        &only(rules::NoRedundantPropertyDefault, 80400),
        "RemoveDefaultValueFromAssignedPropertyRector/Oracle",
    );
}

#[test]
fn inline_constructor_default_to_property() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rector/InlineConstructorDefaultToPropertyRector/Source");
    let sources = ["AbstractParentWithFilledProperty.php", "ValidatedBase.php"].map(|f| {
        let path = source.join(f);
        let text = std::fs::read_to_string(&path).unwrap();
        (path.display().to_string(), text)
    });
    rules::facts::with_sources(sources.to_vec(), || {
        run(
            &only(rules::PreferPropertyDefault, 80400),
            "InlineConstructorDefaultToPropertyRector/Fixture",
        );
    });
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone, PHP 8.4).
    run(
        &only(rules::PreferPropertyDefault, 80400),
        "InlineConstructorDefaultToPropertyRector/Oracle",
    );
}

#[test]
fn foreach_to_in_array() {
    run(
        &only(rules::PreferInArrayOverForeach::default(), 80400),
        "ForeachToInArrayRector/Fixture",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone, PHP 8.4).
    run(
        &only(rules::PreferInArrayOverForeach::default(), 80400),
        "ForeachToInArrayRector/Oracle",
    );
    run(
        &only(rules::PreferInArrayOverForeach::default(), 80400),
        "ForeachToInArrayRector/Pun",
    );
}

#[test]
fn consecutive_null_compare_returns_to_null_coalesce_queue() {
    run(
        &only(rules::PreferCoalesceChain::default(), 80400),
        "ConsecutiveNullCompareReturnsToNullCoalesceQueueRector/Fixture",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone, PHP 8.4).
    run(
        &only(rules::PreferCoalesceChain::default(), 80400),
        "ConsecutiveNullCompareReturnsToNullCoalesceQueueRector/Oracle",
    );
    run(
        &only(rules::PreferCoalesceChain::default(), 80400),
        "ConsecutiveNullCompareReturnsToNullCoalesceQueueRector/Pun",
    );
}

#[test]
fn simplify_foreach_to_coalescing() {
    run(
        &only(rules::PreferCoalesceOverForeach::default(), 80400),
        "SimplifyForeachToCoalescingRector/Fixture",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone, PHP 8.4).
    run(
        &only(rules::PreferCoalesceOverForeach::default(), 80400),
        "SimplifyForeachToCoalescingRector/Oracle",
    );
}

#[test]
fn remove_dead_condition_above_return() {
    let linter = only(rules::NoUselessCondition, 80400);
    run(&linter, "RemoveDeadConditionAboveReturnRector/Fixture");
    run(&linter, "RemoveDeadConditionAboveReturnRector/Oracle");
}

#[test]
fn remove_condition_exact_return() {
    let linter = only(rules::NoConditionExactReturn, 80400);
    run(&linter, "RemoveConditionExactReturnRector/Fixture");
    run(&linter, "RemoveConditionExactReturnRector/Oracle");
}

#[test]
fn remove_double_assign() {
    let linter = only(rules::NoOverwrittenAssignment::default(), 80400);
    run(&linter, "RemoveDoubleAssignRector/Fixture");
    run(&linter, "RemoveDoubleAssignRector/Oracle");
}

#[test]
fn remove_next_same_value_condition() {
    let linter = only(rules::NoRedundantCondition, 80400);
    run(&linter, "RemoveNextSameValueConditionRector/Fixture");
    run(&linter, "RemoveNextSameValueConditionRector/Oracle");
}

#[test]
fn remove_dead_loop() {
    let linter = only(rules::NoEmptyLoop::default(), 80400);
    run(&linter, "RemoveDeadLoopRector/Fixture");
    run_with_whitespace_exceptions(
        &linter,
        "RemoveDeadLoopRector/Oracle",
        &["dead_loop_braceless.php.inc"],
    );
}

#[test]
fn remove_dead_stmt() {
    let linter = only(rules::NoUnusedExpression, 80400);
    run_formatter_too(
        &linter,
        "RemoveDeadStmtRector/Fixture",
        &["constant_fetch.php.inc", "scalar.php.inc"],
    );
    run(&linter, "RemoveDeadStmtRector/FixtureRemovedComments");
    run(&linter, "RemoveDeadStmtRector/Oracle");
    run(&linter, "RemoveDeadStmtRector/Pun");
}

#[test]
fn remove_sole_value_sprintf() {
    let linter = only(rules::NoSoleValueSprintf, 80400);
    run(&linter, "RemoveSoleValueSprintfRector/Fixture");
    run(&linter, "RemoveSoleValueSprintfRector/Oracle");
}

#[test]
fn compact_to_variables() {
    let linter = only(rules::NoCompact, 80400);
    run(&linter, "CompactToVariablesRector/Fixture");
    run(&linter, "CompactToVariablesRector/Oracle");
}

#[test]
fn is_a_with_string_with_third_argument() {
    run(
        &only(rules::RequireIsAAllowString, 80400),
        "IsAWithStringWithThirdArgumentRector/Fixture",
    );
    // Pun's own cases; the expected code is the oracle's (.oracle/rector-bin, this rule alone, PHP 8.4).
    run(
        &only(rules::RequireIsAAllowString, 80400),
        "IsAWithStringWithThirdArgumentRector/Oracle",
    );
}

#[test]
fn unnecessary_ternary_expression() {
    let linter = only(rules::NoBoolTernary::default(), 80400);
    run(&linter, "UnnecessaryTernaryExpressionRector/Fixture");
    run(&linter, "UnnecessaryTernaryExpressionRector/Oracle");
    run(&linter, "UnnecessaryTernaryExpressionRector/Pun");
}

#[test]
fn remove_useless_ternary() {
    let linter = only(rules::NoUselessTernary::default(), 80400);
    run(&linter, "RemoveUselessTernaryRector/Fixture");
    run(&linter, "RemoveUselessTernaryRector/Oracle");
}

/// `checks` keeps the rule to the Rector rules rector.php runs. The expectations are what Rector 2.8.5 wrote with
/// `withPreparedSets(deadCode: true)` (`RemoveUselessTernaryRector`, which also drops `? true : false`) and with
/// `SimplifyTautologyTernaryRector` and `UnnecessaryTernaryExpressionRector` alone.
#[test]
fn no_useless_ternary_checks() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/no-useless-ternary-checks");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let input = read("input.php");
    for (checks, expected) in [
        (serde_json::json!(["useless"]), "useless.php"),
        (
            serde_json::json!(["bool", "tautology"]),
            "bool-and-tautology.php",
        ),
    ] {
        let mut rules: Vec<Box<dyn Rule>> = vec![
            Box::new(rules::NoBoolTernary::default()),
            Box::new(rules::NoTautologyTernary::default()),
            Box::new(rules::NoUselessTernary::default()),
        ];
        for rule in &mut rules {
            rule.configure(&serde_json::json!({ "checks": checks }))
                .unwrap();
        }
        let linter = Linter::new(rules, pun_lint::PhpVersion::from_id(80400));
        let fixed = linter.fix(Path::new("Ternaries.php"), &input).unwrap();
        assert_eq!(fixed.source, read(expected), "{checks}");
    }
    let mut rule = rules::NoUselessTernary::default();
    assert!(
        rule.configure(&serde_json::json!({ "checks": ["all"] }))
            .is_err()
    );
}

#[test]
fn ternary_to_boolean_or_false_to_boolean_and() {
    let linter = only(rules::NoFalseTernary, 80400);
    run(&linter, "TernaryToBooleanOrFalseToBooleanAndRector/Fixture");
    run(&linter, "TernaryToBooleanOrFalseToBooleanAndRector/Oracle");
    run(&linter, "TernaryToBooleanOrFalseToBooleanAndRector/Pun");
}

#[test]
fn simplify_bool_identical_true() {
    let linter = only(rules::NoBooleanLiteralCompare::default(), 80400);
    run(&linter, "SimplifyBoolIdenticalTrueRector/Fixture");
    run(&linter, "SimplifyBoolIdenticalTrueRector/Oracle");
    run(&linter, "SimplifyBoolIdenticalTrueRector/Pun");
}

/// `checks` keeps the rule to the Rector rules rector.php runs. The expectations are what Rector 2.8.5 wrote with
/// `withPreparedSets(deadCode: true)` (`SimplifyBoolIdenticalTrueRector`) and with `SimplifyConditionsRector` and
/// `BooleanNotIdenticalToNotIdenticalRector` alone (both in its code-quality set).
#[test]
fn prefer_simple_condition_checks() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/prefer-simple-condition-checks");
    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let input = read("input.php");
    for (checks, expected) in [
        (serde_json::json!(["identical-true"]), "identical-true.php"),
        (
            serde_json::json!(["conditions", "not-identical"]),
            "conditions-and-not-identical.php",
        ),
    ] {
        let mut rules: Vec<Box<dyn Rule>> = vec![
            Box::new(rules::NoBooleanLiteralCompare::default()),
            Box::new(rules::NoNegatedComparison::default()),
            Box::new(rules::PreferSimpleCondition::default()),
        ];
        for rule in &mut rules {
            rule.configure(&serde_json::json!({ "checks": checks }))
                .unwrap();
        }
        let linter = Linter::new(rules, pun_lint::PhpVersion::from_id(80400));
        let fixed = linter.fix(Path::new("Conditions.php"), &input).unwrap();
        assert_eq!(fixed.source, read(expected), "{checks}");
    }
}

#[test]
fn simplify_empty_check_on_empty_array() {
    let linter = only(rules::PreferEmptyArrayIdentical::default(), 80400);
    run(&linter, "SimplifyEmptyCheckOnEmptyArrayRector/Fixture");
    run(&linter, "SimplifyEmptyCheckOnEmptyArrayRector/Oracle");
    run(&linter, "SimplifyEmptyCheckOnEmptyArrayRector/Pun");
}

#[test]
fn number_compare_to_max_func_call() {
    let linter = only(rules::PreferMinMax, 80400);
    run(&linter, "NumberCompareToMaxFuncCallRector/Fixture");
    run(&linter, "NumberCompareToMaxFuncCallRector/Oracle");
}
