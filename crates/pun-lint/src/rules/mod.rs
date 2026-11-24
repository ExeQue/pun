/// Re-exports each listed rule and defines `$fn`, which builds them in the listed order. The order is the order
/// `Linter` applies fixes that start at the same offset in, so it is part of the output.
macro_rules! rules {
    ($vis:vis fn $fn:ident { $($($module:ident)::+ => $rule:ident $(:: $ctor:ident ())?,)* }) => {
        $(pub use $($module)::+::$rule;)*

        $vis fn $fn() -> Vec<Box<dyn crate::Rule>> {
            vec![$(Box::new($rule $(:: $ctor())?),)*]
        }
    };
}

mod arrays;
mod classes;
mod complexity;
mod control_flow;
mod dead_code;
mod functions;
mod laravel;
mod operators;
mod pest;
mod php_5_3;
mod php_5_5;
mod php_7_0;
mod php_7_1;
mod php_7_2;
mod php_7_3;
mod php_7_4;
mod php_8_0;
mod php_8_1;
mod php_8_2;
mod php_8_3;
mod php_8_4;
mod php_8_5;
mod php_8_6;
mod phpdoc;
mod phpunit;
mod refactor;
mod security;
mod statements;
mod strings;
mod style;
pub(crate) mod support;
mod tags;
pub(crate) mod types;

pub use laravel::*;
pub use pest::*;
pub use phpunit::*;
pub use refactor::*;
pub use types::*;

use crate::Rule;

/// Every built-in rule: the core's, then the Laravel, Pest and PHPUnit extensions', in the order `pun` registers
/// them.
pub fn all() -> Vec<Box<dyn Rule>> {
    let mut rules = core();
    rules.extend(laravel());
    rules.extend(pest());
    rules.extend(phpunit());
    rules
}

/// The rules outside every extension's groups.
pub fn core() -> Vec<Box<dyn Rule>> {
    let mut rules = types::all();
    rules.extend(refactor::all());
    rules.extend(laravel::outside());
    rules.extend(ungrouped());
    rules
}

/// The Laravel extension's rules.
pub fn laravel() -> Vec<Box<dyn Rule>> {
    laravel::all()
}

/// The Pest extension's rules.
pub fn pest() -> Vec<Box<dyn Rule>> {
    pest::all()
}

/// The PHPUnit extension's rules.
pub fn phpunit() -> Vec<Box<dyn Rule>> {
    phpunit::all()
}

