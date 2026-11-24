//! Rector 2.8.4's own fixtures for the `php-8.1` rules, copied verbatim from rector-src (rules-tests/Php81/... and
//! rules-tests/CodingStyle/...) into tests/rector/<Rector class>/Fixture. Each rule also runs on every node of its
//! fixtures (`everywhere`) and must report what it reports on its target nodes.

use crate::common::run_class;
use crate::rector_refactor::everywhere;
use pun_lint::Rule;
use pun_lint::rules::{
    NoClosureFromCallable, NoNullPregSplitLimit, NoReflectionSetAccessible,
    PreferFirstClassCallableArgument, PreferFirstClassCallableOverArray,
    PreferFirstClassCallableOverArrowFunction, PreferFirstClassCallableOverClosure,
};

fn check(make: fn() -> Box<dyn Rule>, class: &str) {
    run_class(vec![make()], class);
    everywhere(make, class, 80400);
}

#[test]
fn function_first_class_callable() {
    check(
        || Box::new(PreferFirstClassCallableArgument),
        "FunctionFirstClassCallableRector",
    );
}

#[test]
fn closure_from_callable_to_first_class_callable() {
    check(
        || Box::new(NoClosureFromCallable),
        "ClosureFromCallableToFirstClassCallableRector",
    );
}

#[test]
fn remove_reflection_set_accessible_calls() {
    check(
        || Box::new(NoReflectionSetAccessible),
        "RemoveReflectionSetAccessibleCallsRector",
    );
}

#[test]
fn null_to_strict_int_preg_split_limit_arg() {
    check(
        || Box::new(NoNullPregSplitLimit),
        "NullToStrictIntPregSlitFuncCallLimitArgRector",
    );
}

#[test]
fn array_to_first_class_callable() {
    check(
        || Box::new(PreferFirstClassCallableOverArray),
        "ArrayToFirstClassCallableRector",
    );
}

#[test]
fn arrow_function_delegating_call_to_first_class_callable() {
    check(
        || Box::new(PreferFirstClassCallableOverArrowFunction),
        "ArrowFunctionDelegatingCallToFirstClassCallableRector",
    );
}

#[test]
fn closure_delegating_call_to_first_class_callable() {
    check(
        || Box::new(PreferFirstClassCallableOverClosure),
        "ClosureDelegatingCallToFirstClassCallableRector",
    );
}
