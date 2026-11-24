//! rector-laravel 2.7.3's own fixtures for its upgrade rules below Laravel 12, its package upgrade rules below the
//! current majors, its legacy-factories and Lumen rules and UseForwardsCallsTraitRector, copied verbatim from driftingly/rector-laravel at 699f25b
//! (tests/Rector/...) into tests/rector/<Rector class>/Fixture (and its Source/), with rector-laravel's stubs/
//! (tests/rector-laravel-stubs) known beside them. Oracle/ holds extra inputs with what Rector 2.8.4 and
//! rector-laravel 2.7.3 (`.oracle/rector-ecosystem`, the rule alone, `phpVersion(80400)`, the stubs and Source/ on
//! `withAutoloadPaths`, laravel/framework 13 in `vendor/composer/installed.json`) wrote for them. A fixture is
//! `input -----\n expected`, or only the input when the rule must not change it. FactoryDefinitionRector's
//! Fixture/WithoutConfiguration and Fixture/Configured are Fixture and FixtureConfigured here, the second run with the
//! configuration of its configured_rule_with_configuration.php.
//!
//! Pun/ holds the documented cases where Pun differs. UnifyModelDatesWithCastsRector/Pun/skip_constant_date: Pun reads
//! only string literals in `$dates` and leaves a model that names a class constant alone, where Rector resolves the
//! constant. UseForwardsCallsTraitRector/Pun/dynamic_method_name: Pun leaves a call to `$this->{$m}(...)` alone, where
//! Rector writes the variable's name as the method.

use std::path::{Path, PathBuf};

use crate::common::{run_class_paths, run_dirs};
use pun_lint::Rule;
use pun_lint::rules::{
    AddArgumentDefault, AddLoginEventGuard, AddMockConsoleOutputFalse, AddParentBootCall,
    AddParentRegisterCall, AddWhereTime, NoExpressionStringCast, NoExpressionToString,
    NoFakerPropertyAccess, PreferAssertSeeHtml, PreferAssertSentTimes, PreferAttributeAccessor,
    PreferCastsMethodFix, PreferComputedAttribute, PreferDateCasts, PreferDeferrableProvider,
    PreferDispatchSync, PreferFacadeFake, PreferFactoryClasses, PreferFactoryStateMethods,
    PreferFakeHelper, PreferFakeOverExpects, PreferForwardsCalls, PreferGeometryColumn,
    PreferMiddlewareArray, PreferModelFactoryMethod, PreferPermanentRedirect, PreferQueueableTrait,
    PreferStripeMethod, PreferUrlAttribute, PreferUsesArray, ReplaceEventStringWithConstant,
};

fn stubs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/rector-laravel-stubs")
}

fn check(rule: impl Rule + 'static, class: &str) {
    run_class_paths(vec![Box::new(rule)], class, 80400, &[&stubs()]);
}

/// The rule with the configuration of upstream's `config/configured_rule.php`.
fn configured(mut rule: impl Rule + 'static, options: &serde_json::Value, class: &str) {
    rule.configure(options).unwrap();
    check(rule, class);
}

#[test]
fn redirect_301_to_permanent_redirect() {
    check(
        PreferPermanentRedirect,
        "Redirect301ToPermanentRedirectRector",
    );
}

#[test]
fn assert_see_to_assert_see_html() {
    check(PreferAssertSeeHtml, "AssertSeeToAssertSeeHtmlRector");
}

#[test]
fn replace_assert_times_send_with_assert_sent_times() {
    check(
        PreferAssertSentTimes,
        "ReplaceAssertTimesSendWithAssertSentTimesRector",
    );
}

#[test]
fn database_expression_casts_to_method_call() {
    check(
        NoExpressionStringCast,
        "DatabaseExpressionCastsToMethodCallRector",
    );
}

#[test]
fn database_expression_to_string_to_method_call() {
    check(
        NoExpressionToString,
        "DatabaseExpressionToStringToMethodCallRector",
    );
}

#[test]
fn refactor_blueprint_geometry_columns() {
    check(
        PreferGeometryColumn,
        "RefactorBlueprintGeometryColumnsRector",
    );
}

#[test]
fn replace_without_jobs_events_and_notifications_with_facade_fake() {
    check(
        PreferFacadeFake,
        "ReplaceWithoutJobsEventsAndNotificationsWithFacadeFakeRector",
    );
}

#[test]
fn dispatch_non_should_queue_to_dispatch_sync() {
    check(
        PreferDispatchSync,
        "DispatchNonShouldQueueToDispatchSyncRector",
    );
}

#[test]
fn replace_faker_instance_with_helper() {
    check(PreferFakeHelper, "ReplaceFakerInstanceWithHelperRector");
}

#[test]
fn add_parent_boot_to_model_class_method() {
    check(AddParentBootCall, "AddParentBootToModelClassMethodRector");
}

