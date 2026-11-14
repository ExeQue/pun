mod arrays;
mod classes;
mod complexity;
mod control_flow;
mod dead_code;
mod functions;
mod laravel;
mod operators;
mod pest;
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

pub use arrays::no_implicit_array_creation::NoImplicitArrayCreation;
pub use arrays::prefer_array_append::PreferArrayAppend;
pub use classes::no_explicit_constructor_call::NoExplicitConstructorCall;
pub use classes::no_null_property_init::NoNullPropertyInit;
pub use classes::no_protected_constant_in_final::NoProtectedConstantInFinal;
pub use classes::no_protected_method_in_final::NoProtectedMethodInFinal;
pub use classes::no_protected_property_in_final::NoProtectedPropertyInFinal;
pub use classes::no_redundant_readonly_property::NoRedundantReadonlyProperty;
pub use classes::no_static_constructor_call::NoStaticConstructorCall;
pub use classes::no_underscore_class::NoUnderscoreClass;
pub use classes::prefer_new_self::PreferNewSelf;
pub use classes::prefer_self_over_static::PreferSelfOverStatic;
pub use classes::prefer_serialize_method::PreferSerializeMethod;
pub use classes::prefer_unserialize_method::PreferUnserializeMethod;
pub use complexity::no_and_assignment::NoAndAssignment;
pub use complexity::no_boolean_flag_parameter::NoBooleanFlagParameter;
pub use complexity::no_boolean_literal_compare::NoBooleanLiteralCompare;
pub use complexity::no_compact::NoCompact;
pub use complexity::no_concat_assign_new_item::NoConcatAssignNewItem;
pub use complexity::no_constant_condition::NoConstantCondition;
pub use complexity::no_constant_via_variable::NoConstantViaVariable;
pub use complexity::no_dirname_dir_concat::NoDirnameDirConcat;
pub use complexity::no_duplicate_match_arm::NoDuplicateMatchArm;
pub use complexity::no_else_clause::NoElseClause;
pub use complexity::no_empty_file::NoEmptyFile;
pub use complexity::no_excessive_cyclomatic::NoExcessiveCyclomatic;
pub use complexity::no_excessive_halstead::NoExcessiveHalstead;
pub use complexity::no_excessive_kan_defect::NoExcessiveKanDefect;
pub use complexity::no_excessive_members::{
    NoExcessiveEnumCases, NoExcessiveMethods, NoExcessiveProperties,
};
pub use complexity::no_excessive_nesting::NoExcessiveNesting;
pub use complexity::no_excessive_parameters::NoExcessiveParameters;
pub use complexity::no_goto::NoGoto;
pub use complexity::no_iterator_to_array_in_foreach::NoIteratorToArrayInForeach;
pub use complexity::no_negated_and::NoNegatedAnd;
pub use complexity::no_negated_boolean_literal::NoNegatedBooleanLiteral;
pub use complexity::no_negated_comparison::NoNegatedComparison;
pub use complexity::no_negated_group::NoNegatedGroup;
pub use complexity::no_negated_instanceof_ternary::NoNegatedInstanceofTernary;
pub use complexity::no_phpdoc_class_case_mismatch::NoPhpdocClassCaseMismatch;
pub use complexity::no_redundant_null_check::NoRedundantNullCheck;
pub use complexity::no_redundant_nullable_check::NoRedundantNullableCheck;
pub use complexity::no_redundant_write_visibility::NoRedundantWriteVisibility;
pub use complexity::no_single_case_switch::NoSingleCaseSwitch;
pub use complexity::no_single_item_in_array::NoSingleItemInArray;
pub use complexity::no_sole_value_sprintf::NoSoleValueSprintf;
pub use complexity::no_unused_label::NoUnusedLabel;
pub use complexity::no_useless_array_values::NoUselessArrayValues;
pub use complexity::prefer_absolute_include::PreferAbsoluteInclude;
pub use complexity::prefer_array_literal::PreferArrayLiteral;
pub use complexity::prefer_coalesce_chain::PreferCoalesceChain;
pub use complexity::prefer_coalesce_over_empty_ternary::PreferCoalesceOverEmptyTernary;
pub use complexity::prefer_coalesce_over_foreach::PreferCoalesceOverForeach;
pub use complexity::prefer_empty_array_compare::PreferEmptyArrayCompare;
pub use complexity::prefer_empty_array_identical::PreferEmptyArrayIdentical;
pub use complexity::prefer_explicit_array_comparison::PreferExplicitArrayComparison;
pub use complexity::prefer_func_num_args::PreferFuncNumArgs;
pub use complexity::prefer_if_statement::PreferIfStatement;
pub use complexity::prefer_in_array::PreferInArray;
pub use complexity::prefer_in_array_over_foreach::PreferInArrayOverForeach;
pub use complexity::prefer_in_array_over_search::PreferInArrayOverSearch;
pub use complexity::prefer_inline_array_return::PreferInlineArrayReturn;
pub use complexity::prefer_instance_method::PreferInstanceMethod;
pub use complexity::prefer_min_max::PreferMinMax;
pub use complexity::prefer_named_literal_argument::PreferNamedLiteralArgument;
pub use complexity::prefer_not_in_array::PreferNotInArray;
pub use complexity::prefer_null_coalesce::PreferNullCoalesce;
pub use complexity::prefer_property_default::PreferPropertyDefault;
pub use complexity::prefer_property_exists::PreferPropertyExists;
pub use complexity::prefer_simple_condition::PreferSimpleCondition;
pub use complexity::require_attribute_argument_order::RequireAttributeArgumentOrder;
pub use complexity::require_consistent_return::RequireConsistentReturn;
pub use complexity::require_is_a_allow_string::RequireIsAAllowString;
pub use complexity::require_named_argument_order::RequireNamedArgumentOrder;
pub use complexity::require_previous_exception::RequirePreviousException;
pub use control_flow::no_assign_in_condition::NoAssignInCondition;
pub use control_flow::no_else_after_return::NoElseAfterReturn;
pub use control_flow::no_empty_catch::NoEmptyCatch;
pub use control_flow::no_for_init_overwrite::NoForInitOverwrite;
pub use control_flow::no_loop_variable_overwrite::NoLoopVariableOverwrite;
pub use control_flow::no_unsafe_finally::NoUnsafeFinally;
pub use control_flow::prefer_returned_condition::PreferReturnedCondition;
pub use dead_code::no_always_false_condition::NoAlwaysFalseCondition;
pub use dead_code::no_always_true_if::NoAlwaysTrueIf;
pub use dead_code::no_always_true_instanceof::NoAlwaysTrueInstanceof;
pub use dead_code::no_always_true_instanceof_assert::NoAlwaysTrueInstanceofAssert;
pub use dead_code::no_and_true::NoAndTrue;
pub use dead_code::no_bool_ternary::NoBoolTernary;
pub use dead_code::no_cast_in_concat::NoCastInConcat;
pub use dead_code::no_condition_exact_return::NoConditionExactReturn;
pub use dead_code::no_dead_catch::NoDeadCatch;
pub use dead_code::no_dead_php_version_check::NoDeadPhpVersionCheck;
pub use dead_code::no_double_self_assignment::NoDoubleSelfAssignment;
pub use dead_code::no_duplicate_array_key::NoDuplicateArrayKey;
pub use dead_code::no_duplicate_case_body::NoDuplicateCaseBody;
pub use dead_code::no_empty_block::NoEmptyBlock;
pub use dead_code::no_empty_loop::NoEmptyLoop;
pub use dead_code::no_empty_method::NoEmptyMethod;
pub use dead_code::no_false_ternary::NoFalseTernary;
pub use dead_code::no_future_php_version_check::NoFuturePhpVersionCheck;
pub use dead_code::no_identical_branches::NoIdenticalBranches;
pub use dead_code::no_implode_ternary::NoImplodeTernary;
pub use dead_code::no_invalid_return_tag::NoInvalidReturnTag;
pub use dead_code::no_invalid_var_tag::NoInvalidVarTag;
pub use dead_code::no_mirror_assignment::NoMirrorAssignment;
pub use dead_code::no_null_default_argument::{NoNullDefaultArgument, NoNullDefaultNamedArgument};
pub use dead_code::no_null_tag_value::NoNullTagValue;
pub use dead_code::no_overridden_assign_before_if::NoOverriddenAssignBeforeIf;
pub use dead_code::no_overwritten_assignment::NoOverwrittenAssignment;
pub use dead_code::no_parent_call_without_parent::NoParentCallWithoutParent;
pub use dead_code::no_redundant_condition::NoRedundantCondition;
pub use dead_code::no_redundant_default_argument::NoRedundantDefaultArgument;
pub use dead_code::no_redundant_foreach_guard::NoRedundantForeachGuard;
pub use dead_code::no_redundant_promoted_assign::NoRedundantPromotedAssign;
pub use dead_code::no_redundant_property_default::NoRedundantPropertyDefault;
pub use dead_code::no_redundant_type_check::NoRedundantTypeCheck;
pub use dead_code::no_return_in_constructor::NoReturnInConstructor;
pub use dead_code::no_tautology_ternary::NoTautologyTernary;
pub use dead_code::no_typed_property_instanceof::NoTypedPropertyInstanceof;
pub use dead_code::no_unreachable_code::NoUnreachableCode;
pub use dead_code::no_unused_assignment::NoUnusedAssignment;
pub use dead_code::no_unused_constructor_param::NoUnusedConstructorParam;
pub use dead_code::no_unused_expression::NoUnusedExpression;
pub use dead_code::no_unused_foreach_key::NoUnusedForeachKey;
pub use dead_code::no_unused_global::NoUnusedGlobal;
pub use dead_code::no_unused_private_constant::NoUnusedPrivateConstant;
pub use dead_code::no_unused_private_method::NoUnusedPrivateMethod;
pub use dead_code::no_unused_private_method_param::NoUnusedPrivateMethodParam;
pub use dead_code::no_unused_private_property::NoUnusedPrivateProperty;
pub use dead_code::no_unused_promoted_property::NoUnusedPromotedProperty;
pub use dead_code::no_unused_public_method_param::NoUnusedPublicMethodParam;
pub use dead_code::no_unused_return_type::NoUnusedReturnType;
pub use dead_code::no_unused_static::NoUnusedStatic;
pub use dead_code::no_unused_test_method_param::NoUnusedTestMethodParam;
pub use dead_code::no_useless_cast::NoUselessCast;
pub use dead_code::no_useless_catch::NoUselessCatch;
pub use dead_code::no_useless_condition::NoUselessCondition;
pub use dead_code::no_useless_continue::NoUselessContinue;
pub use dead_code::no_useless_filter_var::NoUselessFilterVar;
pub use dead_code::no_useless_math::NoUselessMath;
pub use dead_code::no_useless_override::NoUselessOverride;
pub use dead_code::no_useless_phpdoc_type::NoUselessPhpdocType;
pub use dead_code::no_useless_ternary::NoUselessTernary;
pub use dead_code::no_useless_variable::NoUselessVariable;
pub use functions::no_assign_in_argument::NoAssignInArgument;
pub use functions::no_call_user_func_method::NoCallUserFuncMethod;
pub use functions::no_extra_arguments::NoExtraArguments;
pub use functions::no_missing_format_argument::NoMissingFormatArgument;
pub use functions::no_required_after_optional::NoRequiredAfterOptional;
pub use functions::no_swapped_explode_arguments::NoSwappedExplodeArguments;
pub use functions::no_unused_closure_variable::NoUnusedClosureVariable;
pub use functions::no_void_reference_return::NoVoidReferenceReturn;
pub use functions::require_array_filter_callback::RequireArrayFilterCallback;
pub use functions::require_assert_description::RequireAssertDescription;
pub use functions::require_strict_array_search::RequireStrictArraySearch;
pub use functions::require_strict_function_call::RequireStrictFunctionCall;
pub use functions::require_strict_in_array::RequireStrictInArray;
pub use laravel::*;
pub use operators::no_loose_equality::NoLooseEquality;
pub use operators::prefer_boolean_and_or::PreferBooleanAndOr;
pub use pest::*;
pub use php_8_4::add_csv_escape_argument::AddCsvEscapeArgument;
pub use php_8_4::prefer_array_all::PreferArrayAll;
pub use php_8_4::prefer_array_any::PreferArrayAny;
pub use php_8_4::prefer_array_find::PreferArrayFind;
pub use php_8_4::prefer_array_find_key::PreferArrayFindKey;
pub use php_8_4::prefer_round_mode_enum::PreferRoundModeEnum;
pub use php_8_5::no_finfo_buffer_context::NoFinfoBufferContext;
pub use php_8_5::no_null_array_key::NoNullArrayKey;
pub use php_8_5::no_null_debug_info::NoNullDebugInfo;
pub use php_8_5::no_out_of_range_chr::NoOutOfRangeChr;
pub use php_8_5::prefer_array_first_last::PreferArrayFirstLast;
pub use php_8_5::prefer_ord_first_byte::PreferOrdFirstByte;
pub use php_8_6::prefer_clamp::PreferClamp;
pub use phpdoc::no_mixed_overruled_by_native::NoMixedOverruledByNative;
pub use phpdoc::no_useless_param_tag::NoUselessParamTag;
pub use phpdoc::no_useless_readonly::NoUselessReadonly;
pub use phpdoc::no_useless_return_tag::NoUselessReturnTag;
pub use phpdoc::no_useless_var_tag::NoUselessVarTag;
pub use phpdoc::no_void_on_magic_method::NoVoidOnMagicMethod;
pub use phpunit::*;
pub use refactor::no_negated_condition::NoNegatedCondition;
pub use refactor::*;
pub use security::no_debug_calls::NoDebugCalls;
pub use security::no_eval::NoEval;
pub use security::no_ffi::NoFfi;
pub use security::no_global::NoGlobal;
pub use security::no_insecure_comparison::NoInsecureComparison;
pub use security::no_literal_password::NoLiteralPassword;
pub use security::no_request_variable::NoRequestVariable;
pub use security::no_tainted_data::NoTaintedData;
pub use security::require_preg_quote_delimiter::RequirePregQuoteDelimiter;
pub use security::restricted::{NoRestrictedFunctions, NoRestrictedInstantiation};
pub use statements::add_safe_strict_types::AddSafeStrictTypes;
pub use strings::no_single_argument_sprintf::NoSingleArgumentSprintf;
pub use strings::prefer_empty_string_identical::PreferEmptyStringIdentical;
pub use strings::require_interpolation_style::RequireInterpolationStyle;
pub use style::comments::{NoInvalidPhpdoc, RequireTodoReference};
pub use style::constructs::{
    NoInlineHtml, NoIsset, NoMultipleClassesPerFile, PreferWhileLoop, RequireNamespace,
    RequireSortedIntegerKeys,
};
pub use style::function_calls::{
    NoAmbiguousConstant, NoAmbiguousFunction, NoIniSet, NoSprintfConcat, PreferCreateFromTimestamp,
};
pub use style::no_array_accumulation_in_loop::NoArrayAccumulationInLoop;
pub use style::no_assign_in_ternary_branch::NoAssignInTernaryBranch;
pub use style::no_dynamic_member::{
    NoDynamicMethodCall, NoDynamicMethodCallable, NoDynamicProperty, NoDynamicStaticCall,
    NoDynamicStaticCallable, NoDynamicStaticProperty,
};
pub use style::no_dynamic_name::NoDynamicName;
pub use style::no_empty::NoEmpty;
pub use style::no_escaped_quotes::NoEscapedQuotes;
pub use style::no_hash_emoji::NoHashEmoji;
pub use style::no_multi_assign::NoMultiAssign;
pub use style::no_nested_ternary::NoNestedTernary;
pub use style::no_parameter_shadowing::NoParameterShadowing;
pub use style::no_redundant_final_const::NoRedundantFinalConst;
pub use style::no_short_ternary::NoShortTernary;
pub use style::no_side_effects_with_declarations::NoSideEffectsWithDeclarations;
pub use style::no_unreachable_loop::NoUnreachableLoop;
pub use style::no_variable_variable::NoVariableVariable;
pub use style::prefer_catch_name_from_type::PreferCatchNameFromType;
pub use style::prefer_class_constant::PreferClassConstant;
pub use style::prefer_class_keyword::PreferClassKeyword;
pub use style::prefer_class_name_constant::PreferClassNameConstant;
pub use style::prefer_compound_assignment::PreferCompoundAssignment;
pub use style::prefer_early_continue::PreferEarlyContinue;
pub use style::prefer_early_return::PreferEarlyReturn;
pub use style::prefer_early_return_over_prepared_value::PreferEarlyReturnOverPreparedValue;
pub use style::prefer_instanceof_stringable::PreferInstanceofStringable;
pub use style::prefer_php_version_id::PreferPhpVersionId;
pub use style::prefer_property_over_local_getter::PreferPropertyOverLocalGetter;
pub use style::prefer_spread::PreferSpread;
pub use style::prefer_variadic_param::PreferVariadicParam;
pub use style::require_assign_set_padding::RequireAssignSetPadding;
pub use style::require_naming_convention::RequireNamingConvention;
pub use style::require_parent_visibility::RequireParentVisibility;
pub use style::require_phpdoc::RequirePhpdoc;
pub use style::require_statement_padding::RequireStatementPadding;
pub use tags::no_invalid_open_tag::NoInvalidOpenTag;
pub use types::add_datetime_property_type::AddDatetimePropertyType;
pub use types::*;

