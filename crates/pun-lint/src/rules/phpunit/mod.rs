mod add_allow_mock_objects_attribute;
mod add_closure_return_type_from_assert;
mod add_data_provider_allow_mock_objects;
mod add_data_provider_iterator_return_type;
mod add_depended_test_return_type;
mod add_instanceof_assert;
mod add_mock_intersection_param;
mod add_mock_intersection_type;
mod add_mock_object_type;
mod add_param_type_depends;
mod add_return_type_from_mock_object;
mod add_stub_intersection_type;
mod add_test_void_return;
mod add_will_return_callback_false_fallback;
mod add_will_return_callback_param_type;
pub(crate) mod annotations;
mod assert_calls;
mod no_any_matcher;
mod no_assert_contains_only;
mod no_assert_is_type;
mod no_assert_same_type_mismatch;
mod no_at_matcher;
mod no_bare_mock_assign;
mod no_bare_mock_builder;
mod no_empty_test;
mod no_entity_mock;
mod no_expects_any;
mod no_final_constructor_override;
mod no_flipped_assertion;
mod no_float_delta_parameter;
mod no_mock_intersection_property;
mod no_negated_assertion;
mod no_setup_override_attribute;
mod no_stub_with_expectations;
mod no_test_constructor;
mod no_unused_mock;
mod no_unused_mock_property;
mod no_value_object_mock;
pub(crate) mod phpdoc_type;
mod prefer_assert;
mod prefer_assert_array_has_key;
mod prefer_assert_count;
mod prefer_assert_instanceof;
mod prefer_assert_instanceof_over_empty;
mod prefer_assert_instances_of;
mod prefer_assert_null_true_false;
mod prefer_assert_string_contains;
mod prefer_assert_true_false;
mod prefer_attribute_over_annotation;
mod prefer_comparison_assertion;
mod prefer_covers_attribute;
mod prefer_create_stub;
mod prefer_create_stub_argument;
mod prefer_create_stub_expression;
mod prefer_create_stub_in_coalesce;
mod prefer_data_provider;
mod prefer_data_provider_attribute;
mod prefer_depends_attribute;
mod prefer_inline_create_stub;
mod prefer_local_over_setup_property;
mod prefer_merged_will_return_callback;
mod prefer_mock_intersection_return;
mod prefer_named_data_provider_keys;
mod prefer_narrow_will_return_callback;
mod prefer_narrow_with_consecutive;
mod prefer_property_assertions;
mod prefer_requires_attribute;
mod prefer_simple_callback_assert;
mod prefer_simple_mock_expectation;
mod prefer_simple_with_matcher;
mod prefer_specific_true_false_assertion;
mod prefer_specific_will_method;
mod prefer_standalone_asserts_over_callback;
mod prefer_stub_var;
mod prefer_test_attribute;
mod prefer_test_prefix;
mod prefer_test_with_attribute;
mod prefer_this_call;
mod prefer_ticket_attribute;
mod prefer_will_return_callback;
mod prefer_with_is_instance_of;
mod prefer_yield_data_provider;
mod ptype;
mod require_data_provider_newlines;
mod require_exact_test_argument_type;
mod require_final_test_case;
mod require_mock_expects;
pub(crate) mod require_strict_types_in_tests;
mod require_string_assert_argument;
mod test_methods;

pub(crate) use crate::rules::support::expressions::text;
pub(crate) use crate::rules::support::names::resolve_class_name as resolve;
pub(crate) use prefer_test_with_attribute::print_float;

use crate::problem::{Edit, Span};
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{
    Argument, ArgumentList, ClassLikeMemberSelector, Expression, Identifier, Method, Modifier,
    Node, Variable,
};
use std::fmt::Write as _;

use crate::Context;
use crate::rules::laravel::is_a;
use crate::rules::support::sequence_diff::{Elem, coalesce_replacements, diff};
use crate::rules::types::class_scope;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};

