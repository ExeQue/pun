pub(crate) mod compare;
pub mod facts;
pub(crate) mod names;

mod add_array_return;
mod add_bool_return;
mod add_bool_return_from_const;
mod add_closure_never_return;
mod add_closure_return;
mod add_closure_void_return;
pub(super) mod add_datetime_property_type;
mod add_fluent_return;
mod add_function_void_return;
mod add_getter_return_phpdoc;
mod add_magic_method_types;
mod add_method_void_return;
mod add_native_scalar_return;
mod add_never_return;
mod add_param_phpdoc_from_callable_call;
mod add_param_phpdoc_from_dim_fetch;
mod add_param_phpdoc_from_local_calls;
mod add_param_phpdoc_from_reference_assign;
mod add_param_type_array_any_all;
mod add_param_type_array_dim_fetch;
mod add_param_type_array_function;
mod add_param_type_array_map;
mod add_param_type_array_reduce;
mod add_param_type_arrow_dim_fetch;
mod add_param_type_data_provider;
mod add_param_type_dim_fetch_key;
mod add_param_type_iterable_method_call;
mod add_param_type_method_call;
mod add_param_type_property;
mod add_param_type_spl_fixed_array;
mod add_param_type_variable_call;
mod add_property_type_assigns;
mod add_property_type_behat_context;
mod add_property_type_constructor;
mod add_property_type_create_mock;
mod add_property_type_jms;
mod add_property_type_setter_getter;
mod add_property_type_setup;
mod add_property_type_setup_docblock;
mod add_return_phpdoc_common_object;
mod add_return_phpdoc_dim_assigned_object;
mod add_return_phpdoc_direct_array;
mod add_return_phpdoc_json_array;
mod add_return_phpdoc_scalar_array;
mod add_return_type_from_cast;
mod add_return_type_from_constant;
mod add_return_type_from_get_repository;
mod add_return_type_from_native_call;
mod add_return_type_from_new;
mod add_return_type_from_new_array;
mod add_return_type_from_param;
mod add_return_type_from_parent;
mod add_return_type_from_property;
mod add_return_type_from_symfony_serializer;
mod add_return_type_from_ternary;
mod add_return_type_from_try_catch;
mod add_return_type_from_typed_call;
mod add_return_type_from_yield;
mod add_scalar_return;
mod add_union_return;
mod add_var_phpdoc_from_constructor_param;
mod add_var_phpdoc_from_default;
mod add_var_phpdoc_from_dim_fetch;
mod add_var_phpdoc_from_getter;
pub(crate) mod declare;
pub(crate) mod docblock;
mod doctrine_repository;
mod implicit_array_observer;
mod native;
mod never;
pub(crate) mod no_nullsafe_on_non_null;
mod param;
mod prefer_explicit_null_instanceof;
mod prefer_generic_collection_phpdoc;
mod prefer_identical_same_type;
mod prefer_instanceof;
mod prefer_specific_return_type;
pub(crate) mod property_type;
pub(crate) mod rector_type;
pub(crate) mod silent_void;
mod type_factory;
mod union_method;

pub(crate) use add_closure_return::closure_return;
pub(crate) use never::has_parent_class_method;

use mago_syntax::cst::{
    Block, Expression, Extends, Identifier, MethodBody, Modifier, Node, Return, Sequence, Statement,
};
use pun_problem::{Edit, Safety, Span};

use crate::Report;
use crate::syntax::node_span;
use facts::Facts;
use names::resolve_class_name;

