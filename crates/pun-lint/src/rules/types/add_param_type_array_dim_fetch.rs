use mago_codex::ttype::TType;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::array::TArray;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{
    Argument, AssignmentOperator, BinaryOperator, Call, ClassLikeMemberSelector, Expression,
    FunctionLikeParameterList, Instantiation, MethodBody, Modifier, Node, NodeKind,
    UnaryPrefixOperator, Variable,
};
use pun_problem::{Safety, Span};

use super::add_param_type_method_call::walk;
use super::class_scope;
use super::facts::{Facts, facts};
use super::has_parent_class_method;
use super::param::{Flow, add_hint, traverse};
use crate::rules::phpunit::string_value;
use crate::rules::support::arguments::value;
use crate::rules::support::class_members::type_of;
use crate::rules::support::expressions::named;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct AddParamTypeArrayDimFetch;

impl Rule for AddParamTypeArrayDimFetch {
    fn id(&self) -> &'static str {
        "lint/types/add-param-type-array-dim-fetch"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method, NodeKind::Function, NodeKind::Closure])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (params, body): (&FunctionLikeParameterList<'_>, Node<'_, '_>) = match node {
            Node::Method(m) => {
                let MethodBody::Concrete(body) = &m.body else {
                    return;
                };
                (&m.parameter_list, Node::Block(body))
            }
            Node::Function(f) => (&f.parameter_list, Node::Block(&f.body)),
            _ => {
                let Node::Closure(c) = node else { return };
                (&c.parameter_list, Node::Block(&c.body))
            }
        };
        let facts = facts(ctx);
        if let Node::Method(m) = node
            && !m
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Private(_)))
            && class_scope(ancestors)
                .is_some_and(|scope| has_parent_class_method(&facts, &scope, m.name.value))
        {
            return;
        }
        let doc = crate::rules::support::docblocks::Doc::of(ctx, ancestors, node_span(&node).start);
        let passed = matches!(node, Node::Closure(_))
            && ancestors
                .iter()
                .rev()
                .find(|a| !matches!(a, Node::Expression(_) | Node::Parenthesized(_)))
                .is_some_and(|a| {
                    matches!(
                        a,
                        Node::Argument(_) | Node::PositionalArgument(_) | Node::NamedArgument(_)
                    )
                });
        let call = ancestors.iter().rev().find(|a| {
            !matches!(
                a,
                Node::Expression(_)
                    | Node::Parenthesized(_)
                    | Node::Argument(_)
                    | Node::PositionalArgument(_)
                    | Node::NamedArgument(_)
                    | Node::ArgumentList(_)
            )
        });
        let numeric_matches = matches!(call, Some(Node::FunctionCall(call)) if named(call, "preg_replace_callback")
            && call.argument_list.arguments.first().and_then(|a| string_value(value(a))).is_some_and(|p| numeric_group(&p)));
        let frames = matches!(ancestors.iter().rev().find(|a| matches!(a, Node::ArgumentList(_))),
            Some(Node::ArgumentList(list)) if list.arguments.iter().any(|a| matches!(unparenthesize(value(a)),
                Expression::Call(Call::Function(f)) if named(f, "debug_backtrace"))));
        let mut edits = Vec::new();
        for (index, param) in params.parameters.iter().enumerate() {
            if param.hint.is_some()
                || param.ellipsis.is_some()
                || (numeric_matches || frames) && index == 0
            {
                continue;
            }
            if let Some(default) = &param.default_value
                && !is_array(&facts, default.value)
            {
                continue;
            }
            let tagged = doc.as_ref().is_some_and(|d| {
                (0..d.len()).any(|i| {
                    let words = d.words(i);
                    words.first() == Some(&"@param")
                        && words.iter().any(|w| w.as_bytes() == param.variable.name)
                })
            });
            if is_accessed_array_dim_fetch(&facts, body, param.variable.name, tagged || passed)
                && !matches!(node, Node::Method(m) if m.modifiers.iter().any(|m| matches!(m, Modifier::Private(_)))
                    && is_called_with_array_access_arg(&facts, ancestors, m.name.value, index))
            {
                edits.push(add_hint(param, "array"));
            }
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Add the `array` type to parameters read with a key.",
            Safety::Unsafe,
            edits,
        ));
    }
}