rules! {
    pub(super) fn all {
        no_empty_test => NoEmptyTest,
        no_negated_assertion => NoNegatedAssertion,
        prefer_assert_null_true_false => PreferAssertNullTrueFalse,
        prefer_assert_true_false => PreferAssertTrueFalse,
        prefer_assert_string_contains => PreferAssertStringContains,
        no_assert_is_type => NoAssertIsType,
        no_assert_contains_only => NoAssertContainsOnly,
        prefer_comparison_assertion => PreferComparisonAssertion,
        prefer_assert_instanceof => PreferAssertInstanceof,
        prefer_assert_instanceof_over_empty => PreferAssertInstanceofOverEmpty,
        prefer_property_assertions => PreferPropertyAssertions,
        add_closure_return_type_from_assert => AddClosureReturnTypeFromAssert,
        no_flipped_assertion => NoFlippedAssertion,
        no_float_delta_parameter => NoFloatDeltaParameter,
        prefer_specific_true_false_assertion => PreferSpecificTrueFalseAssertion,
        prefer_assert_array_has_key => PreferAssertArrayHasKey,
        require_string_assert_argument => RequireStringAssertArgument,
        prefer_assert_instances_of => PreferAssertInstancesOf,
        no_assert_same_type_mismatch => NoAssertSameTypeMismatch,
        prefer_assert_count => PreferAssertCount,
        no_expects_any => NoExpectsAny,
        prefer_simple_with_matcher => PreferSimpleWithMatcher,
        prefer_specific_will_method => PreferSpecificWillMethod,
        prefer_simple_mock_expectation => PreferSimpleMockExpectation,
        prefer_narrow_with_consecutive => PreferNarrowWithConsecutive,
        no_final_constructor_override => NoFinalConstructorOverride,
        no_setup_override_attribute => NoSetupOverrideAttribute,
        require_strict_types_in_tests => RequireStrictTypesInTests,
        prefer_this_call => PreferThisCall,
        no_unused_mock => NoUnusedMock,
        no_at_matcher => NoAtMatcher,
        no_bare_mock_builder => NoBareMockBuilder,
        prefer_create_stub_in_coalesce => PreferCreateStubInCoalesce,
        require_data_provider_newlines => RequireDataProviderNewlines,
        prefer_with_is_instance_of => PreferWithIsInstanceOf,
        prefer_simple_callback_assert => PreferSimpleCallbackAssert,
        prefer_narrow_will_return_callback => PreferNarrowWillReturnCallback,
        prefer_assert => PreferAssert,
        prefer_merged_will_return_callback => PreferMergedWillReturnCallback::default(),
        add_depended_test_return_type => AddDependedTestReturnType,
        no_test_constructor => NoTestConstructor,
        prefer_named_data_provider_keys => PreferNamedDataProviderKeys,
        prefer_test_prefix => PreferTestPrefix,
        no_value_object_mock => NoValueObjectMock,
        no_bare_mock_assign => NoBareMockAssign,
        prefer_standalone_asserts_over_callback => PreferStandaloneAssertsOverCallback,
        prefer_ticket_attribute => PreferTicketAttribute,
        prefer_data_provider_attribute => PreferDataProviderAttribute,
        prefer_requires_attribute => PreferRequiresAttribute,
        prefer_depends_attribute => PreferDependsAttribute,
        prefer_covers_attribute => PreferCoversAttribute,
        prefer_test_with_attribute => PreferTestWithAttribute,
        prefer_attribute_over_annotation => PreferAttributeOverAnnotation::default(),
        prefer_data_provider => PreferDataProvider,
        require_final_test_case => RequireFinalTestCase,
        add_test_void_return => AddTestVoidReturn,
        add_data_provider_iterator_return_type => AddDataProviderIteratorReturnType,
        add_return_type_from_mock_object => AddReturnTypeFromMockObject,
        prefer_create_stub_expression => PreferCreateStubExpression,
        no_mock_intersection_property => NoMockIntersectionProperty,
        no_unused_mock_property => NoUnusedMockProperty,
        prefer_create_stub => PreferCreateStub,
        require_mock_expects => RequireMockExpects,
        prefer_inline_create_stub => PreferInlineCreateStub,
        prefer_stub_var => PreferStubVar,
        prefer_mock_intersection_return => PreferMockIntersectionReturn,
        add_mock_intersection_type => AddMockIntersectionType,
        add_stub_intersection_type => AddStubIntersectionType,
        add_mock_object_type => AddMockObjectType,
        add_mock_intersection_param => AddMockIntersectionParam,
        add_allow_mock_objects_attribute => AddAllowMockObjectsAttribute,
        add_data_provider_allow_mock_objects => AddDataProviderAllowMockObjects,
        add_param_type_depends => AddParamTypeDepends,
        prefer_local_over_setup_property => PreferLocalOverSetupProperty,
        require_exact_test_argument_type => RequireExactTestArgumentType,
        prefer_test_attribute => PreferTestAttribute,
        no_entity_mock => NoEntityMock,
        prefer_yield_data_provider => PreferYieldDataProvider,
        no_stub_with_expectations => NoStubWithExpectations,
        prefer_create_stub_argument => PreferCreateStubArgument,
        add_will_return_callback_param_type => AddWillReturnCallbackParamType,
        prefer_will_return_callback => PreferWillReturnCallback::default(),
        add_instanceof_assert => AddInstanceofAssert,
        add_instanceof_assert => AddInstanceofAssertForArgument,
        no_any_matcher => NoAnyMatcher,
        add_will_return_callback_false_fallback => AddWillReturnCallbackFalseFallback,
        super::types::doctrine_repository => AddPropertyTypeContainerGet,
        super::types::add_property_type_create_mock => AddPropertyTypeCreateMock,
        super::types::add_property_type_setup => AddPropertyTypeSetup,
        super::types::add_property_type_setup_docblock => AddPropertyTypeSetupDocblock,
        assert_calls => PreferSelfCall,
        assert_calls => PreferThisAssert,
        assert_calls => PreferDirectAssert,
        test_methods => PreferCamelCaseTestNames,
        test_methods => PreferSnakeCaseTestNames,
        test_methods => RequireExplicitMockExpects,
        test_methods => RequireArrayKeyAssert,
        test_methods => AddSeeTestTag,
    }
}

