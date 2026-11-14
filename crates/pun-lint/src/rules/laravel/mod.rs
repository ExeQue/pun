mod add_factory_extends_phpdoc;
mod add_has_factory_trait;
mod add_query_closure_type;
mod add_relation_generics;
mod add_relation_query_closure_type;
mod add_tap_closure_type;
mod conditional_helpers;
mod names;
mod no_app_array_access;
mod no_collection_aliases;
mod no_controller_middleware;
mod no_negated_blank_filled;
mod no_negated_contains;
mod no_negated_filter;
mod no_negated_when;
mod no_request_all;
mod no_useless_value_call;
mod no_useless_with_call;
mod object_type;
mod prefer_abort_if;
mod prefer_anonymous_migration;
mod prefer_back_helper;
mod prefer_casts_method;
mod prefer_class_container_key;
mod prefer_closure_only_bind;
mod prefer_collected_by_attribute;
mod prefer_collection_all;
mod prefer_constructor_injection;
mod prefer_date_facade;
mod prefer_default_argument;
mod prefer_dispatch_helper;
mod prefer_env_helper;
mod prefer_environment_check;
mod prefer_exception_class_string;
mod prefer_explicit_json_call;
mod prefer_facade_over_app;
mod prefer_latest_oldest;
mod prefer_nullsafe;
mod prefer_protected_model_methods;
mod prefer_query_builder_call;
mod prefer_report_if;
mod prefer_request_facade;
mod prefer_request_server;
mod prefer_request_validate;
mod prefer_resolve_helper;
mod prefer_scope_attribute;
mod prefer_session_facade;
mod prefer_sleep_facade;
mod prefer_status_assertion_method;
mod prefer_str_starts_with;
mod prefer_table_attribute;
mod prefer_throw_if;
mod prefer_to_route_helper;
mod prefer_today;
mod prefer_travel_to;
mod prefer_typed_assert_closure;
mod prefer_validation_rule_array;
mod prefer_view_array;
mod print;
pub(crate) mod property_attribute;
mod property_rules;
mod query_builder;

pub use prefer_class_container_key::ContainerService;
pub use prefer_constructor_injection::Injection;

use mago_syntax::cst::{
    Argument, ArgumentList, Assignment, AssignmentOperator, Call, ClassLikeMemberSelector,
    CompositeString, Expression, Node, UnaryPrefixOperator, Variable,
};
use pun_problem::{Edit, Safety, Span};

use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report};
use names::resolve_class_name;