fn numeric_group(pattern: &str) -> bool {
    static GROUP: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?:^|[^\\])\((?:\?(?:P?<\w+>|'\w+'))?(?:(?:\\d|\[0-9\])(?:[+*?]|\{\d+(?:,\d*)?\})?)+\)")
            .unwrap()
    });
    GROUP.is_match(pattern)
}

fn is_array(facts: &Facts, expr: &Expression<'_>) -> bool {
    if matches!(
        unparenthesize(expr),
        Expression::Array(_) | Expression::LegacyArray(_)
    ) {
        return true;
    }
    type_of(facts, expr).is_some_and(|t| {
        !t.types.is_empty() && t.types.iter().all(|a| matches!(a, TAtomic::Array(_)))
    })
}

fn is_accessed_array_dim_fetch(
    facts: &Facts,
    body: Node<'_, '_>,
    name: &[u8],
    known: bool,
) -> bool {
    let mut assigned: Vec<usize> = Vec::new();
    let mut initial: Option<&TUnion> = None;
    let mut seen = false;
    let mut targets: Vec<Span> = Vec::new();
    let mut asserted: Vec<usize> = Vec::new();
    traverse(body, &mut |node| {
        if let Node::MethodCall(c) = node
            && let ClassLikeMemberSelector::Identifier(method) = &c.method
            && c.argument_list
                .arguments
                .iter()
                .any(|a| is_variable(value(a), name))
            && type_of(facts, c.object).is_some_and(|t| asserts(facts, t, method.value))
        {
            asserted.push(node_span(c).end);
        }
        if let Node::Assignment(a) = node
            && is_variable(a.lhs, name)
        {
            assigned.push(node_span(a).end);
            targets.push(node_span(unparenthesize(a.lhs)));
        }
        if let Node::Expression(e) = node
            && !seen
            && is_variable(e, name)
            && !targets.contains(&node_span(e))
        {
            seen = true;
            initial = type_of(facts, e);
        }
        Flow::Continue
    });
    let mut guarded: Vec<Span> = Vec::new();
    let mut accessed = false;
    traverse(body, &mut |node| {
        match node {
            Node::Binary(b) if matches!(b.operator, BinaryOperator::NullCoalesce(_)) => {
                guarded.push(node_span(unparenthesize(b.lhs)));
            }
            Node::IssetConstruct(i) => {
                guarded.extend(i.values.iter().map(|v| node_span(unparenthesize(v))));
            }
            Node::EmptyConstruct(e) => guarded.push(node_span(unparenthesize(e.value))),
            _ => {}
        }
        if should_stop(node, name) {
            accessed = false;
            return Flow::Stop;
        }
        let Node::ArrayAccess(access) = node else {
            return Flow::Continue;
        };
        if !is_variable(access.array, name) {
            return Flow::Continue;
        }
        let mut variable = type_of(facts, access.array);
        let at = node_span(access).start;
        let unassigned = !assigned.iter().any(|&end| end <= at);
        let narrowed;
        if known && unassigned && !asserted.iter().any(|&end| end <= at) {
            let here = variable;
            variable = initial;
            let has_null = |t: &TUnion| t.types.iter().any(|a| matches!(a, TAtomic::Null));
            if let (Some(i), Some(h)) = (initial, here)
                && i.types.len() > 1
                && has_null(i)
                && !has_null(h)
            {
                narrowed = TUnion::from_vec(
                    i.types
                        .iter()
                        .filter(|a| !matches!(a, TAtomic::Null))
                        .cloned()
                        .collect(),
                );
                variable = Some(&narrowed);
            }
        }
        if !known
            && unassigned
            && variable.is_some_and(|t| t.types.iter().all(|a| matches!(a, TAtomic::Array(_))))
        {
            variable = None;
        }
        let without_null;
        if guarded.contains(&node_span(access))
            && let Some(t) = variable
            && t.types.len() > 1
            && t.types.iter().any(|a| matches!(a, TAtomic::Null))
        {
            without_null = TUnion::from_vec(
                t.types
                    .iter()
                    .filter(|a| !matches!(a, TAtomic::Null))
                    .cloned()
                    .collect(),
            );
            variable = Some(&without_null);
        }
        if variable.is_none() && known {
            accessed = false;
            return Flow::Stop;
        }
        if variable.is_some_and(is_string) {
            accessed = false;
            return Flow::Stop;
        }
        let integer = type_of(facts, access.index).is_none_or(is_integer);
        if integer && variable.is_none_or(maybe_string) {
            return Flow::Continue;
        }
        if variable.is_some_and(|t| is_union(t) || is_array_access(facts, t)) {
            accessed = false;
            return Flow::Stop;
        }
        accessed = true;
        Flow::Continue
    });
    accessed
}