rules! {
    pub(super) fn all {
        add_return_type_from_parent => AddReturnTypeFromParent,
        add_closure_void_return => AddClosureVoidReturn,
        add_function_void_return => AddFunctionVoidReturn,
        add_closure_return => AddArrowFunctionReturn,
        add_bool_return_from_const => AddBoolReturnFromConst,
        add_return_type_from_new_array => AddPrivateMethodArrayReturnType,
        add_return_type_from_new_array => AddReturnTypeFromNewArray,
        add_return_type_from_constant => AddReturnTypeFromConstant,
        add_scalar_return => AddStringReturnFromScalar,
        add_scalar_return => AddNumericReturnFromScalar,
        add_bool_return => AddBoolReturn,
        add_native_scalar_return => AddStringReturn,
        add_native_scalar_return => AddNumericReturn,
        add_return_type_from_ternary => AddReturnTypeFromTernary,
        add_array_return => AddArrayReturn,
        add_return_type_from_new => AddReturnTypeFromNew,
        add_return_type_from_cast => AddReturnTypeFromCast,
        add_return_type_from_symfony_serializer => AddReturnTypeFromSymfonySerializer,
        add_method_void_return => AddMethodVoidReturn,
        add_return_type_from_property => AddReturnTypeFromProperty,
        add_union_return => AddNullableReturn,
        add_return_type_from_yield => AddReturnTypeFromYield,
        add_return_type_from_native_call => AddReturnTypeFromNativeCall,
        add_return_type_from_try_catch => AddReturnTypeFromTryCatch,
        add_return_type_from_typed_call => AddReturnTypeFromTypedCall,
        add_magic_method_types => AddMagicMethodTypes,
        prefer_specific_return_type => PreferSpecificReturnType,
        add_union_return => AddUnionReturn,
        add_closure_never_return => AddClosureNeverReturn,
        add_closure_return => AddClosureReturn,
        add_return_type_from_param => AddReturnTypeFromParam,
        add_fluent_return => AddFluentReturn,
        add_never_return => AddNeverReturn,
        prefer_explicit_null_instanceof => PreferExplicitNullInstanceof,
        prefer_instanceof => PreferInstanceof,
        prefer_identical_same_type => PreferIdenticalSameType,
        no_nullsafe_on_non_null => NoNullsafeOnNonNull,
        add_property_type_constructor => AddPropertyTypeConstructor,
        add_property_type_assigns => AddPropertyTypeAssigns::default(),
        add_property_type_setter_getter => AddPropertyTypeSetterGetter,
        add_property_type_jms => AddPropertyTypeJmsScalar,
        add_property_type_jms => AddPropertyTypeJmsObject,
        add_property_type_behat_context => AddPropertyTypeBehatContext,
        doctrine_repository => AddPropertyTypeContainerGet,
        doctrine_repository => AddPropertyTypeGetRepository,
        add_property_type_create_mock => AddPropertyTypeCreateMock,
        add_property_type_setup => AddPropertyTypeSetup,
        add_property_type_setup_docblock => AddPropertyTypeSetupDocblock,
        doctrine_repository => AddDoctrineRepositoryReturnType,
        add_return_type_from_get_repository => AddReturnTypeFromGetRepository,
        add_var_phpdoc_from_default => AddVarPhpdocFromDefault,
        add_param_phpdoc_from_dim_fetch => AddParamPhpdocFromDimFetch,
        add_param_phpdoc_from_local_calls => AddParamPhpdocFromLocalCalls,
        add_param_phpdoc_from_reference_assign => AddParamPhpdocFromReferenceAssign,
        add_param_phpdoc_from_callable_call => AddParamPhpdocFromCallableCall,
        add_return_phpdoc_common_object => AddReturnPhpdocCommonObject,
        add_return_phpdoc_scalar_array => AddReturnPhpdocScalarArray,
        add_return_phpdoc_direct_array => AddReturnPhpdocDirectArray,
        add_return_phpdoc_dim_assigned_object => AddReturnPhpdocDimAssignedObject,
        add_return_phpdoc_json_array => AddReturnPhpdocJsonArray,
        add_var_phpdoc_from_constructor_param => AddVarPhpdocFromConstructorParam,
        add_var_phpdoc_from_getter => AddVarPhpdocFromGetter,
        add_var_phpdoc_from_dim_fetch => AddVarPhpdocFromDimFetch,
        add_getter_return_phpdoc => AddGetterReturnPhpdoc,
        prefer_generic_collection_phpdoc => PreferGenericCollectionPhpdoc,
        add_param_type_spl_fixed_array => AddParamTypeSplFixedArray,
        add_param_type_data_provider => AddParamTypeDataProvider,
        add_param_type_method_call => AddParamTypeMethodCall::default(),
        add_param_type_array_map => AddParamTypeArrayMap,
        add_param_type_array_reduce => AddParamTypeArrayReduce,
        add_param_type_arrow_dim_fetch => AddParamTypeArrowDimFetch,
        add_param_type_property => AddParamTypeProperty,
        add_param_type_variable_call => AddParamTypeVariableCall,
        add_param_type_array_dim_fetch => AddParamTypeArrayDimFetch,
        add_param_type_dim_fetch_key => AddParamTypeDimFetchKey,
        add_param_type_array_function => AddParamTypeArrayFunction,
        add_param_type_iterable_method_call => AddParamTypeIterableMethodCall,
        add_param_type_array_any_all => PreferNarrowParamType,
        add_param_type_array_any_all => AddParamTypeArrayAnyAll,
    }
}