pub(crate) const TEST_CASE: &str = "PHPUnit\\Framework\\TestCase";
pub(crate) const TEST_CASE_LEGACY: &str = "PHPUnit_Framework_TestCase";
pub(crate) const ASSERT: &str = "PHPUnit\\Framework\\Assert";

pub(crate) fn is_in_test_class(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    method: Option<&Method<'_>>,
) -> bool {
    let Some(at) = ancestors.iter().rposition(|a| {
        matches!(
            a,
            Node::Class(_)
                | Node::AnonymousClass(_)
                | Node::Interface(_)
                | Node::Trait(_)
                | Node::Enum(_)
        )
    }) else {
        return false;
    };
    if matches!(ancestors[at], Node::Trait(_)) {
        let name = class_scope(ancestors)
            .and_then(|s| s.name)
            .unwrap_or_default();
        let in_tests_namespace = name
            .rsplit_once('\\')
            .is_some_and(|(ns, _)| ns.split('\\').any(|part| part == "Test" || part == "Tests"));
        let Some(method) = method else {
            return in_tests_namespace;
        };
        if !is_public(method)
            || method
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Static(_)))
        {
            return false;
        }
        return in_tests_namespace || method.name.value.starts_with(b"test");
    }
    class_is(ctx, &ancestors[..=at], &[TEST_CASE, TEST_CASE_LEGACY])
}

pub(super) fn class_is(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], targets: &[&str]) -> bool {
    let class = ancestors.iter().rev().find(|a| {
        matches!(
            a,
            Node::Class(_)
                | Node::AnonymousClass(_)
                | Node::Interface(_)
                | Node::Trait(_)
                | Node::Enum(_)
        )
    });
    let key = (
        class.map_or(u32::MAX, |c| mago_span::HasSpan::span(c).start.offset),
        mago_span::HasSpan::span(ancestors.last().unwrap())
            .start
            .offset,
        targets.join("|"),
    );
    if let Some(&found) = ctx.class_is.borrow().get(&key) {
        return found;
    }
    let found = class_is_uncached(ctx, ancestors, targets);
    ctx.class_is.borrow_mut().insert(key, found);
    found
}

fn class_is_uncached(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], targets: &[&str]) -> bool {
    let scope = class_scope(ancestors).unwrap();
    if let Some(name) = &scope.name {
        return is_a(ctx, name, targets);
    }
    let span = mago_span::HasSpan::span(ancestors.last().unwrap());
    let name = facts(ctx).anonymous_class_name(span);
    is_a(ctx, &name, targets) || scope.parents.iter().any(|p| is_a(ctx, p, targets))
}

pub(crate) fn is_public(method: &Method<'_>) -> bool {
    !method
        .modifiers
        .iter()
        .any(|m| matches!(m, Modifier::Private(_) | Modifier::Protected(_)))
}

#[derive(Clone, Copy)]
pub(crate) struct AnyCall<'a> {
    pub span: Span,
    pub receiver: &'a Expression<'a>,
    pub is_static: bool,
    pub method: &'a ClassLikeMemberSelector<'a>,
    pub args: &'a ArgumentList<'a>,
}