rules! {
    pub(super) fn all {
        no_negated_blank_filled => NoNegatedBlankFilled,
        prefer_back_helper => PreferBackHelper,
        prefer_to_route_helper => PreferToRouteHelper,
        prefer_env_helper => PreferEnvHelper,
        prefer_exception_class_string => PreferExceptionClassString,
        prefer_sleep_facade => PreferSleepFacade,
        prefer_today => PreferToday,
        prefer_request_server => PreferRequestServer,
        prefer_str_starts_with => PreferStrStartsWith,
        prefer_session_facade => PreferSessionFacade,
        prefer_abort_if => PreferAbortIf,
        prefer_report_if => PreferReportIf,
        prefer_throw_if => PreferThrowIf,
        prefer_resolve_helper => PreferResolveHelper,
        prefer_facade_over_app => PreferFacadeOverApp,
        no_request_all => NoRequestAll,
        prefer_casts_method => PreferCastsMethod,
        prefer_anonymous_migration => PreferAnonymousMigration,
        prefer_nullsafe => PreferNullsafe::default(),
        prefer_date_facade => PreferDateFacade,
        prefer_request_facade => PreferRequestFacade,
        prefer_default_argument => PreferDefaultArgument,
        prefer_explicit_json_call => PreferExplicitJsonCall,
        prefer_travel_to => PreferTravelTo,
        property_rules => PreferAliasesAttribute,
        property_rules => PreferAppendsAttribute,
        property_rules => PreferBackoffAttribute,
        prefer_collected_by_attribute => PreferCollectedByAttribute,
        property_rules => PreferCollectsAttribute,
        property_rules => PreferCommandHiddenAttribute,
        property_rules => PreferConnectionAttribute,
        property_rules => PreferDateFormatAttribute,
        property_rules => PreferDelayAttribute,
        property_rules => PreferDeleteWhenMissingModelsAttribute,
        property_rules => PreferDescriptionAttribute,
        property_rules => PreferErrorBagAttribute,
        property_rules => PreferFailOnTimeoutAttribute,
        property_rules => PreferFillableAttribute,
        property_rules => PreferGuardedAttribute,
        property_rules => PreferHelpAttribute,
        property_rules => PreferHiddenAttribute,
        property_rules => PreferJobConnectionAttribute,
        property_rules => PreferMaxExceptionsAttribute,
        property_rules => PreferPreserveKeysAttribute,
        property_rules => PreferQueueAttribute,
        property_rules => PreferSignatureAttribute,
        property_rules => PreferStopOnFirstFailureAttribute,
        property_rules => PreferTimeoutAttribute,
        prefer_table_attribute => PreferTableAttribute,
        property_rules => PreferTouchesAttribute,
        property_rules => PreferTriesAttribute,
        property_rules => PreferUniqueForAttribute,
        property_rules => PreferVisibleAttribute,
        property_rules => PreferWithoutIncrementingAttribute,
        property_rules => PreferWithoutTimestampsAttribute,
        prefer_scope_attribute => PreferScopeAttribute,
        no_collection_aliases => NoCollectionAliases,
        no_negated_contains => NoNegatedContains,
        no_negated_filter => NoNegatedFilter,
        no_negated_when => NoNegatedWhen,
        prefer_status_assertion_method => PreferStatusAssertionMethod,
        prefer_environment_check => PreferEnvironmentCheck,
        prefer_typed_assert_closure => PreferTypedAssertClosure,
        no_useless_value_call => NoUselessValueCall,
        no_useless_with_call => NoUselessWithCall,
        prefer_dispatch_helper => PreferDispatchHelper,
        prefer_validation_rule_array => PreferValidationRuleArray,
        no_controller_middleware => NoControllerMiddleware,
        prefer_view_array => PreferViewArray,
        add_query_closure_type => AddQueryClosureType,
        add_relation_query_closure_type => AddRelationQueryClosureType,
        prefer_protected_model_methods => PreferProtectedModelMethods,
        prefer_latest_oldest => PreferLatestOldest::default(),
        prefer_request_validate => PreferRequestValidate,
        prefer_collection_all => PreferCollectionAll,
        prefer_closure_only_bind => PreferClosureOnlyBind,
        add_has_factory_trait => AddHasFactoryTrait::default(),
        prefer_query_builder_call => PreferQueryBuilderCall::default(),
        prefer_class_container_key => PreferClassContainerKey::default(),
        add_factory_extends_phpdoc => AddFactoryExtendsPhpdoc,
        add_relation_generics => AddRelationGenerics,
        add_tap_closure_type => AddTapClosureType,
        no_app_array_access => NoAppArrayAccess,
        prefer_constructor_injection => PreferConstructorInjection::default(),
    }
}

pub(crate) fn is_name(name: &[u8], wanted: &str) -> bool {
    name.eq_ignore_ascii_case(wanted.as_bytes())
}

pub(crate) fn member_name<'a>(selector: &ClassLikeMemberSelector<'a>) -> Option<&'a [u8]> {
    match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    }
}

pub(crate) fn class_name(expr: &Expression<'_>, ancestors: &[Node<'_, '_>]) -> Option<String> {
    match expr {
        Expression::Identifier(id) => Some(resolve_class_name(ancestors, id)),
        _ => None,
    }
}

pub(crate) fn is_variable(expr: &Expression<'_>, name: &str) -> bool {
    matches!(unparenthesize(expr), Expression::Variable(Variable::Direct(v)) if &v.name[1..] == name.as_bytes())
}

pub(crate) fn argument_texts<'s>(ctx: &Context<'s>, list: &ArgumentList<'_>) -> Vec<&'s str> {
    list.arguments
        .iter()
        .map(|a| {
            let s = node_span(a);
            &ctx.source[s.start..s.end]
        })
        .collect()
}

pub(crate) fn get_arg<'a>(
    list: &'a ArgumentList<'a>,
    name: &str,
    position: usize,
) -> Option<&'a Argument<'a>> {
    list.arguments
        .iter()
        .enumerate()
        .find(|(i, a)| match a {
            Argument::Named(n) => n.name.value == name.as_bytes(),
            Argument::Positional(p) => p.ellipsis.is_none() && *i == position,
        })
        .map(|(_, a)| a)
}