pub(crate) const MAGIC: [&str; 17] = [
    "__construct",
    "__destruct",
    "__call",
    "__callstatic",
    "__get",
    "__set",
    "__isset",
    "__unset",
    "__sleep",
    "__wakeup",
    "__tostring",
    "__set_state",
    "__clone",
    "__invoke",
    "__debuginfo",
    "__serialize",
    "__unserialize",
];

pub(crate) const SCALAR_TYPES: u32 = 70000;

pub(crate) const VOID_TYPE: u32 = 70100;

pub(crate) fn add_return_type(span: Span, at: usize, r#type: &str) -> Report {
    Report::fixed(
        span,
        format!("Add the `{type}` return type."),
        Safety::Unsafe,
        vec![Edit::new(Span::new(at, at), format!(": {type}"))],
    )
}

pub(crate) fn any_scoped<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    found: &mut dyn FnMut(Node<'ast, 'arena>) -> bool,
) -> bool {
    let mut hit = false;
    node.visit_children(|child| {
        if hit {
            return;
        }
        if found(child) {
            hit = true;
            return;
        }
        match child {
            Node::Function(_)
            | Node::Closure(_)
            | Node::ArrowFunction(_)
            | Node::Method(_)
            | Node::Class(_)
            | Node::Interface(_)
            | Node::Trait(_)
            | Node::Enum(_) => {}
            Node::AnonymousClass(class) => {
                if let Some(args) = &class.argument_list {
                    hit = any_scoped(Node::PartialArgumentList(args), found);
                }
            }
            _ => hit = any_scoped(child, found),
        }
    });
    hit
}

pub(crate) struct Untyped<'ast, 'arena> {
    pub span: Span,
    pub at: usize,
    pub body: &'ast Block<'arena>,
}

pub(crate) fn untyped<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    ancestors: &[Node<'ast, 'arena>],
    ctx: &crate::Context<'_>,
) -> Option<Untyped<'ast, 'arena>> {
    match node {
        Node::Function(f) if f.return_type_hint.is_none() => Some(Untyped {
            span: node_span(&f.name),
            at: node_span(&f.parameter_list).end,
            body: &f.body,
        }),
        Node::Method(m) if m.return_type_hint.is_none() => {
            let MethodBody::Concrete(body) = &m.body else {
                return None;
            };
            let scope = class_scope(ancestors);
            if never::should_skip_class_method(&facts::facts(ctx), scope.as_ref(), m) {
                return None;
            }
            Some(Untyped {
                span: node_span(&m.name),
                at: node_span(&m.parameter_list).end,
                body,
            })
        }
        _ => None,
    }
}

pub(crate) fn returns_scoped<'ast, 'arena>(body: &'ast Block<'arena>) -> Vec<&'ast Return<'arena>> {
    let mut returns = Vec::new();
    let yields = any_scoped(Node::Block(body), &mut |n| match n {
        Node::Yield(_) => true,
        Node::Return(r) => {
            returns.push(r);
            false
        }
        _ => false,
    });
    if yields {
        returns.clear();
    }
    returns
}