impl<'a> AnyCall<'a> {
    pub(crate) fn of(node: Node<'a, 'a>) -> Option<AnyCall<'a>> {
        match node {
            Node::MethodCall(m) => Some(AnyCall {
                span: node_span(m),
                receiver: m.object,
                is_static: false,
                method: &m.method,
                args: &m.argument_list,
            }),
            Node::StaticMethodCall(s) => Some(AnyCall {
                span: node_span(s),
                receiver: s.class,
                is_static: true,
                method: &s.method,
                args: &s.argument_list,
            }),
            _ => None,
        }
    }

    pub(crate) fn name(&self) -> Option<&'a [u8]> {
        match self.method {
            ClassLikeMemberSelector::Identifier(id) => Some(id.value),
            _ => None,
        }
    }

    pub(crate) fn name_span(&self) -> Span {
        node_span(self.method)
    }

    pub(crate) fn is_name(&self, names: &[&str]) -> bool {
        self.name()
            .is_some_and(|n| names.iter().any(|w| n.eq_ignore_ascii_case(w.as_bytes())))
    }

    pub(crate) fn args(&self) -> Vec<&'a Argument<'a>> {
        self.args.arguments.iter().collect()
    }

    pub(crate) fn receiver_text(&self, ctx: &Context<'_>) -> String {
        let operator = if self.is_static { "::" } else { "->" };
        format!("{}{operator}", text(ctx, self.receiver))
    }
}

pub(crate) fn is_test_case_call(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    call: &AnyCall<'_>,
) -> bool {
    receiver_is(ctx, ancestors, call, &[TEST_CASE, ASSERT])
}

pub(crate) fn is_test_case_call_named(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    call: &AnyCall<'_>,
    names: &[&str],
) -> bool {
    call.is_name(names) && is_test_case_call(ctx, ancestors, call)
}

pub(crate) fn receiver_is(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    call: &AnyCall<'_>,
    targets: &[&str],
) -> bool {
    let receiver = unparenthesize(call.receiver);
    if call.is_static {
        return match receiver {
            Expression::Self_(_) | Expression::Static(_) => {
                own_class(ancestors).is_some_and(|c| is_a(ctx, &c, targets))
            }
            Expression::Parent(_) => own_class(ancestors).is_some_and(|own| {
                let parents = class_parents(ctx, &own);
                parents.len() == 1 && is_a(ctx, &parents[0], targets)
            }),
            Expression::Identifier(id) => is_a(ctx, &resolve(ancestors, id), targets),
            _ => false,
        };
    }
    if let Expression::Variable(Variable::Direct(v)) = receiver
        && v.name == b"$this"
    {
        return own_class_like(ancestors).is_some_and(|c| class_is(ctx, c, targets));
    }
    object_class(ctx, receiver).is_some_and(|c| is_a(ctx, &c, targets))
}

pub(crate) fn object_class(ctx: &Context<'_>, expr: &Expression<'_>) -> Option<String> {
    let facts = facts(ctx);
    let t = facts.full_type(node_span(expr))?;
    match t.types.as_ref() {
        [TAtomic::Object(TObject::Named(named))] if named.intersection_types.is_none() => {
            Some(named.name.to_string())
        }
        _ => None,
    }
}

pub(crate) fn class_parents(ctx: &Context<'_>, class: &str) -> Vec<String> {
    let facts = facts(ctx);
    let mut out = Vec::new();
    let mut at = class.to_string();
    while let Some(name) = facts
        .codebase
        .get_class_like(at.as_bytes())
        .and_then(|c| c.direct_parent_class)
        .map(|parent| {
            facts
                .codebase
                .get_class_like(parent.as_bytes())
                .map_or(parent.to_string(), |p| p.original_name.to_string())
        })
        .filter(|name| !out.contains(name))
    {
        out.push(name.clone());
        at = name;
    }
    out
}

fn own_class_like<'n, 'ast, 'arena>(
    ancestors: &'n [Node<'ast, 'arena>],
) -> Option<&'n [Node<'ast, 'arena>]> {
    let at = ancestors.iter().rposition(|a| {
        matches!(
            a,
            Node::Class(_)
                | Node::AnonymousClass(_)
                | Node::Interface(_)
                | Node::Trait(_)
                | Node::Enum(_)
        )
    })?;
    (!matches!(ancestors[at], Node::Trait(_))).then(|| &ancestors[..=at])
}

pub(crate) fn own_class(ancestors: &[Node<'_, '_>]) -> Option<String> {
    let nodes = own_class_like(ancestors)?;
    class_scope(nodes)?.name
}