rules! {
    fn ungrouped {
        complexity::no_and_assignment => NoAndAssignment,
        complexity::prefer_null_coalesce => PreferNullCoalesce::default(),
        complexity::no_negated_and => NoNegatedAnd::default(),
        complexity::no_concat_assign_new_item => NoConcatAssignNewItem,
        complexity::no_negated_boolean_literal => NoNegatedBooleanLiteral,
        complexity::no_negated_group => NoNegatedGroup::default(),
        complexity::no_negated_instanceof_ternary => NoNegatedInstanceofTernary,
        complexity::no_dirname_dir_concat => NoDirnameDirConcat,
        complexity::prefer_explicit_array_comparison => PreferExplicitArrayComparison,
        complexity::prefer_if_statement => PreferIfStatement,
        complexity::prefer_empty_array_compare => PreferEmptyArrayCompare::default(),
        complexity::prefer_func_num_args => PreferFuncNumArgs,
        complexity::no_useless_array_values => NoUselessArrayValues,
        dead_code::no_tautology_ternary => NoTautologyTernary::default(),
        complexity::no_single_item_in_array => NoSingleItemInArray,
        dead_code::no_and_true => NoAndTrue,
        dead_code::no_cast_in_concat => NoCastInConcat,
        dead_code::no_useless_continue => NoUselessContinue,
        dead_code::no_empty_block => NoEmptyBlock::default(),
        dead_code::no_useless_catch => NoUselessCatch,
        dead_code::no_double_self_assignment => NoDoubleSelfAssignment,
        dead_code::no_duplicate_array_key => NoDuplicateArrayKey,
        style::no_redundant_final_const => NoRedundantFinalConst,
        dead_code::no_unused_foreach_key => NoUnusedForeachKey,
        dead_code::no_return_in_constructor => NoReturnInConstructor,
        dead_code::no_unreachable_code => NoUnreachableCode,
        dead_code::no_redundant_promoted_assign => NoRedundantPromotedAssign,
        dead_code::no_duplicate_case_body => NoDuplicateCaseBody,
        dead_code::no_identical_branches => NoIdenticalBranches,
        control_flow::no_else_after_return => NoElseAfterReturn,
        style::prefer_early_return => PreferEarlyReturn,
        style::prefer_early_return_over_prepared_value => PreferEarlyReturnOverPreparedValue,
        complexity::prefer_in_array => PreferInArray::default(),
        complexity::prefer_not_in_array => PreferNotInArray,
        complexity::no_redundant_null_check => NoRedundantNullCheck::default(),
        complexity::prefer_simple_condition => PreferSimpleCondition::default(),
        complexity::prefer_in_array_over_search => PreferInArrayOverSearch::default(),
        style::no_assign_in_ternary_branch => NoAssignInTernaryBranch,
        complexity::no_single_case_switch => NoSingleCaseSwitch,
        dead_code::no_implode_ternary => NoImplodeTernary,
        classes::prefer_new_self => PreferNewSelf,
        complexity::prefer_array_literal => PreferArrayLiteral,
        complexity::prefer_absolute_include => PreferAbsoluteInclude,
        dead_code::no_future_php_version_check => NoFuturePhpVersionCheck,
        dead_code::no_dead_php_version_check => NoDeadPhpVersionCheck,
        dead_code::no_unused_public_method_param => NoUnusedPublicMethodParam,
        dead_code::no_unused_private_method_param => NoUnusedPrivateMethodParam::default(),
        dead_code::no_unused_private_property => NoUnusedPrivateProperty,
        dead_code::no_redundant_property_default => NoRedundantPropertyDefault,
        complexity::prefer_property_default => PreferPropertyDefault,
        complexity::prefer_in_array_over_foreach => PreferInArrayOverForeach::default(),
        complexity::prefer_coalesce_chain => PreferCoalesceChain::default(),
        complexity::prefer_coalesce_over_foreach => PreferCoalesceOverForeach::default(),
        style::prefer_property_over_local_getter => PreferPropertyOverLocalGetter,
        control_flow::no_empty_catch => NoEmptyCatch,
        control_flow::no_unsafe_finally => NoUnsafeFinally,
        control_flow::no_assign_in_condition => NoAssignInCondition,
        complexity::no_unused_label => NoUnusedLabel,
        complexity::no_redundant_write_visibility => NoRedundantWriteVisibility,
        complexity::no_iterator_to_array_in_foreach => NoIteratorToArrayInForeach,
        tags::no_invalid_open_tag => NoInvalidOpenTag,
        refactor::no_negated_condition => NoNegatedCondition,
        dead_code::no_unused_global => NoUnusedGlobal,
        dead_code::no_unused_static => NoUnusedStatic,
        complexity::no_duplicate_match_arm => NoDuplicateMatchArm,
        dead_code::no_useless_condition => NoUselessCondition,
        dead_code::no_condition_exact_return => NoConditionExactReturn,
        dead_code::no_overridden_assign_before_if => NoOverriddenAssignBeforeIf,
        dead_code::no_redundant_type_check => NoRedundantTypeCheck,
        dead_code::no_overwritten_assignment => NoOverwrittenAssignment::default(),
        dead_code::no_redundant_condition => NoRedundantCondition,
        dead_code::no_empty_loop => NoEmptyLoop::default(),
        dead_code::no_unused_expression => NoUnusedExpression,
        complexity::no_sole_value_sprintf => NoSoleValueSprintf,
        complexity::no_compact => NoCompact,
        complexity::require_is_a_allow_string => RequireIsAAllowString,
        php_5_3::no_http_vars => NoHttpVars,
        php_5_5::no_class_magic_constant => NoClassMagicConstant,
        php_5_5::prefer_self_over_get_called_class => PreferSelfOverGetCalledClass,
        php_5_5::prefer_static_over_get_called_class => PreferStaticOverGetCalledClass,
        php_7_0::no_call_user_method => NoCallUserMethod,
        php_7_0::no_list_from_string => NoListFromString,
        php_7_0::prefer_braced_variable_variable => PreferBracedVariableVariable,
        php_7_0::prefer_coalesce_return => PreferCoalesceReturn,
        php_7_0::prefer_random_int => PreferRandomInt,
        php_7_0::prefer_spaceship => PreferSpaceship,
        php_7_0::prefer_spaceship_return => PreferSpaceshipReturn,
        php_7_0::prefer_static_method_call => PreferStaticMethodCall,
        php_7_0::prefer_throwable_handler => PreferThrowableHandler,
        php_7_0::prefer_time => PreferTime,
        php_7_1::no_append_to_string => NoAppendToString,
        php_7_1::no_non_numeric_string_operand => NoNonNumericStringOperand,
        php_7_1::prefer_is_iterable => PreferIsIterable,
        php_7_1::prefer_multi_catch => PreferMultiCatch,
        php_7_2::no_constant_define_name => NoConstantDefineName,
        php_7_2::no_each => NoEach,
        php_7_2::no_get_class_null => NoGetClassNull,
        php_7_2::no_list_each => NoListEach,
        php_7_2::no_string_assert => NoStringAssert,
        php_7_2::prefer_foreach_over_each => PreferForeachOverEach,
        php_7_2::require_parse_str_result => RequireParseStrResult,
        php_7_3::no_case_insensitive_constant => NoCaseInsensitiveConstant,
        php_7_3::no_case_insensitive_define => NoCaseInsensitiveDefine,
        php_7_3::no_heredoc_label_in_body => NoHeredocLabelInBody,
        php_7_3::no_unescaped_regex_dash => NoUnescapedRegexDash,
        php_7_3::prefer_array_key_first => PreferArrayKeyFirst,
        php_7_3::prefer_array_key_first_last => PreferArrayKeyFirstLast,
        php_7_3::prefer_is_countable => PreferIsCountable,
        php_7_3::prefer_setcookie_options => PreferSetcookieOptions,
        php_7_3::require_string_needle => RequireStringNeedle,
        php_7_4::add_null_property_default => AddNullPropertyDefault,
        php_7_4::no_array_key_exists_on_object => NoArrayKeyExistsOnObject,
        php_7_4::no_filter_magic_quotes => NoFilterMagicQuotes,
        php_7_4::no_hebrevc => NoHebrevc,
        php_7_4::no_mb_strrpos_encoding_offset => NoMbStrrposEncodingOffset,
        php_7_4::no_money_format => NoMoneyFormat,
        php_7_4::no_reflection_export => NoReflectionExport,
        php_7_4::no_restore_include_path => NoRestoreIncludePath,
        php_7_4::prefer_arrow_function => PreferArrowFunction,
        php_7_4::prefer_coalesce_assign_over_if => PreferCoalesceAssignOverIf,
        php_8_0::no_int_get_headers_format => NoIntGetHeadersFormat,
        php_8_0::no_unused_catch_variable => NoUnusedCatchVariable,
        php_8_0::no_version_compare_operator_alias => NoVersionCompareOperatorAlias,
        php_8_0::prefer_get_debug_type => PreferGetDebugType,
        php_8_0::prefer_match => PreferMatch,
        php_8_0::prefer_nullsafe_over_ternary => PreferNullsafeOverTernary,
        php_8_0::prefer_static_class => PreferStaticClass,
        php_8_0::prefer_str_contains_over_mb_strpos => PreferStrContainsOverMbStrpos,
        php_8_0::prefer_str_ends_with => PreferEndsWithFunction,
        php_8_0::prefer_str_starts_with => PreferStartsWithFunction,
        php_8_1::no_closure_from_callable => NoClosureFromCallable,
        php_8_1::no_null_preg_split_limit => NoNullPregSplitLimit,
        php_8_1::no_reflection_set_accessible => NoReflectionSetAccessible,
        php_8_1::prefer_enum_case_over_spatie_call => PreferEnumCaseOverSpatieCall,
        php_8_1::prefer_enum_from => PreferEnumFrom,
        php_8_1::prefer_first_class_callable_argument => PreferFirstClassCallableArgument,
        php_8_1::prefer_first_class_callable_over_array => PreferFirstClassCallableOverArray,
        php_8_1::prefer_first_class_callable_over_arrow_function => PreferFirstClassCallableOverArrowFunction,
        php_8_1::prefer_first_class_callable_over_closure => PreferFirstClassCallableOverClosure,
        php_8_1::prefer_native_enum_over_myclabs => PreferNativeEnumOverMyCLabs,
        php_8_1::prefer_native_enum_over_spatie => PreferNativeEnumOverSpatie::default(),
        php_8_1::prefer_readonly_property => PreferReadonlyProperty,
        php_8_2::add_skip_dots_flag => AddSkipDotsFlag,
        php_8_2::no_dollar_brace_interpolation => NoDollarBraceInterpolation,
        php_8_2::no_utf8_encode_decode => NoUtf8EncodeDecode,
        php_8_2::prefer_readonly_class => PreferReadonlyClass,
        php_8_3::add_const_type => AddConstType::default(),
        php_8_3::prefer_readonly_anonymous_class => PreferReadonlyAnonymousClass,
        php_8_3::prefer_json_validate => PreferJsonValidate,
        php_8_3::prefer_self_class => PreferSelfClass,
        php_8_3::prefer_dynamic_class_constant => PreferDynamicClassConstant,
        php_8_3::prefer_ldap_uri => PreferLdapUri,
        php_8_4::add_csv_escape_argument => AddCsvEscapeArgument,
        php_8_4::prefer_round_mode_enum => PreferRoundModeEnum,
        php_8_5::no_finfo_buffer_context => NoFinfoBufferContext,
        php_8_5::no_null_debug_info => NoNullDebugInfo,
        php_8_5::no_null_array_key => NoNullArrayKey,
        statements::add_safe_strict_types => AddSafeStrictTypes,
        php_8_5::no_out_of_range_chr => NoOutOfRangeChr,
        php_8_5::prefer_ord_first_byte => PreferOrdFirstByte,
        php_8_5::prefer_array_first_last => PreferArrayFirstLast,
        php_8_6::prefer_clamp => PreferClamp,
        php_8_6::prefer_readonly_property_default => PreferReadonlyPropertyDefault,
        php_8_4::prefer_array_all => PreferArrayAll::default(),
        php_8_4::prefer_array_any => PreferArrayAny::default(),
        php_8_4::prefer_array_find => PreferArrayFind::default(),
        php_8_4::prefer_array_find_key => PreferArrayFindKey::default(),
        security::no_eval => NoEval,
        security::no_global => NoGlobal,
        security::no_request_variable => NoRequestVariable,
        style::no_dynamic_name => NoDynamicName,
        classes::no_underscore_class => NoUnderscoreClass,
        functions::no_void_reference_return => NoVoidReferenceReturn,
        style::no_nested_ternary => NoNestedTernary,
        style::no_empty => NoEmpty,
        style::no_short_ternary => NoShortTernary,
        style::no_variable_variable => NoVariableVariable,
        style::prefer_carbon => PreferCarbon,
        style::prefer_carbon => PreferCarbonTimestamp,
        classes::no_explicit_constructor_call => NoExplicitConstructorCall,
        functions::no_assign_in_argument => NoAssignInArgument,
        functions::no_swapped_explode_arguments => NoSwappedExplodeArguments,
        security::require_preg_quote_delimiter => RequirePregQuoteDelimiter,
        security::no_ffi => NoFfi,
        security::no_insecure_comparison => NoInsecureComparison,
        security::no_literal_password => NoLiteralPassword,
        style::no_hash_emoji => NoHashEmoji,
        functions::no_missing_format_argument => NoMissingFormatArgument,
        style::no_parameter_shadowing => NoParameterShadowing,
        style::no_unreachable_loop => NoUnreachableLoop,
        style::prefer_instanceof_stringable => PreferInstanceofStringable,
        style::no_array_accumulation_in_loop => NoArrayAccumulationInLoop,
        style::prefer_early_continue => PreferEarlyContinue,
        security::no_tainted_data => NoTaintedData,
        classes::no_protected_constant_in_final => NoProtectedConstantInFinal,
        classes::no_protected_method_in_final => NoProtectedMethodInFinal,
        classes::no_protected_property_in_final => NoProtectedPropertyInFinal,
        functions::require_strict_array_search => RequireStrictArraySearch,
        style::no_escaped_quotes => NoEscapedQuotes,
        style::no_multi_assign => NoMultiAssign,
        style::require_statement_padding => RequireStatementPadding,
        style::require_assign_set_padding => RequireAssignSetPadding,
        style::prefer_php_version_id => PreferPhpVersionId,
        dead_code::no_bool_ternary => NoBoolTernary::default(),
        dead_code::no_useless_ternary => NoUselessTernary::default(),
        dead_code::no_false_ternary => NoFalseTernary,
        complexity::no_boolean_literal_compare => NoBooleanLiteralCompare::default(),
        complexity::prefer_empty_array_identical => PreferEmptyArrayIdentical::default(),
        complexity::prefer_min_max => PreferMinMax,
        phpdoc::no_void_on_magic_method => NoVoidOnMagicMethod,
        phpdoc::no_useless_readonly => NoUselessReadonly,
        functions::no_call_user_func_method => NoCallUserFuncMethod,
        phpdoc::no_mixed_overruled_by_native => NoMixedOverruledByNative,
        functions::no_required_after_optional => NoRequiredAfterOptional,
        complexity::no_negated_comparison => NoNegatedComparison::default(),
        complexity::prefer_coalesce_over_empty_ternary => PreferCoalesceOverEmptyTernary,
        control_flow::no_for_init_overwrite => NoForInitOverwrite,
        operators::no_loose_equality => NoLooseEquality,
        security::no_debug_calls => NoDebugCalls,
        complexity::require_named_argument_order => RequireNamedArgumentOrder,
        complexity::require_attribute_argument_order => RequireAttributeArgumentOrder,
        control_flow::no_loop_variable_overwrite => NoLoopVariableOverwrite,
        dead_code::no_unused_private_constant => NoUnusedPrivateConstant,
        style::prefer_variadic_param => PreferVariadicParam,
        style::prefer_spread => PreferSpread,
        classes::no_static_constructor_call => NoStaticConstructorCall,
        dead_code::no_null_tag_value => NoNullTagValue,
        functions::require_strict_function_call => RequireStrictFunctionCall,
        style::prefer_catch_name_from_type => PreferCatchNameFromType,
        dead_code::no_parent_call_without_parent => NoParentCallWithoutParent,
        dead_code::no_null_default_argument => NoNullDefaultArgument::default(),
        dead_code::no_null_default_argument => NoNullDefaultNamedArgument::default(),
        dead_code::no_unused_constructor_param => NoUnusedConstructorParam,
        dead_code::no_dead_catch => NoDeadCatch,
        types::add_datetime_property_type => AddDatetimePropertyType,
        style::prefer_class_keyword => PreferClassKeyword::default(),
        arrays::no_implicit_array_creation => NoImplicitArrayCreation,
        style::prefer_class_constant => PreferClassConstant::default(),
        style::require_parent_visibility => RequireParentVisibility,
        dead_code::no_unused_promoted_property => NoUnusedPromotedProperty,
        complexity::no_boolean_flag_parameter => NoBooleanFlagParameter::default(),
        complexity::no_else_clause => NoElseClause,
        complexity::no_empty_file => NoEmptyFile,
        complexity::no_excessive_cyclomatic => NoExcessiveCyclomatic::default(),
        complexity::no_excessive_members => NoExcessiveEnumCases::default(),
        complexity::no_excessive_halstead => NoExcessiveHalstead::default(),
        complexity::no_excessive_kan_defect => NoExcessiveKanDefect::default(),
        complexity::no_excessive_members => NoExcessiveMethods::default(),
        complexity::no_excessive_nesting => NoExcessiveNesting::default(),
        complexity::no_excessive_parameters => NoExcessiveParameters::default(),
        complexity::no_excessive_members => NoExcessiveProperties::default(),
        complexity::no_goto => NoGoto,
        complexity::prefer_named_literal_argument => PreferNamedLiteralArgument::default(),
        functions::require_assert_description => RequireAssertDescription,
        style::function_calls => NoIniSet,
        style::function_calls => NoSprintfConcat,
        style::function_calls => NoAmbiguousFunction,
        style::function_calls => NoAmbiguousConstant,
        style::function_calls => PreferCreateFromTimestamp,
        style::comments => RequireTodoReference,
        style::comments => NoInvalidPhpdoc,
        style::constructs => NoInlineHtml,
        style::constructs => NoIsset::default(),
        style::constructs => NoMultipleClassesPerFile,
        style::constructs => PreferWhileLoop,
        style::constructs => RequireNamespace,
        style::constructs => RequireSortedIntegerKeys,
        security::restricted => NoRestrictedFunctions::default(),
        security::restricted => NoRestrictedInstantiation::default(),
        strings::require_interpolation_style => RequireInterpolationStyle::default(),
        style::no_side_effects_with_declarations => NoSideEffectsWithDeclarations::default(),
        style::require_naming_convention => RequireNamingConvention::default(),
        style::require_phpdoc => RequirePhpdoc::default(),
        dead_code::no_empty_method => NoEmptyMethod,
        dead_code::no_unused_private_method => NoUnusedPrivateMethod,
        dead_code::no_useless_override => NoUselessOverride,
        dead_code::no_redundant_default_argument => NoRedundantDefaultArgument::default(),
        style::no_dynamic_member => NoDynamicMethodCall,
        style::no_dynamic_member => NoDynamicMethodCallable,
        style::no_dynamic_member => NoDynamicProperty,
        style::no_dynamic_member => NoDynamicStaticCall,
        style::no_dynamic_member => NoDynamicStaticCallable,
        style::no_dynamic_member => NoDynamicStaticProperty,
        style::prefer_class_name_constant => PreferClassNameConstant::default(),
        functions::require_strict_in_array => RequireStrictInArray,
        functions::no_extra_arguments => NoExtraArguments,
        functions::require_array_filter_callback => RequireArrayFilterCallback,
        complexity::no_constant_condition => NoConstantCondition,
        complexity::prefer_instance_method => PreferInstanceMethod,
        dead_code::no_unused_test_method_param => NoUnusedTestMethodParam::default(),
        complexity::require_previous_exception => RequirePreviousException,
        dead_code::no_useless_filter_var => NoUselessFilterVar,
        dead_code::no_useless_math => NoUselessMath,
        dead_code::no_useless_cast => NoUselessCast,
        dead_code::no_always_true_instanceof => NoAlwaysTrueInstanceof::default(),
        dead_code::no_typed_property_instanceof => NoTypedPropertyInstanceof,
        dead_code::no_redundant_foreach_guard => NoRedundantForeachGuard,
        dead_code::no_always_false_condition => NoAlwaysFalseCondition,
        dead_code::no_always_true_if => NoAlwaysTrueIf,
        dead_code::no_always_true_instanceof_assert => NoAlwaysTrueInstanceofAssert::default(),
        complexity::no_constant_via_variable => NoConstantViaVariable,
        complexity::require_consistent_return => RequireConsistentReturn,
        complexity::no_phpdoc_class_case_mismatch => NoPhpdocClassCaseMismatch,
        complexity::prefer_property_exists => PreferPropertyExists,
        dead_code::no_invalid_var_tag => NoInvalidVarTag,
        complexity::prefer_inline_array_return => PreferInlineArrayReturn,
        phpdoc::no_useless_var_tag => NoUselessVarTag,
        phpdoc::no_useless_param_tag => NoUselessParamTag,
        phpdoc::no_useless_return_tag => NoUselessReturnTag,
        dead_code::no_invalid_return_tag => NoInvalidReturnTag,
        dead_code::no_useless_phpdoc_type => NoUselessPhpdocType,
        dead_code::no_unused_return_type => NoUnusedReturnType::default(),
        complexity::no_redundant_nullable_check => NoRedundantNullableCheck::default(),
        dead_code::no_unused_assignment => NoUnusedAssignment::default(),
        operators::prefer_coalesce_assign_over_fallback => PreferCoalesceAssignOverFallback,
        dead_code::no_unused_return_value => NoUnusedReturnValue,
    }
}
