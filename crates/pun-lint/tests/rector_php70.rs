//! Rector 2.8.4's own fixtures for the `php-7.0` rules, copied verbatim from rector-src (rules-tests/Php70/...) into
//! tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4 (`.oracle/rector-bin`, the
//! rule alone, `withPhpVersion(80400)`) wrote for them. Each rule also runs on every node of its fixtures
//! (`everywhere`) and must report what it reports on its target nodes.

use crate::common::run_class;
use crate::rector_refactor::everywhere;
use pun_lint::Rule;
use pun_lint::rules::{NoCallUserMethod, PreferRandomInt, PreferTime};

fn check(make: fn() -> Box<dyn Rule>, class: &str) {
    run_class(vec![make()], class);
    everywhere(make, class, 80400);
}

#[test]
fn call_user_method() {
    check(|| Box::new(NoCallUserMethod), "CallUserMethodRector");
}

#[test]
fn random_function() {
    check(|| Box::new(PreferRandomInt), "RandomFunctionRector");
}

#[test]
fn rename_mktime_without_args_to_time() {
    check(
        || Box::new(PreferTime),
        "RenameMktimeWithoutArgsToTimeRector",
    );
}