pub(crate) fn own_class_reflection(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
) -> Option<String> {
    let nodes = own_class_like(ancestors)?;
    Some(match class_scope(nodes).unwrap().name {
        Some(name) => name,
        None => facts(ctx).anonymous_class_name(mago_span::HasSpan::span(nodes.last().unwrap())),
    })
}

pub(crate) fn class_body<'a>(
    node: Node<'a, 'a>,
) -> Option<(
    &'a mago_syntax::cst::Sequence<'a, mago_syntax::cst::ClassLikeMember<'a>>,
    mago_span::Span,
    mago_span::Span,
)> {
    match node {
        Node::Class(c) => Some((&c.members, c.left_brace, c.right_brace)),
        Node::AnonymousClass(c) => Some((&c.members, c.left_brace, c.right_brace)),
        Node::Trait(t) => Some((&t.members, t.left_brace, t.right_brace)),
        Node::Interface(i) => Some((&i.members, i.left_brace, i.right_brace)),
        Node::Enum(e) => Some((&e.members, e.left_brace, e.right_brace)),
        _ => None,
    }
}

pub(crate) const CLASS_LIKES: &[mago_syntax::cst::NodeKind] = &[
    mago_syntax::cst::NodeKind::Class,
    mago_syntax::cst::NodeKind::AnonymousClass,
    mago_syntax::cst::NodeKind::Trait,
    mago_syntax::cst::NodeKind::Interface,
    mago_syntax::cst::NodeKind::Enum,
];

pub(crate) fn expr_text<'s>(ctx: &Context<'s>, expr: &Expression<'_>) -> &'s str {
    text(ctx, unparenthesize(expr))
}

pub(crate) fn old_args(call: &AnyCall<'_>, from: usize) -> impl Iterator<Item = NewArg> {
    (from..call.args.arguments.len()).map(NewArg::Old)
}

pub(crate) fn remove_first_arg(ctx: &Context<'_>, call: &AnyCall<'_>, name: &str) -> Vec<Edit> {
    let new: Vec<NewArg> = old_args(call, 1).collect();
    rewrite_args(ctx, call, Some(name), &new)
}

pub(crate) fn renamed(call: &AnyCall<'_>, map: &[(&str, &'static str)]) -> Option<&'static str> {
    let name = call.name()?;
    map.iter()
        .find(|(k, _)| k.as_bytes().eq_ignore_ascii_case(name))
        .map(|(_, v)| *v)
}

pub(crate) fn new_call(ctx: &Context<'_>, call: &AnyCall<'_>, name: &str, args: &[&str]) -> String {
    format!("{}{name}({})", call.receiver_text(ctx), args.join(", "))
}

pub(crate) fn string_value(expr: &Expression<'_>) -> Option<String> {
    match crate::rules::support::values::value(expr)? {
        crate::rules::support::values::Value::Str(s) => Some(s),
        _ => None,
    }
}

pub(crate) fn class_string(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
) -> Option<String> {
    use mago_syntax::cst::{Access, ClassLikeConstantSelector};
    match unparenthesize(expr) {
        Expression::Access(Access::ClassConstant(access)) => {
            let is_class = matches!(&access.constant, ClassLikeConstantSelector::Identifier(c) if c.value.eq_ignore_ascii_case(b"class"));
            match access.class {
                _ if !is_class => None,
                Expression::Identifier(class) => Some(resolve(ancestors, class)),
                Expression::Self_(_) | Expression::Static(_) => {
                    own_class_reflection(ctx, ancestors)
                }
                Expression::Parent(_) => class_parents(ctx, &own_class_reflection(ctx, ancestors)?)
                    .into_iter()
                    .next(),
                _ => None,
            }
        }
        expr => string_value(expr),
    }
}

pub(crate) fn is_constant_value(ctx: &Context<'_>, expr: &Expression<'_>) -> bool {
    use mago_codex::ttype::atomic::array::TArray;
    let expr = unparenthesize(expr);
    if matches!(expr, Expression::Literal(_) | Expression::MagicConstant(_)) {
        return true;
    }
    let facts = facts(ctx);
    facts
        .full_type(node_span(expr))
        .is_some_and(|t| match t.types.as_ref() {
            [atomic] if atomic.is_literal() => true,
            [TAtomic::Array(TArray::Keyed(k))] => k.parameters.is_none(),
            [TAtomic::Array(TArray::List(l))] => l.element_type.is_never(),
            _ => false,
        })
}

pub(crate) enum NewArg {
    Old(usize),
    New(String),
}

