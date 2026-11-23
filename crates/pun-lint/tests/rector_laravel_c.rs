//! rector-laravel 2.7.3's own fixtures for the `laravel` rules it added, copied verbatim from driftingly/rector-laravel
//! at 699f25b (tests/Rector/...) into tests/rector/<Rector class>/Fixture (and its Source/), with rector-laravel's
//! stubs/ (tests/rector-laravel-stubs) known beside them. Oracle/ holds extra inputs with what Rector 2.8.4 and
//! rector-laravel 2.7.3 (`.oracle/rector-ecosystem`, the rule alone, `phpVersion(80400)`, the stubs and Source/ on
//! `withAutoloadPaths`) wrote for them. A fixture is `input -----\n expected`, or only the input when the rule must not
//! change it.
//!
//! Pun/ holds the cases where Pun keeps the code valid or the meaning where Rector does not:
//!
//! - ContainerArrayAccessToMethodCallRector/Pun/simple_interpolation: in a `"$app[b]"` string part Rector writes
//!   `{$app->make(b)}`, a constant `b` instead of the string key; Pun changes only the `{$...}` parts.
//! - EmptyGuardedPropertyToUnguardedAttributeRector/Pun/multi_item: `protected $guarded = [], $fillable = [...];` Rector
//!   removes the whole statement, `$fillable` with it; Pun leaves it alone.

use std::path::Path;

use crate::common::run_class_paths;
use pun_lint::Rule;
use pun_lint::rules::{
    AddScopeBuilderGenerics, NoDefaultAuthorize, PreferCollectionIsEmpty,
    PreferContainerMethodCall, PreferFileFacadeDelete, PreferRepositoryMethodCall,
    PreferRouteKeyAttribute, PreferUnguardedAttribute,
};

fn check(rule: impl Rule + 'static, class: &str) {
    let stubs = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/rector-laravel-stubs");
    run_class_paths(vec![Box::new(rule)], class, 80400, &[&stubs]);
}

#[test]
fn unlink_func_call_to_file_facade_delete() {
    check(
        PreferFileFacadeDelete,
        "UnlinkFuncCallToFileFacadeDeleteRector",
    );
}

#[test]
fn collection_count_comparison_to_is_empty() {
    check(
        PreferCollectionIsEmpty,
        "CollectionCountComparisonToIsEmptyRector",
    );
}

#[test]
fn container_array_access_to_method_call() {
    check(
        PreferContainerMethodCall,
        "ContainerArrayAccessToMethodCallRector",
    );
}

#[test]
fn repository_array_access_to_method_call() {
    check(
        PreferRepositoryMethodCall,
        "RepositoryArrayAccessToMethodCallRector",
    );
}

#[test]
fn empty_guarded_property_to_unguarded_attribute() {
    check(
        PreferUnguardedAttribute,
        "EmptyGuardedPropertyToUnguardedAttributeRector",
    );
}

#[test]
fn route_key_method_to_route_key_attribute() {
    check(
        PreferRouteKeyAttribute,
        "RouteKeyMethodToRouteKeyAttributeRector",
    );
}

#[test]
fn remove_default_authorize_true_from_form_request() {
    check(
        NoDefaultAuthorize,
        "RemoveDefaultAuthorizeTrueFromFormRequestRector",
    );
}

#[test]
fn add_generic_builder_to_scopes() {
    check(AddScopeBuilderGenerics, "AddGenericBuilderToScopesRector");
}