pub(crate) fn has_only_return_with_expr(
    place: &silent_void::Place<'_, '_, '_>,
    body: &Block<'_>,
    returns: &[&Return<'_>],
) -> bool {
    !returns.is_empty()
        && returns.iter().all(|r| r.value.is_some())
        && !silent_void::has_silent_void(place, body.statements.iter())
}

pub(crate) fn has_exclusive_void(body: &Block<'_>) -> bool {
    !any_scoped(Node::Block(body), &mut |n| match n {
        Node::Return(r) => r.value.is_some(),
        Node::Yield(_) => true,
        _ => false,
    })
}

pub(crate) fn statement_count(source: &str, block: &Block<'_>) -> usize {
    let statements: Vec<&Statement<'_>> = block.statements.iter().collect();
    let from = statements
        .last()
        .map_or(block.left_brace.end.offset as usize, |s| node_span(*s).end);
    let rest = &source[from..block.right_brace.start.offset as usize];
    statements.len() + usize::from(!rest.trim().is_empty())
}

pub(crate) struct ClassScope {
    pub name: Option<String>,
    pub is_final: bool,
    pub is_abstract: bool,
    pub is_interface: bool,
    pub parents: Vec<String>,
    pub parent_class: Option<String>,
}

pub(crate) fn class_scope(ancestors: &[Node<'_, '_>]) -> Option<ClassScope> {
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
    let before = &ancestors[..at];
    let named = |name: &[u8]| {
        let id = String::from_utf8_lossy(name);
        Some(match namespace_of(before) {
            Some(ns) => format!("{ns}\\{id}"),
            None => id.into_owned(),
        })
    };
    let resolve = |ids: Vec<&Identifier<'_>>| {
        ids.into_iter()
            .map(|id| resolve_class_name(before, id))
            .collect::<Vec<_>>()
    };
    let has = |modifiers: &Sequence<'_, Modifier<'_>>, f: fn(&Modifier<'_>) -> bool| {
        modifiers.iter().any(f)
    };
    let extended = |extends: Option<&Extends<'_>>| {
        extends
            .and_then(|e| e.types.first())
            .map(|id| resolve_class_name(before, id))
    };
    Some(match ancestors[at] {
        Node::Class(c) => ClassScope {
            name: named(c.name.value),
            parent_class: extended(c.extends.as_ref()),
            is_final: has(&c.modifiers, |m| matches!(m, Modifier::Final(_))),
            is_abstract: has(&c.modifiers, |m| matches!(m, Modifier::Abstract(_))),
            is_interface: false,
            parents: resolve(
                c.extends
                    .iter()
                    .flat_map(|e| e.types.iter())
                    .chain(c.implements.iter().flat_map(|i| i.types.iter()))
                    .collect(),
            ),
        },
        Node::AnonymousClass(c) => ClassScope {
            name: None,
            parent_class: extended(c.extends.as_ref()),
            is_final: true,
            is_abstract: false,
            is_interface: false,
            parents: resolve(
                c.extends
                    .iter()
                    .flat_map(|e| e.types.iter())
                    .chain(c.implements.iter().flat_map(|i| i.types.iter()))
                    .collect(),
            ),
        },
        Node::Interface(i) => ClassScope {
            name: named(i.name.value),
            is_final: false,
            is_abstract: true,
            is_interface: true,
            parent_class: None,
            parents: resolve(i.extends.iter().flat_map(|e| e.types.iter()).collect()),
        },
        Node::Trait(t) => ClassScope {
            name: named(t.name.value),
            is_final: false,
            is_abstract: false,
            is_interface: false,
            parent_class: None,
            parents: Vec::new(),
        },
        Node::Enum(e) => ClassScope {
            name: named(e.name.value),
            is_final: true,
            is_abstract: false,
            is_interface: false,
            parent_class: None,
            parents: resolve(e.implements.iter().flat_map(|i| i.types.iter()).collect()),
        },
        _ => unreachable!("matched above"),
    })
}

pub(crate) fn namespace_of<'a>(ancestors: &[Node<'a, 'a>]) -> Option<String> {
    ancestors.iter().rev().find_map(|a| match a {
        Node::Namespace(ns) => ns
            .name
            .as_ref()
            .map(|n| String::from_utf8_lossy(n.value()).into_owned()),
        _ => None,
    })
}

pub(crate) fn ancestor_names(facts: &Facts, scope: &ClassScope) -> Vec<String> {
    let codebase = &facts.codebase;
    if let Some(class) = scope
        .name
        .as_deref()
        .and_then(|n| codebase.get_class_like(n.as_bytes()))
    {
        let words = class
            .all_parent_interfaces
            .iter()
            .chain(class.used_traits.iter())
            .chain(class.all_parent_classes.iter());
        return words
            .map(|w| String::from_utf8_lossy(w.as_bytes()).into_owned())
            .collect();
    }
    let mut out = Vec::new();
    for parent in &scope.parents {
        out.push(parent.clone());
        if let Some(class) = codebase.get_class_like(parent.as_bytes()) {
            out.extend(
                class
                    .all_parent_interfaces
                    .iter()
                    .chain(class.used_traits.iter())
                    .chain(class.all_parent_classes.iter())
                    .map(|w| String::from_utf8_lossy(w.as_bytes()).into_owned()),
            );
        }
    }
    out
}

pub(crate) fn is_return_vendor_locked(facts: &Facts, scope: &ClassScope, method: &[u8]) -> bool {
    for ancestor in ancestor_names(facts, scope) {
        let Some(m) = facts.codebase.get_method(ancestor.as_bytes(), method) else {
            continue;
        };
        return m
            .return_type_declaration_metadata
            .as_ref()
            .is_some_and(|t| !t.type_union.is_mixed());
    }
    false
}

pub(crate) fn is_throw_statement(statement: &Statement<'_>) -> bool {
    matches!(statement, Statement::Expression(e) if matches!(e.expression, Expression::Throw(_)))
}