pub(crate) fn rewrite_args(
    ctx: &Context<'_>,
    call: &AnyCall<'_>,
    name: Option<&str>,
    new: &[NewArg],
) -> Vec<Edit> {
    let source = ctx.source;
    let old: Vec<Span> = call.args.arguments.iter().map(node_span).collect();
    let printed = |a: &NewArg| match a {
        NewArg::Old(i) => source[old[*i].start..old[*i].end].to_string(),
        NewArg::New(text) => text.clone(),
    };
    let fallback = || {
        let name = name.map_or_else(
            || source[call.name_span().start..call.name_span().end].to_string(),
            str::to_string,
        );
        let args: Vec<String> = new.iter().map(printed).collect();
        vec![Edit::new(
            call.span,
            format!("{}{name}({})", call.receiver_text(ctx), args.join(", ")),
        )]
    };
    let keys: Vec<usize> = new
        .iter()
        .enumerate()
        .map(|(j, a)| match a {
            NewArg::Old(i) => *i,
            NewArg::New(_) => old.len() + j,
        })
        .collect();
    let diff = coalesce_replacements(diff(old.len(), &keys));
    let multiline = old.len() >= 2
        && old
            .windows(2)
            .all(|w| source[w[0].end..w[1].end].contains('\n'));
    let open = call.args.left_parenthesis.end.offset as usize;
    let mut pos = open;
    let mut result = String::new();
    let mut before_first_keep = true;
    let mut skip_removed = false;
    let mut delayed: Vec<String> = Vec::new();
    let mut indent = String::new();
    let comment_start = |from: usize, i: usize| {
        crate::rules::support::statement_edits::first_comment(source, from, old[i].start)
            .unwrap_or(old[i].start)
    };
    for (index, elem) in diff.iter().enumerate() {
        match *elem {
            Elem::Keep(o, n) | Elem::Replace(o, n) => {
                before_first_keep = false;
                indent = " ".repeat(
                    crate::rules::support::statement_edits::line_indent(source, old[o].start).len(),
                );
                let from = comment_start(pos, o);
                if !skip_removed {
                    result.push_str(&source[pos..from]);
                }
                for add in delayed.drain(..) {
                    result.push_str(&add);
                    if multiline {
                        let _ = write!(result, ",\n{indent}");
                    } else {
                        result.push_str(", ");
                    }
                }
                let keep = matches!(elem, Elem::Keep(..));
                if keep {
                    result.push_str(&source[from..old[o].start]);
                }
                skip_removed = false;
                result.push_str(&printed(&new[n]));
                pos = old[o].end;
            }
            Elem::Add(n) => {
                if before_first_keep {
                    delayed.push(printed(&new[n]));
                    continue;
                }
                if multiline {
                    let _ = write!(result, ",\n{indent}");
                } else {
                    result.push_str(", ");
                }
                result.push_str(&printed(&new[n]));
            }
            Elem::Remove(o) => {
                let start = comment_start(pos, o);
                if index == 0 {
                    result.push_str(&source[pos..start]);
                    skip_removed = true;
                }
                pos = old[o].end;
            }
        }
    }
    if skip_removed || !delayed.is_empty() {
        return fallback();
    }
    let mut edits = Vec::new();
    if let Some(name) = name {
        edits.push(Edit::new(call.name_span(), name));
    }
    edits.push(Edit::new(Span::new(open, pos), result));
    edits
}