use crate::Rule;

pub fn all() -> Vec<Box<dyn Rule>> {
    let mut rules = laravel::all();
    rules.extend(types::all());
    rules.extend(pest::all());
    rules.extend(phpunit::all());
    rules.extend(refactor::all());
    rules.extend(ungrouped());
    rules
}

fn ungrouped() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(PreferCompoundAssignment),
        Box::new(NoAndAssignment),
        Box::new(PreferNullCoalesce),
        Box::new(PreferArrayAppend),
        Box::new(NoNegatedAnd),
        Box::new(NoConcatAssignNewItem),
        Box::new(NoNegatedBooleanLiteral),
        Box::new(NoNegatedGroup),
        Box::new(NoNegatedInstanceofTernary),
        Box::new(NoDirnameDirConcat),
        Box::new(PreferExplicitArrayComparison),
        Box::new(PreferIfStatement),
        Box::new(PreferEmptyArrayCompare),
        Box::new(PreferFuncNumArgs),
        Box::new(NoUselessArrayValues),
        Box::new(NoTautologyTernary),
        Box::new(NoSingleItemInArray),
        Box::new(NoSingleArgumentSprintf),
        Box::new(NoAndTrue),
        Box::new(NoCastInConcat),
        Box::new(NoUselessContinue),
        Box::new(NoEmptyBlock),
        Box::new(NoUselessCatch),
        Box::new(NoDoubleSelfAssignment),
        Box::new(NoDuplicateArrayKey),
        Box::new(NoRedundantFinalConst),
        Box::new(NoNullPropertyInit),
        Box::new(NoUnusedClosureVariable),
        Box::new(NoUnusedForeachKey),
        Box::new(NoReturnInConstructor),
        Box::new(NoUnreachableCode),
        Box::new(NoMirrorAssignment),
        Box::new(NoRedundantPromotedAssign),
        Box::new(NoDuplicateCaseBody),
        Box::new(NoIdenticalBranches),
        Box::new(NoElseAfterReturn),
        Box::new(PreferEarlyReturn),
        Box::new(PreferEarlyReturnOverPreparedValue),
        Box::new(PreferInArray),
        Box::new(PreferNotInArray),
        Box::new(NoRedundantNullCheck),
        Box::new(PreferSimpleCondition),
        Box::new(PreferInArrayOverSearch),
        Box::new(NoAssignInTernaryBranch),
        Box::new(NoSingleCaseSwitch),
        Box::new(NoImplodeTernary),
        Box::new(PreferNewSelf),
        Box::new(PreferArrayLiteral),
        Box::new(NoRedundantReadonlyProperty),
        Box::new(PreferAbsoluteInclude),
        Box::new(PreferBooleanAndOr),
        Box::new(NoFuturePhpVersionCheck),
        Box::new(NoDeadPhpVersionCheck),
        Box::new(NoUnusedPublicMethodParam),
        Box::new(NoUnusedPrivateMethodParam),
        Box::new(NoUnusedPrivateProperty),
        Box::new(NoRedundantPropertyDefault),
        Box::new(PreferPropertyDefault),
        Box::new(PreferInArrayOverForeach),
        Box::new(PreferCoalesceChain),
        Box::new(PreferCoalesceOverForeach),
        Box::new(PreferPropertyOverLocalGetter),
        Box::new(NoEmptyCatch),
        Box::new(NoUnsafeFinally),
        Box::new(NoAssignInCondition),
        Box::new(NoUnusedLabel),
        Box::new(NoRedundantWriteVisibility),
        Box::new(NoIteratorToArrayInForeach),
        Box::new(NoInvalidOpenTag),
        Box::new(NoNegatedCondition),
        Box::new(NoUnusedGlobal),
        Box::new(NoUnusedStatic),
        Box::new(NoDuplicateMatchArm),
        Box::new(NoUselessCondition),
        Box::new(NoConditionExactReturn),
        Box::new(NoOverriddenAssignBeforeIf),
        Box::new(NoRedundantTypeCheck),
        Box::new(NoOverwrittenAssignment),
        Box::new(NoRedundantCondition),
        Box::new(NoEmptyLoop),
        Box::new(NoUnusedExpression),
        Box::new(NoSoleValueSprintf),
        Box::new(NoCompact),
        Box::new(RequireIsAAllowString),
        Box::new(AddCsvEscapeArgument),
        Box::new(PreferRoundModeEnum),
        Box::new(NoFinfoBufferContext),
        Box::new(NoNullDebugInfo),
        Box::new(NoNullArrayKey),
        Box::new(AddSafeStrictTypes),
        Box::new(NoOutOfRangeChr),
        Box::new(PreferOrdFirstByte),
        Box::new(PreferArrayFirstLast),
        Box::new(PreferClamp),
        Box::new(PreferArrayAll),
        Box::new(PreferArrayAny),
        Box::new(PreferArrayFind),
        Box::new(PreferArrayFindKey),
        Box::new(NoEval),
        Box::new(NoGlobal),
        Box::new(NoRequestVariable),
        Box::new(NoDynamicName),
        Box::new(NoUnderscoreClass),
        Box::new(NoVoidReferenceReturn),
        Box::new(NoNestedTernary),
        Box::new(NoEmpty),
        Box::new(NoShortTernary),
        Box::new(NoVariableVariable),
        Box::new(NoExplicitConstructorCall),
        Box::new(NoAssignInArgument),
        Box::new(NoSwappedExplodeArguments),
        Box::new(RequirePregQuoteDelimiter),
        Box::new(NoFfi),
        Box::new(NoInsecureComparison),
        Box::new(NoLiteralPassword),
        Box::new(NoHashEmoji),
        Box::new(NoMissingFormatArgument),
        Box::new(NoParameterShadowing),
        Box::new(NoUnreachableLoop),
        Box::new(PreferInstanceofStringable),
        Box::new(NoArrayAccumulationInLoop),
        Box::new(PreferEarlyContinue),
        Box::new(NoTaintedData),
        Box::new(NoProtectedConstantInFinal),
        Box::new(NoProtectedMethodInFinal),
        Box::new(NoProtectedPropertyInFinal),
        Box::new(PreferSelfOverStatic),
        Box::new(RequireStrictArraySearch),
        Box::new(NoEscapedQuotes),
        Box::new(NoMultiAssign),
        Box::new(RequireStatementPadding),
        Box::new(RequireAssignSetPadding),
        Box::new(PreferPhpVersionId),
        Box::new(NoBoolTernary),
        Box::new(NoUselessTernary),
        Box::new(NoFalseTernary),
        Box::new(PreferEmptyStringIdentical),
        Box::new(NoBooleanLiteralCompare),
        Box::new(PreferEmptyArrayIdentical),
        Box::new(PreferMinMax),
        Box::new(PreferSerializeMethod),
        Box::new(PreferUnserializeMethod),
        Box::new(NoVoidOnMagicMethod),
        Box::new(NoUselessReadonly),
        Box::new(NoCallUserFuncMethod),
        Box::new(NoMixedOverruledByNative),
        Box::new(NoRequiredAfterOptional),
        Box::new(NoNegatedComparison),
        Box::new(PreferCoalesceOverEmptyTernary),
        Box::new(NoForInitOverwrite),
        Box::new(NoLooseEquality),
        Box::new(NoDebugCalls),
        Box::new(RequireNamedArgumentOrder),
        Box::new(RequireAttributeArgumentOrder),
        Box::new(NoLoopVariableOverwrite),
        Box::new(NoUnusedPrivateConstant),
        Box::new(PreferVariadicParam),
        Box::new(PreferSpread),
        Box::new(NoStaticConstructorCall),
        Box::new(NoNullTagValue),
        Box::new(RequireStrictFunctionCall),
        Box::new(PreferCatchNameFromType),
        Box::new(NoParentCallWithoutParent),
        Box::new(NoUselessVariable),
        Box::new(NoNullDefaultArgument),
        Box::new(NoNullDefaultNamedArgument),
        Box::new(NoUnusedConstructorParam),
        Box::new(NoDeadCatch),
        Box::new(AddDatetimePropertyType),
        Box::new(PreferClassKeyword),
        Box::new(NoImplicitArrayCreation),
        Box::new(PreferClassConstant),
        Box::new(RequireParentVisibility),
        Box::new(NoUnusedPromotedProperty),
        Box::new(NoBooleanFlagParameter::default()),
        Box::new(NoElseClause),
        Box::new(NoEmptyFile),
        Box::new(NoExcessiveCyclomatic::default()),
        Box::new(NoExcessiveEnumCases::default()),
        Box::new(NoExcessiveHalstead::default()),
        Box::new(NoExcessiveKanDefect::default()),
        Box::new(NoExcessiveMethods::default()),
        Box::new(NoExcessiveNesting::default()),
        Box::new(NoExcessiveParameters::default()),
        Box::new(NoExcessiveProperties::default()),
        Box::new(NoGoto),
        Box::new(PreferNamedLiteralArgument::default()),
        Box::new(RequireAssertDescription),
        Box::new(NoIniSet),
        Box::new(NoSprintfConcat),
        Box::new(NoAmbiguousFunction),
        Box::new(NoAmbiguousConstant),
        Box::new(PreferCreateFromTimestamp),
        Box::new(RequireTodoReference),
        Box::new(NoInvalidPhpdoc),
        Box::new(NoInlineHtml),
        Box::new(NoIsset::default()),
        Box::new(NoMultipleClassesPerFile),
        Box::new(PreferWhileLoop),
        Box::new(RequireNamespace),
        Box::new(RequireSortedIntegerKeys),
        Box::new(NoRestrictedFunctions::default()),
        Box::new(NoRestrictedInstantiation::default()),
        Box::new(RequireInterpolationStyle::default()),
        Box::new(NoSideEffectsWithDeclarations::default()),
        Box::new(RequireNamingConvention::default()),
        Box::new(RequirePhpdoc::default()),
        Box::new(NoEmptyMethod),
        Box::new(NoUnusedPrivateMethod),
        Box::new(NoUselessOverride),
        Box::new(NoRedundantDefaultArgument),
        Box::new(NoDynamicMethodCall),
        Box::new(NoDynamicMethodCallable),
        Box::new(NoDynamicProperty),
        Box::new(NoDynamicStaticCall),
        Box::new(NoDynamicStaticCallable),
        Box::new(NoDynamicStaticProperty),
        Box::new(PreferClassNameConstant::default()),
        Box::new(RequireStrictInArray),
        Box::new(NoExtraArguments),
        Box::new(RequireArrayFilterCallback),
        Box::new(NoConstantCondition),
        Box::new(PreferInstanceMethod),
        Box::new(NoUnusedTestMethodParam),
        Box::new(RequirePreviousException),
        Box::new(NoUselessFilterVar),
        Box::new(NoUselessMath),
        Box::new(NoUselessCast),
        Box::new(NoAlwaysTrueInstanceof),
        Box::new(NoTypedPropertyInstanceof),
        Box::new(NoRedundantForeachGuard),
        Box::new(NoAlwaysFalseCondition),
        Box::new(NoAlwaysTrueIf),
        Box::new(NoAlwaysTrueInstanceofAssert),
        Box::new(NoConstantViaVariable),
        Box::new(RequireConsistentReturn),
        Box::new(NoPhpdocClassCaseMismatch),
        Box::new(PreferPropertyExists),
        Box::new(PreferReturnedCondition),
        Box::new(NoInvalidVarTag),
        Box::new(PreferInlineArrayReturn),
        Box::new(NoUselessVarTag),
        Box::new(NoUselessParamTag),
        Box::new(NoUselessReturnTag),
        Box::new(NoInvalidReturnTag),
        Box::new(NoUselessPhpdocType),
        Box::new(NoUnusedReturnType::default()),
        Box::new(NoRedundantNullableCheck),
        Box::new(NoUnusedAssignment),
    ]
}