#[test]
fn add_parent_register_to_event_service_provider() {
    check(
        AddParentRegisterCall,
        "AddParentRegisterToEventServiceProviderRector",
    );
}

#[test]
fn property_defer_to_deferrable_provider_to() {
    check(
        PreferDeferrableProvider,
        "PropertyDeferToDeferrableProviderToRector",
    );
}

#[test]
fn replace_queue_traits_with_queueable() {
    check(
        PreferQueueableTrait,
        "ReplaceQueueTraitsWithQueueableRector",
    );
}

#[test]
fn unify_model_dates_with_casts() {
    check(PreferDateCasts, "UnifyModelDatesWithCastsRector");
}

#[test]
fn model_casts_property_to_casts_method() {
    check(
        PreferCastsMethodFix,
        "ModelCastsPropertyToCastsMethodRector",
    );
}

#[test]
fn add_mock_console_output_false_to_console_tests() {
    check(
        AddMockConsoleOutputFalse,
        "AddMockConsoleOutputFalseToConsoleTestsRector",
    );
}

#[test]
fn add_guard_to_login_event() {
    check(AddLoginEventGuard, "AddGuardToLoginEventRector");
}

#[test]
fn change_query_where_date_value_with_carbon() {
    check(AddWhereTime, "ChangeQueryWhereDateValueWithCarbonRector");
}

#[test]
fn replace_expects_methods_in_tests() {
    check(PreferFakeOverExpects, "ReplaceExpectsMethodsInTestsRector");
}

#[test]
fn cashier_stripe_options_to_stripe() {
    check(PreferStripeMethod, "CashierStripeOptionsToStripeRector");
}

#[test]
fn replace_faker_property_fetch_with_method_call() {
    check(
        NoFakerPropertyAccess,
        "ReplaceFakerPropertyFetchWithMethodCallRector",
    );
}

#[test]
fn livewire_component_computed_method_to_computed_attribute() {
    check(
        PreferComputedAttribute,
        "LivewireComponentComputedMethodToComputedAttributeRector",
    );
}

#[test]
fn livewire_component_query_string_to_url_attribute() {
    check(
        PreferUrlAttribute,
        "LivewireComponentQueryStringToUrlAttributeRector",
    );
}

#[test]
fn lumen_routes_string_action_to_uses_array() {
    check(PreferUsesArray, "LumenRoutesStringActionToUsesArrayRector");
}

#[test]
fn lumen_routes_string_middleware_to_array() {
    check(
        PreferMiddlewareArray,
        "LumenRoutesStringMiddlewareToArrayRector",
    );
}

#[test]
fn migrate_to_simplified_attribute() {
    check(
        PreferAttributeAccessor,
        "MigrateToSimplifiedAttributeRector",
    );
}

#[test]
fn factory_applying_states() {
    check(PreferFactoryStateMethods, "FactoryApplyingStatesRector");
}

#[test]
fn factory_func_call_to_static_call() {
    check(
        PreferModelFactoryMethod::default(),
        "FactoryFuncCallToStaticCallRector",
    );
}

#[test]
fn add_argument_default_value() {
    configured(
        AddArgumentDefault::default(),
        &serde_json::json!({"items": [{
            "class": "Illuminate\\Contracts\\Events\\Dispatcher",
            "method": "listen",
            "position": 1,
            "default-value": null,
        }]}),
        "AddArgumentDefaultValueRector",
    );
}

#[test]
fn event_string_to_class_constant() {
    configured(
        ReplaceEventStringWithConstant::default(),
        &serde_json::json!({"items": [
            {"string": "auth.login", "class": "Illuminate\\Auth\\Events\\Login", "constant": "class"},
            {"string": "auth.logout", "class": "Illuminate\\Auth\\Events\\Logout", "constant": "class"},
        ]}),
        "EventStringToClassConstantRector",
    );
}

#[test]
fn factory_definition() {
    check(PreferFactoryClasses::default(), "FactoryDefinitionRector");
    let mut configured = PreferFactoryClasses::default();
    configured
        .configure(&serde_json::json!({"items": [
            "RectorLaravel\\Tests\\Rector\\Namespace_\\FactoryDefinitionRector\\Fixture\\Configured\\Model",
            "App\\User",
            "RectorLaravel\\Tests\\Rector\\Namespace_\\FactoryDefinitionRector\\Fixture\\Configured\\Account",
            "RectorLaravel\\Tests\\Rector\\Namespace_\\FactoryDefinitionRector\\Source\\Model",
        ]}))
        .unwrap();
    run_dirs(
        vec![Box::new(configured)],
        "FactoryDefinitionRector",
        &["FixtureConfigured"],
        80400,
        &[&stubs()],
    );
}

#[test]
fn use_forwards_calls_trait() {
    check(PreferForwardsCalls, "UseForwardsCallsTraitRector");
}