pub(crate) fn expr_name<'a>(expr: &'a Expression<'a>) -> Option<&'a [u8]> {
    use mago_syntax::cst::{Access, Call};
    let strip = |n: &'a [u8]| n.strip_prefix(b"\\").unwrap_or(n);
    match unparenthesize(expr) {
        Expression::Variable(Variable::Direct(v)) => Some(&v.name[1..]),
        Expression::Access(Access::Property(p)) => match &p.property {
            ClassLikeMemberSelector::Identifier(id) => Some(id.value),
            _ => None,
        },
        Expression::Access(Access::NullSafeProperty(p)) => match &p.property {
            ClassLikeMemberSelector::Identifier(id) => Some(id.value),
            _ => None,
        },
        Expression::Access(Access::StaticProperty(p)) => match &p.property {
            Variable::Direct(v) => Some(&v.name[1..]),
            _ => None,
        },
        Expression::ConstantAccess(c) => Some(strip(c.name.value())),
        Expression::Call(Call::Function(f)) => match f.function {
            Expression::Identifier(id) => Some(strip(id.value())),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn contains(node: Node<'_, '_>, found: &dyn Fn(Node<'_, '_>) -> bool) -> bool {
    if found(node) {
        return true;
    }
    let mut hit = false;
    node.visit_children(|child| {
        if !hit && contains(child, found) {
            hit = true;
        }
    });
    hit
}

pub(crate) fn is_method_call_named(node: Node<'_, '_>, names: &[&str]) -> bool {
    AnyCall::of(node).is_some_and(|c| !c.is_static && c.is_name(names))
}

pub(crate) fn without_equal_to(ctx: &Context<'_>, expr: &Expression<'_>) -> String {
    use mago_syntax::cst::MethodCall;
    fn collect<'a>(node: Node<'a, 'a>, root: Span, calls: &mut Vec<&'a MethodCall<'a>>) {
        if let Node::MethodCall(m) = node
            && node_span(m) != root
            && is_method_call_named(node, &["equalTo"])
        {
            calls.push(m);
            return;
        }
        node.visit_children(|child| collect(child, root, calls));
    }
    let expr = unparenthesize(expr);
    let span = node_span(expr);
    let mut calls = Vec::new();
    Node::Expression(expr).visit_children(|child| collect(child, span, &mut calls));
    let mut out = String::new();
    let mut at = span.start;
    for m in calls {
        let call = node_span(m);
        out.push_str(&ctx.source[at..call.start]);
        out.push_str(&replaced_equal_to(ctx, m));
        at = call.end;
    }
    out.push_str(&ctx.source[at..span.end]);
    out
}

fn replaced_equal_to(ctx: &Context<'_>, call: &mago_syntax::cst::MethodCall<'_>) -> String {
    match call.argument_list.arguments.first() {
        Some(first) => inner_without_equal_to(ctx, first.value()),
        None => text(ctx, call).to_string(),
    }
}

pub(crate) fn inner_without_equal_to(ctx: &Context<'_>, expr: &Expression<'_>) -> String {
    use mago_syntax::cst::Call;
    let value = unparenthesize(expr);
    if let Expression::Call(Call::Method(m)) = value
        && is_method_call_named(Node::MethodCall(m), &["equalTo"])
    {
        return replaced_equal_to(ctx, m);
    }
    without_equal_to(ctx, value)
}

pub(crate) fn method_anew(
    ctx: &crate::Context<'_>,
    method: &Method<'_>,
    block: &mago_syntax::cst::Block<'_>,
) -> String {
    use crate::rules::support::statement_edits::{block_list, line_indent};
    let source = ctx.source;
    let indent = line_indent(source, node_span(method).start);
    let mut out = crate::rules::support::class_members::modifiers_text(&method.modifiers);
    out.push_str("function ");
    if method.ampersand.is_some() {
        out.push('&');
    }
    out.push_str(std::str::from_utf8(method.name.value).unwrap_or_default());
    let list = &method.parameter_list;
    let params: Vec<Span> = list.parameters.iter().map(node_span).collect();
    out.push('(');
    out.push_str(&maybe_multiline(
        ctx,
        list.left_parenthesis.end.offset as usize,
        &params,
        indent,
    ));
    out.push(')');
    if let Some(hint) = &method.return_type_hint {
        let s = node_span(&hint.hint);
        out.push_str(": ");
        out.push_str(&source[s.start..s.end]);
    }
    out.push('\n');
    out.push_str(indent);
    out.push_str(&block_anew(ctx, &block_list(source, block), indent));
    out
}

pub(crate) fn block_anew(
    ctx: &crate::Context<'_>,
    list: &crate::rules::support::statement_edits::List<'_>,
    indent: &str,
) -> String {
    use crate::rules::support::statement_edits::{line_indent, reindent, tokens};
    let source = ctx.source;
    let inner = format!("{indent}    ");
    let mut lines = Vec::new();
    let comments = |from: usize, to: usize, lines: &mut Vec<String>| {
        for t in tokens(source, from, to)
            .into_iter()
            .filter(|t| t.kind.is_comment())
        {
            let span = Span::new(t.start, t.end);
            lines.push(reindent(source, span, line_indent(source, t.start), &inner));
        }
    };
    for (i, statement) in list.stmts.iter().enumerate() {
        let span = node_span(*statement);
        comments(list.before(ctx, i), span.start, &mut lines);
        lines.push(reindent(
            source,
            span,
            line_indent(source, span.start),
            &inner,
        ));
    }
    if let Some(nop) = list.nop_span(source) {
        comments(nop.start, nop.end, &mut lines);
    }
    if lines.is_empty() {
        return format!("{{\n{indent}}}");
    }
    format!(
        "{{\n{inner}{}\n{indent}}}",
        lines.join(&format!("\n{inner}"))
    )
}

pub(crate) fn maybe_multiline(
    ctx: &crate::Context<'_>,
    from: usize,
    items: &[Span],
    indent: &str,
) -> String {
    use crate::rules::support::statement_edits::{previous_token_end, tokens};
    let source = ctx.source;
    let mut from = from;
    let mut printed = Vec::new();
    for span in items {
        let before: Vec<&str> = tokens(
            source,
            previous_token_end(ctx, from, span.start),
            span.start,
        )
        .into_iter()
        .filter(|t| t.kind.is_comment())
        .map(|t| &source[t.start..t.end])
        .collect();
        printed.push((before, &source[span.start..span.end]));
        from = span.end;
    }
    if printed.iter().all(|(c, _)| c.is_empty()) {
        let texts: Vec<&str> = printed.iter().map(|(_, p)| *p).collect();
        return texts.join(", ");
    }
    let inner = format!("\n{indent}    ");
    let lines: Vec<String> = printed
        .iter()
        .map(|(comments, item)| {
            let mut line = String::new();
            for comment in comments {
                line.push_str(&inner);
                line.push_str(comment);
            }
            line.push_str(&inner);
            line.push_str(item);
            line
        })
        .collect();
    format!("{}\n{indent}", lines.join(","))
}

pub(crate) fn remove_class_members(
    source: &str,
    node: Node<'_, '_>,
    removed: &[bool],
) -> Vec<Edit> {
    type Parts<'n, 'a> = (
        &'n Sequence<'a, AttributeList<'a>>,
        String,
        Option<&'n Extends<'a>>,
        Option<&'n Implements<'a>>,
    );
    use crate::rules::support::class_members::{modifiers_text, remove_members};
    use crate::rules::support::statement_edits::{first_comment, line_indent};
    use mago_span::HasSpan;
    use mago_syntax::cst::{AttributeList, Extends, Implements, Sequence};
    let text = |s: mago_span::Span| &source[s.start.offset as usize..s.end.offset as usize];
    let span = node_span(&node);
    let indent = line_indent(source, span.start);
    let (lists, head, extends, implements): Parts<'_, '_> = match node {
        Node::Class(c) => (
            &c.attribute_lists,
            format!(
                "{}class {}",
                modifiers_text(&c.modifiers),
                text(c.name.span)
            ),
            c.extends.as_ref(),
            c.implements.as_ref(),
        ),
        Node::AnonymousClass(c) => {
            let args: Vec<&str> = c
                .argument_list
                .iter()
                .flat_map(|a| a.arguments.iter())
                .map(|a| text(a.span()))
                .collect();
            let args = if args.is_empty() {
                String::new()
            } else {
                format!("({})", args.join(", "))
            };
            (
                &c.attribute_lists,
                format!("{}class{args}", modifiers_text(&c.modifiers)),
                c.extends.as_ref(),
                c.implements.as_ref(),
            )
        }
        _ => {
            let Node::Trait(t) = node else { return vec![] };
            (
                &t.attribute_lists,
                format!("trait {}", text(t.name.span)),
                None,
                None,
            )
        }
    };
    let (members, left, right) = class_body(node).expect("a class-like");
    if removed.contains(&false) {
        return remove_members(source, members, left, right, removed);
    }
    let spans: Vec<Span> = members.iter().map(node_span).collect();
    let (open, close) = (left.end.offset as usize, right.start.offset as usize);
    let first = first_comment(source, open, spans[0].start).unwrap_or(spans[0].start);
    if let Some(nop) = first_comment(source, spans[spans.len() - 1].end, close) {
        return vec![Edit::new(Span::new(first, nop), "")];
    }
    let mut out = String::from(if matches!(node, Node::AnonymousClass(_)) {
        "new "
    } else {
        ""
    });
    for list in lists {
        out.push_str(text(list.span()));
        out.push('\n');
        out.push_str(indent);
    }
    out.push_str(&head);
    let names = |types: &mago_syntax::cst::TokenSeparatedSequence<'_, Identifier<'_>>| {
        types
            .iter()
            .map(|t| text(t.span()))
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(extends) = extends {
        out.push_str(" extends ");
        out.push_str(&names(&extends.types));
    }
    if let Some(implements) = implements {
        out.push_str(" implements ");
        out.push_str(&names(&implements.types));
    }
    let _ = write!(out, "\n{indent}{{\n{indent}}}");
    vec![Edit::new(span, out)]
}
