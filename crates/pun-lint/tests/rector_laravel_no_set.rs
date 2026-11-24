//! rector-laravel 2.7.3's rules in no set, with their own fixtures copied verbatim from driftingly/rector-laravel at
//! 699f25b (tests/Rector/...) into tests/rector/<Rector class>/Fixture (and its Source/), with rector-laravel's stubs/
//! (tests/rector-laravel-stubs) known beside them. Oracle/ holds extra inputs with what Rector 2.8.4 and rector-laravel
//! 2.7.3 (`.oracle/rector-ecosystem`, the rule alone, `phpVersion(80400)`, the stubs and Source/ on
//! `withAutoloadPaths`) wrote for them. A fixture is `input -----\n expected`, or only the input when the rule must not
//! change it.
//!
//! Pun/ holds the cases where Pun keeps the code valid or the meaning where Rector does not:
//!
//! - RouteActionCallableRector/Pun/leading_backslash: for a `'\App\FooController@index'` action Rector writes
//!   `\\App\FooController::class`, a parse error; Pun writes the class with one leading backslash.
//!
//! RouteActionCallableRector runs with upstream's `namespace` (its Source/ namespace) and a `routes` entry for
//! Oracle/routes_file, whose Rector run gave the file's absolute path. Source/MoreController.php is Pun's, for the
//! Oracle/ cases (an inherited and an `@method` method).

use std::path::Path;

use crate::common::run_class_paths;
use pun_lint::Rule;
use pun_lint::rules::PreferCallableRouteAction;
use serde_json::json;

fn check(rule: impl Rule + 'static, class: &str) {
    let stubs = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/rector-laravel-stubs");
    run_class_paths(vec![Box::new(rule)], class, 80400, &[&stubs]);
}

#[test]
fn route_action_callable() {
    let source = "RectorLaravel\\Tests\\Rector\\StaticCall\\RouteActionCallableRector\\Source";
    let mut rule = PreferCallableRouteAction::default();
    rule.configure(&json!({
        "namespace": source,
        "routes": {"Oracle/routes_file.php.inc": format!("{source}\\SomeNamespace")},
    }))
    .unwrap();
    check(rule, "RouteActionCallableRector");
    let mut rule = PreferCallableRouteAction::default();
    for (options, error) in [
        (json!([]), "options must be a table"),
        (json!({"namespace": 1}), "`namespace` must be a string"),
        (json!({"routes": []}), "`routes` must be a table"),
        (
            json!({"routes": {"a.php": 1}}),
            "`routes.a.php` must be a string",
        ),
        (
            json!({"name": "x"}),
            "unknown option `name`; the options are namespace, routes",
        ),
    ] {
        assert_eq!(rule.configure(&options).unwrap_err().to_string(), error);
    }
}