pub(crate) fn call(name: &str, args: &[&str]) -> String {
    format!("{name}({})", args.join(", "))
}

fn parent<'n, 'ast, 'arena>(
    span: Span,
    ancestors: &'n [Node<'ast, 'arena>],
) -> Option<(usize, &'n Node<'ast, 'arena>)> {
    ancestors
        .iter()
        .enumerate()
        .rev()
        .find(|(_, a)| node_span(*a) != span && !matches!(a, Node::Parenthesized(_)))
}

pub(crate) fn is_written(span: Span, ancestors: &[Node<'_, '_>]) -> bool {
    let mut at = span;
    let mut index = ancestors.len();
    loop {
        let Some((i, node)) = parent(at, &ancestors[..index]) else {
            return false;
        };
        match node {
            Node::Assignment(Assignment { lhs, .. }) => return node_span(*lhs) == at,
            Node::UnaryPrefix(u) => {
                return matches!(
                    u.operator,
                    UnaryPrefixOperator::Reference(_)
                        | UnaryPrefixOperator::PreIncrement(_)
                        | UnaryPrefixOperator::PreDecrement(_)
                );
            }
            Node::UnaryPostfix(_) | Node::IssetConstruct(_) | Node::Unset(_) => return true,
            Node::Foreach(foreach) => {
                let target = node_span(&foreach.target);
                return target.start <= at.start && at.end <= target.end;
            }
            Node::ArrayAccess(access) if node_span(access.array) == at => {
                at = node_span(node);
                index = i;
            }
            Node::ArrayElement(_)
            | Node::KeyValueArrayElement(_)
            | Node::ValueArrayElement(_)
            | Node::Array(_)
            | Node::List(_) => {
                at = node_span(node);
                index = i;
            }
            _ => return false,
        }
    }
}

pub(crate) fn is_by_ref_argument(
    ctx: &Context<'_>,
    span: Span,
    ancestors: &[Node<'_, '_>],
) -> bool {
    let mut at = span;
    let mut index = ancestors.len();
    while let Some((i, Node::ArrayAccess(access))) = parent(at, &ancestors[..index]) {
        if node_span(access.array) != at {
            break;
        }
        at = node_span(*access);
        index = i;
    }
    let Some(call) = ancestors[..index].iter().rev().find_map(|a| match a {
        Node::FunctionCall(call) => Some(*call),
        _ => None,
    }) else {
        return false;
    };
    let Some((position, argument)) = (call.argument_list.arguments.iter().enumerate())
        .find(|(_, a)| node_span(unparenthesize(a.value())) == at)
    else {
        return false;
    };
    let Some(name) = crate::rules::support::names::called_function_name(ancestors, call.function)
    else {
        return false;
    };
    let facts = crate::rules::types::facts::facts(ctx);
    let local = matches!(
        call.function,
        Expression::Identifier(mago_syntax::cst::Identifier::Local(_))
    );
    let namespaced = (crate::rules::types::names::class_imports(ancestors).0)
        .filter(|_| local && !name.contains('\\'));
    let Some(function) = namespaced
        .and_then(|ns| {
            facts
                .codebase
                .get_function(format!("{ns}\\{name}").as_bytes())
        })
        .or_else(|| facts.codebase.get_function(name.as_bytes()))
    else {
        return false;
    };
    let parameters = &function.parameters;
    let parameter = match argument {
        Argument::Named(named) => parameters
            .iter()
            .find(|p| p.name.0.as_bytes().get(1..) == Some(named.name.value)),
        Argument::Positional(_) => parameters
            .get(position)
            .or_else(|| parameters.last().filter(|p| p.flags.is_variadic())),
    };
    parameter.is_some_and(|p| p.flags.is_by_reference())
}

pub(crate) fn in_string(ancestors: &[Node<'_, '_>]) -> bool {
    ancestors.iter().any(|a| {
        matches!(
            a,
            Node::CompositeString(
                CompositeString::Interpolated(_)
                    | CompositeString::Document(_)
                    | CompositeString::ShellExecute(_)
            )
        )
    })
}

