//! rector-laravel 2.7.3's own fixtures for its rules in no set, copied verbatim from driftingly/rector-laravel at
//! 699f25b (tests/Rector/...) into tests/rector/<Rector class>/Fixture (and its Source/), with rector-laravel's stubs/
//! (tests/rector-laravel-stubs) known beside them. WhereToWhereLikeRector's Fixture/Default and Fixture/Postgres are
//! Fixture and FixturePostgres here, the second run with the configuration of its postgres_configured_rule.php.
//! Oracle/ holds extra inputs with what Rector 2.8.4 and rector-laravel 2.7.3 (`.oracle/rector-ecosystem`, the rule
//! alone, `phpVersion(80400)`, the stubs and Source/ on `withAutoloadPaths`) wrote for them. A fixture is
//! `input -----\n expected`, or only the input when the rule must not change it.
//!
//! Pun/ holds the documented cases where Pun differs. ResponseHelperCallToJsonResponseRector/Pun/multiline_args: Pun
//! keeps a multiline argument list as written, where Rector reprints it on one line.
//! MinutesToSecondsInCacheRector/Pun/ternary_and_named: Pun wraps `$minutes ?: 1` in parentheses before `* 60`, where
//! Rector writes `$minutes ?: 1 * 60` (which multiplies only the `1`), and keeps a named `seconds:` argument named,
//! where Rector drops the name. RemoveModelPropertyFromFactoriesRector/Pun/multi_item: Pun leaves a `$model` declared in
//! one statement with other properties alone, where Rector removes the whole statement.

use std::path::{Path, PathBuf};

use crate::common::{run_class_paths, run_dirs};
use pun_lint::Rule;
use pun_lint::rules::{
    NoDumpCalls, NoFactoryModelProperty, PreferBlankFilled, PreferCacheSeconds,
    PreferComponentsProperty, PreferDateShorthand, PreferJsonResponse, PreferTypedConfig,
    PreferWhereLike, PreferWhereNull,
};

fn stubs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/rector-laravel-stubs")
}

fn check(rule: impl Rule + 'static, class: &str) {
    run_class_paths(vec![Box::new(rule)], class, 80400, &[&stubs()]);
}

#[test]
fn empty_to_blank_and_filled_func() {
    check(PreferBlankFilled, "EmptyToBlankAndFilledFuncRector");
}

#[test]
fn response_helper_call_to_json_response() {
    check(PreferJsonResponse, "ResponseHelperCallToJsonResponseRector");
}

#[test]
fn config_to_typed_config_method_call() {
    check(PreferTypedConfig, "ConfigToTypedConfigMethodCallRector");
}

#[test]
fn remove_dump_data_dead_code() {
    check(NoDumpCalls::default(), "RemoveDumpDataDeadCodeRector");
}

#[test]
fn use_component_property_within_commands() {
    check(
        PreferComponentsProperty,
        "UseComponentPropertyWithinCommandsRector",
    );
}

#[test]
fn remove_model_property_from_factories() {
    check(
        NoFactoryModelProperty,
        "RemoveModelPropertyFromFactoriesRector",
    );
}

#[test]
fn where_null_comparison_to_where_null() {
    check(PreferWhereNull, "WhereNullComparisonToWhereNullRector");
}

#[test]
fn date_where_clause_to_shorthand() {
    check(PreferDateShorthand, "DateWhereClauseToShorthandRector");
}

#[test]
fn minutes_to_seconds_in_cache() {
    check(PreferCacheSeconds, "MinutesToSecondsInCacheRector");
}

#[test]
fn where_to_where_like() {
    check(PreferWhereLike::default(), "WhereToWhereLikeRector");
    let mut postgres = PreferWhereLike::default();
    postgres
        .configure(&serde_json::json!({"using-postgres-driver": true}))
        .unwrap();
    run_dirs(
        vec![Box::new(postgres)],
        "WhereToWhereLikeRector",
        &["FixturePostgres"],
        80400,
        &[&stubs()],
    );
}

#[test]
fn options() {
    let mut like = PreferWhereLike::default();
    for (options, error) in [
        (serde_json::json!([true]), "options must be a table"),
        (
            serde_json::json!({"using-postgres-driver": 1}),
            "`using-postgres-driver` must be a boolean",
        ),
        (
            serde_json::json!({"postgres": true}),
            "unknown option `postgres`; the option is using-postgres-driver",
        ),
    ] {
        assert_eq!(like.configure(&options).unwrap_err().to_string(), error);
    }
    like.configure(&serde_json::json!({"using-postgres-driver": true}))
        .unwrap();
    like.configure(&serde_json::Value::Null).unwrap();
    assert!(!like.using_postgres_driver);
    let mut dump = NoDumpCalls::default();
    assert_eq!(
        dump.configure(&serde_json::json!({"items": 1}))
            .unwrap_err()
            .to_string(),
        "`items` must be an array"
    );
    dump.configure(&serde_json::Value::Null).unwrap();
}
