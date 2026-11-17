//! Larastan's model properties from schema dumps and migrations in configured directories (SquashedMigrationHelper,
//! MigrationHelper, DirectoryResolver), and a macro registered outside a service provider.
//!
//! tests/fixtures/laravel-schema holds the app files: `database/schema` has MySQL dumps that reach every branch of
//! iamcal/sql-parser that IamcalSqlParser reads, dumps it fails on, a file that is not a dump and a nested directory;
//! the migrations sit in two directories a glob matches and in a hidden one it does not.
//!
//! Expected: tests/fixtures/laravel-schema/expected-phpstan.json, PHPStan 2.3.6 + Larastan 3.15.0's errors at level 2
//! on a Laravel 13.43 app with the same files (its AppServiceProvider calls `App\Support\Macros::register()`), with
//! `databaseMigrationsPath` and `squashedMigrationsPath` as below (`scan`), and again with `disableMigrationScan` and
//! `disableSchemaScan` (`disabled`).

use std::path::Path;

use pun_analyse::{Project, run};
use pun_phpstan::{Array, Key, Value as Parameter};
use serde_json::Value;

type Errors = Vec<(String, i64, String, String)>;

fn analyse(disabled: bool) -> Errors {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/laravel-schema");
    let list =
        |paths: &[&str]| Parameter::Array(Array::list(paths.iter().map(|p| Parameter::str(*p))));
    let mut parameters = Array::new();
    parameters.set(Key::from("level"), Parameter::Int(2));
    parameters.set(Key::from("noModelMake"), Parameter::Bool(true));
    parameters.set(
        Key::from("databaseMigrationsPath"),
        list(&[
            "database/*migrations",
            "database/absent",
            "database/nothing*",
        ]),
    );
    parameters.set(
        Key::from("squashedMigrationsPath"),
        list(&[
            "database/schema",
            "database/schema/nested",
            "database/schema",
        ]),
    );
    parameters.set(Key::from("disableMigrationScan"), Parameter::Bool(disabled));
    parameters.set(Key::from("disableSchemaScan"), Parameter::Bool(disabled));
    let mut project = Project::from_config(&root, &parameters);
    project.analysed = vec![
        root.join("app/Probe.php"),
        root.join("app/Support/Macros.php"),
    ];
    let mut scanned = vec![root.join("stubs/laravel.php")];
    for model in std::fs::read_dir(root.join("app/Models")).unwrap() {
        scanned.push(model.unwrap().path());
    }
    scanned.sort();
    project.scanned = scanned;
    let prefix = format!("{}/", root.display());
    let mut got: Errors = run(&project)
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

fn expected(key: &str) -> Errors {
    let oracle: Value = serde_json::from_str(include_str!(
        "fixtures/laravel-schema/expected-phpstan.json"
    ))
    .unwrap();
    oracle[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e[0].as_str().unwrap().to_string(),
                e[1].as_i64().unwrap(),
                e[2].as_str().unwrap().to_string(),
                e[3].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn model_properties_from_schema_dumps_and_migration_directories() {
    assert_eq!(analyse(false), expected("scan"));
}

#[test]
fn no_model_properties_when_the_scans_are_disabled() {
    assert_eq!(analyse(true), expected("disabled"));
}