pub(crate) fn in_superglobal_assign(ancestors: &[Node<'_, '_>]) -> bool {
    const SUPERGLOBALS: [&str; 5] = ["_SERVER", "_GET", "_POST", "_REQUEST", "_ENV"];
    ancestors.iter().any(|a| match a {
        Node::Assignment(assign) if matches!(assign.operator, AssignmentOperator::Assign(_)) => {
            let var = match unparenthesize(assign.lhs) {
                Expression::ArrayAccess(access) => access.array,
                other => other,
            };
            SUPERGLOBALS.iter().any(|name| is_variable(var, name))
        }
        Node::IssetConstruct(_) | Node::Unset(_) => true,
        Node::CompositeString(CompositeString::Interpolated(_) | CompositeString::Document(_)) => {
            true
        }
        _ => false,
    })
}

pub(crate) fn is_bare_call(expr: &Expression<'_>, name: &str) -> bool {
    matches!(
        unparenthesize(expr),
        Expression::Call(Call::Function(f))
            if crate::rules::support::expressions::named(f, name) && f.argument_list.arguments.is_empty()
    )
}

pub(crate) fn replace(span: Span, message: String, code: String) -> Report {
    Report::fixed(span, message, Safety::Unsafe, vec![Edit::new(span, code)])
}

pub(crate) fn redirect_helper(
    node: Node<'_, '_>,
    ancestors: &[Node<'_, '_>],
    ctx: &Context<'_>,
    method: &str,
    helper: &str,
    out: &mut Vec<Report>,
) {
    let (span, list) = match node {
        Node::MethodCall(m)
            if member_name(&m.method).is_some_and(|n| is_name(n, method))
                && is_bare_call(m.object, "redirect") =>
        {
            (node_span(m), &m.argument_list)
        }
        Node::StaticMethodCall(s)
            if member_name(&s.method).is_some_and(|n| is_name(n, method))
                && class_name(s.class, ancestors).is_some_and(|c| {
                    c.eq_ignore_ascii_case("Illuminate\\Support\\Facades\\Redirect")
                }) =>
        {
            (node_span(s), &s.argument_list)
        }
        _ => return,
    };
    out.push(replace(
        span,
        format!("Use `{helper}()` instead of redirecting with `{method}()`."),
        call(helper, &argument_texts(ctx, list)),
    ));
}

pub(crate) fn is_a(ctx: &Context<'_>, class: &str, targets: &[&str]) -> bool {
    let target = |name: &[u8]| {
        targets
            .iter()
            .any(|t| t.as_bytes().eq_ignore_ascii_case(name))
    };
    if target(class.as_bytes()) {
        return true;
    }
    let facts = crate::rules::types::facts::facts(ctx);
    let codebase = &facts.codebase;
    codebase.get_class_like(class.as_bytes()).is_some_and(|c| {
        c.all_parent_classes
            .iter()
            .chain(c.all_parent_interfaces.iter())
            .any(|p| target(p.as_bytes()))
            || std::iter::once(&c.name)
                .chain(c.all_parent_classes.iter())
                .filter_map(|p| codebase.get_class_like(p.as_bytes()))
                .any(|p| p.used_traits.iter().any(|t| target(t.as_bytes())))
    })
}

pub(crate) fn is_object(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
    class: &str,
) -> bool {
    match unparenthesize(expr) {
        Expression::Variable(Variable::Direct(v)) if v.name == b"$this" => {
            let own = ancestors.iter().rev().find_map(|a| {
                let name = match a {
                    Node::Class(c) => c.name.value,
                    Node::Enum(e) => e.name.value,
                    Node::AnonymousClass(c) => {
                        let span = mago_span::HasSpan::span(*c);
                        return Some(
                            crate::rules::types::facts::facts(ctx).anonymous_class_name(span),
                        );
                    }
                    _ => return None,
                };
                let name = String::from_utf8_lossy(name);
                Some(match crate::rules::types::namespace_of(ancestors) {
                    Some(ns) => format!("{ns}\\{name}"),
                    None => name.into_owned(),
                })
            });
            own.is_some_and(|own| is_a(ctx, &own, &[class]))
        }
        e => crate::rules::support::values::is_object_of(ancestors, e, class, node_span(e).start),
    }
}