fn asserts(facts: &Facts, t: &TUnion, method: &[u8]) -> bool {
    let [TAtomic::Object(TObject::Named(object))] = t.types.as_ref() else {
        return false;
    };
    facts
        .documented()
        .get_declaring_method(object.name.as_bytes(), method)
        .is_some_and(|m| !m.assertions.is_empty())
}

fn should_stop(node: Node<'_, '_>, name: &[u8]) -> bool {
    let checked: Option<&Expression<'_>> = match node {
        Node::FunctionCall(call)
            if ["is_array", "is_string", "is_int", "is_bool", "is_float"]
                .iter()
                .any(|n| named(call, n)) =>
        {
            call.argument_list.arguments.first().map(value)
        }
        Node::ExpressionStatement(statement) => Some(statement.expression),
        Node::Binary(binary) if matches!(binary.operator, BinaryOperator::NullCoalesce(_)) => {
            Some(binary.lhs)
        }
        Node::Assignment(assign) if matches!(assign.operator, AssignmentOperator::Coalesce(_)) => {
            Some(assign.lhs)
        }
        _ => None,
    };
    if let Some(checked) = checked.map(unparenthesize) {
        if let Expression::Call(Call::Method(call)) = checked
            && is_variable(call.object, name)
        {
            return true;
        }
        if is_variable(checked, name) {
            return true;
        }
    }
    match node {
        Node::EmptyConstruct(empty) => is_variable(empty.value, name),
        Node::Echo(echo) => echo.values.iter().any(|v| is_variable(v, name)),
        Node::UnaryPrefix(cast) if matches!(cast.operator, UnaryPrefixOperator::ArrayCast(..)) => {
            is_variable(cast.operand, name)
        }
        Node::PropertyAccess(fetch) => is_dim_fetch_on(fetch.object, name),
        Node::StaticPropertyAccess(fetch) => is_dim_fetch_on(fetch.class, name),
        Node::Binary(binary) if matches!(binary.operator, BinaryOperator::Instanceof(_)) => {
            is_variable(binary.lhs, name)
        }
        Node::Assignment(assign)
            if matches!(assign.operator, AssignmentOperator::Assign(_))
                && is_variable(assign.lhs, name) =>
        {
            call_args(assign.rhs).is_some_and(|args| {
                args.iter()
                    .any(|a| matches!(a, Argument::Positional(p) if p.ellipsis.is_none() && is_variable(p.value, name))
                        || matches!(a, Argument::Named(n) if is_variable(n.value, name)))
            })
        }
        _ => false,
    }
}

fn call_args<'a>(expr: &'a Expression<'a>) -> Option<Vec<&'a Argument<'a>>> {
    let list = match unparenthesize(expr) {
        Expression::Call(Call::Function(c)) => &c.argument_list,
        Expression::Call(Call::Method(c)) => &c.argument_list,
        Expression::Call(Call::NullSafeMethod(c)) => &c.argument_list,
        Expression::Call(Call::StaticMethod(c)) => &c.argument_list,
        Expression::Instantiation(Instantiation {
            argument_list: Some(list),
            ..
        }) => list,
        _ => return None,
    };
    Some(list.arguments.iter().collect())
}

fn is_dim_fetch_on(expr: &Expression<'_>, name: &[u8]) -> bool {
    matches!(unparenthesize(expr), Expression::ArrayAccess(a) if is_variable(a.array, name))
}

pub(crate) fn is_variable(expr: &Expression<'_>, name: &[u8]) -> bool {
    matches!(unparenthesize(expr), Expression::Variable(Variable::Direct(v)) if v.name == name)
}

fn is_string(t: &TUnion) -> bool {
    !t.types.is_empty()
        && t.types
            .iter()
            .all(|a| matches!(a, TAtomic::Scalar(s) if s.is_any_string()))
}

fn is_integer(t: &TUnion) -> bool {
    !t.types.is_empty()
        && t.types
            .iter()
            .all(|a| matches!(a, TAtomic::Scalar(s) if s.is_int()))
}

