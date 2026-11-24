//! Rector 2.8.4's own fixtures for its datetime-to-carbon rules, copied verbatim from rector-src's rules-tests at 2.8.4
//! (Carbon/Rector/...) into tests/rector/<Rector class>/Fixture. Oracle/ holds extra inputs with what Rector 2.8.4
//! (`.oracle/rector-bin`, the rule alone, `phpVersion(80400)`) wrote for them. `lint/style/prefer-carbon` is both
//! `DateTimeInstanceToCarbonRector` and `DateTimeMethodCallToCarbonRector`, so it runs on the fixtures of both.

use crate::common::run_class;
use pun_lint::rules::{PreferCarbon, PreferCarbonTimestamp};

#[test]
fn date_time_instance_to_carbon() {
    run_class(
        vec![Box::new(PreferCarbon)],
        "DateTimeInstanceToCarbonRector",
    );
}

#[test]
fn date_time_method_call_to_carbon() {
    run_class(
        vec![Box::new(PreferCarbon)],
        "DateTimeMethodCallToCarbonRector",
    );
}

#[test]
fn time_func_call_to_carbon() {
    run_class(
        vec![Box::new(PreferCarbonTimestamp)],
        "TimeFuncCallToCarbonRector",
    );
}
