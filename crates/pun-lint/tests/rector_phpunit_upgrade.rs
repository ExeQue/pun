//! rector-phpunit's fixtures for its upgrade rules below phpunit110 (the `PHPUnit50` ... `PHPUnit100` rules that
//! the composer-based set registers), copied verbatim from rectorphp/rector-phpunit at 22c6dae (rules-tests/...) into
//! tests/rector/<Rector class>/Fixture (and its Source/), run as `rector_phpunit.rs` runs its fixtures. Oracle/ holds
//! extra inputs with what Rector 2.8.4 and that rector-phpunit (`.oracle/rector-ecosystem`, the rule alone,
//! `phpVersion(80400)`, tests/phpunit-stubs loaded by a bootstrap autoloader, and a `vendor/composer/installed.json`
//! with the phpunit/phpunit version the rule's constraint asks for) wrote for them. Pun/ holds the documented cases
//! where Pun differs:
//!
//! PublicDataProviderClassMethodRector/Pun/respect_parent_*: two upstream fixtures, kept here with Pun's output.
//! Rector also reads the `@dataProvider` tags of the test methods in the abstract parent classes below `TestCase`
//! (their AST, from another file); Pun reads only the class's own test methods, so it leaves a provider that only a
//! parent's test names alone.
//!
//! RemoveDataProviderTestPrefixRector/Pun/imported_data_provider: a `use` of a class named `DataProvider` (here the
//! attribute) makes Rector's PHPDoc parser read `@dataProvider` as a Doctrine annotation of that class, so Rector
//! finds no provider through the tag (`PublicDataProviderClassMethodRector` too); Pun reads the tag either way.
//!
//! AssertRegExpRector/Pun/condition_variable: an expected value that is not an integer or a constant, or a
//! `preg_match()` without a subject, makes Rector fail on the file; Pun leaves the call alone.
//!
//! TestListenerToHooksRector/Pun/before_list_hook: the upstream fixture with the blank line between the two kept
//! methods, which Pun keeps as written where Rector's reprint of the class drops it.

use crate::rector_phpunit::run_class;
use pun_lint::Rule;
use pun_lint::rules::{
    AddParentTestConstructor, AddProphecyTrait, NoAssertClassHasAttribute,
    NoAssertContainsOnString, NoDataProviderTestPrefix, NoMockBuilderSetMethods,
    NoScenarioAnnotation, NoTestListener, PreferAssertContainsEquals,
    PreferAssertMatchesRegularExpression, PreferExpectPhpError, PreferSpecificAssertEquals,
    RequirePublicDataProvider,
};

fn check(rule: impl Rule + 'static, class: &str) {
    run_class(vec![Box::new(rule)], class);
}

#[test]
fn specific_assert_contains() {
    check(NoAssertContainsOnString, "SpecificAssertContainsRector");
}

#[test]
fn specific_assert_contains_without_identity() {
    check(
        PreferAssertContainsEquals,
        "SpecificAssertContainsWithoutIdentityRector",
    );
}

#[test]
fn assert_equals_parameter_to_specific_methods_type() {
    check(
        PreferSpecificAssertEquals,
        "AssertEqualsParameterToSpecificMethodsTypeRector",
    );
}

#[test]
fn assert_reg_exp() {
    check(PreferAssertMatchesRegularExpression, "AssertRegExpRector");
}

#[test]
fn explicit_php_error_api() {
    check(PreferExpectPhpError, "ExplicitPhpErrorApiRector");
}

#[test]
fn property_exists_without_assert() {
    check(
        NoAssertClassHasAttribute,
        "PropertyExistsWithoutAssertRector",
    );
}

#[test]
fn remove_set_methods_method_call() {
    check(NoMockBuilderSetMethods, "RemoveSetMethodsMethodCallRector");
}

#[test]
fn public_data_provider_class_method() {
    check(
        RequirePublicDataProvider,
        "PublicDataProviderClassMethodRector",
    );
}

#[test]
fn remove_data_provider_test_prefix() {
    check(
        NoDataProviderTestPrefix,
        "RemoveDataProviderTestPrefixRector",
    );
}

#[test]
fn scenario_to_test_annotation() {
    check(NoScenarioAnnotation, "ScenarioToTestAnnotationRector");
}

#[test]
fn add_prophecy_trait() {
    check(AddProphecyTrait, "AddProphecyTraitRector");
}

#[test]
fn parent_test_class_constructor() {
    check(AddParentTestConstructor, "ParentTestClassConstructorRector");
}

#[test]
fn test_listener_to_hooks() {
    check(NoTestListener, "TestListenerToHooksRector");
}