fn maybe_string(t: &TUnion) -> bool {
    !is_string(t)
        && (t.types.is_empty()
            || t.types.iter().any(|a| match a {
                TAtomic::Mixed(_) | TAtomic::Callable(_) => true,
                TAtomic::Scalar(s) => {
                    s.is_any_string()
                        || matches!(s, TScalar::Generic | TScalar::ArrayKey | TScalar::Numeric)
                }
                _ => false,
            }))
}

fn is_union(t: &TUnion) -> bool {
    if t.types.iter().any(|a| matches!(a, TAtomic::Mixed(_))) {
        return false;
    }
    let count = |f: fn(&TAtomic) -> bool| t.types.iter().filter(|a| f(a)).count();
    let bools = count(|a| matches!(a, TAtomic::Scalar(TScalar::Bool(_))));
    let arrays = count(|a| matches!(a, TAtomic::Array(_)));
    if arrays == t.types.len() {
        let arrays: Vec<&TArray> = (t.types.iter())
            .filter_map(|a| match a {
                TAtomic::Array(array) => Some(array),
                _ => None,
            })
            .collect();
        return constant_arrays_are_union(&arrays);
    }
    t.types.len() - bools.saturating_sub(1) - arrays.saturating_sub(1) > 1
}

fn constant_arrays_are_union(arrays: &[&TArray]) -> bool {
    let mut items: Vec<&(bool, TUnion)> = Vec::new();
    for array in arrays {
        let own: Vec<&(bool, TUnion)> = match array {
            TArray::Keyed(k) if k.parameters.is_none() => {
                k.known_items.iter().flat_map(|i| i.values()).collect()
            }
            TArray::List(l) if l.element_type.is_never() => {
                l.known_elements.iter().flat_map(|i| i.values()).collect()
            }
            _ => return false,
        };
        if !own.is_empty() && own.iter().all(|(optional, _)| *optional) {
            return true;
        }
        items.extend(own);
    }
    let values: Vec<&TUnion> = items.iter().map(|(_, v)| v).collect();
    let mut kinds: Vec<String> = Vec::new();
    for v in values {
        for a in v.types.iter() {
            let kind = match a {
                TAtomic::Scalar(s) if s.is_literal_value() && !s.is_bool() => {
                    a.get_id().to_string()
                }
                TAtomic::Scalar(s) if s.is_int() => "int".to_string(),
                TAtomic::Scalar(s) if s.is_float() => "float".to_string(),
                TAtomic::Scalar(s) if s.is_any_string() => "string".to_string(),
                TAtomic::Scalar(s) if s.is_bool() => "bool".to_string(),
                TAtomic::Array(_) => "array".to_string(),
                other => other.get_id().to_string(),
            };
            if !kinds.contains(&kind) {
                kinds.push(kind);
            }
        }
    }
    kinds.len() > 1
}

fn is_called_with_array_access_arg(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    method: &[u8],
    position: usize,
) -> bool {
    let Some(class) = ancestors.iter().rev().find(|a| {
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
    if !matches!(class, Node::Class(_) | Node::AnonymousClass(_)) {
        return false;
    }
    let mut found = false;
    walk(*class, &mut |node| {
        let (selector, list) = match node {
            Node::MethodCall(call) => (&call.method, &call.argument_list),
            Node::StaticMethodCall(call) => (&call.method, &call.argument_list),
            _ => return,
        };
        if let ClassLikeMemberSelector::Identifier(id) = selector
            && id.value.eq_ignore_ascii_case(method)
            && let Some(arg) = list.arguments.get(position)
            && type_of(facts, value(arg)).is_some_and(|t| is_array_access(facts, t))
        {
            found = true;
        }
    });
    found
}

fn is_array_access(facts: &Facts, t: &TUnion) -> bool {
    let [TAtomic::Object(TObject::Named(named))] = t.types.as_ref() else {
        return false;
    };
    let name = named.name.as_bytes();
    name.eq_ignore_ascii_case(b"ArrayAccess")
        || facts.codebase.get_class_like(name).is_some_and(|c| {
            c.all_parent_interfaces
                .iter()
                .any(|i| i.as_bytes().eq_ignore_ascii_case(b"ArrayAccess"))
        })
}
