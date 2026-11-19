use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::*;
use pun_problem::{Edit, Span};
use std::fmt::Write as _;

use crate::rules::support::docblocks::doc_comment;
use crate::rules::support::expressions::named;
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::rewrites::remove_items;
use crate::rules::support::statement_edits::{block_list, braced, line_indent};
use crate::rules::support::values::{Value, value};
use crate::rules::types::facts::Facts;
use crate::rules::types::{MAGIC, namespace_of};
use crate::syntax::{node_span, unparenthesize};

pub(crate) struct ClassInfo<'ast, 'arena> {
    pub node: Node<'ast, 'arena>,
    pub members: &'ast Sequence<'arena, ClassLikeMember<'arena>>,
    pub left_brace: mago_span::Span,
    pub right_brace: mago_span::Span,
    pub name: String,
    pub is_anonymous: bool,
    pub is_final: bool,
    pub extends: Option<String>,
}

pub(crate) fn class_info<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    ancestors: &[Node<'ast, 'arena>],
    facts: &Facts,
) -> Option<ClassInfo<'ast, 'arena>> {
    let mut info = None;
    with_class_info(node, ancestors, facts, |c| info = Some(c));
    info
}

pub(crate) fn with_class_info<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    ancestors: &[Node<'ast, 'arena>],
    facts: &Facts,
    mut visit: impl FnMut(ClassInfo<'ast, 'arena>),
) {
    let extended = |extends: Option<&Extends<'_>>| {
        extends
            .and_then(|e| e.types.first())
            .map(|id| resolve_class_name(ancestors, id))
    };
    if let Node::Class(c) = node {
        visit(ClassInfo {
            node,
            members: &c.members,
            left_brace: c.left_brace,
            right_brace: c.right_brace,
            name: qualified(ancestors, c.name.value),
            is_anonymous: false,
            is_final: c.modifiers.iter().any(|m| matches!(m, Modifier::Final(_))),
            extends: extended(c.extends.as_ref()),
        });
    }
    if let Node::AnonymousClass(c) = node {
        visit(ClassInfo {
            node,
            members: &c.members,
            left_brace: c.left_brace,
            right_brace: c.right_brace,
            name: facts.anonymous_class_name(mago_span::HasSpan::span(c)),
            is_anonymous: true,
            is_final: c.modifiers.iter().any(|m| matches!(m, Modifier::Final(_))),
            extends: extended(c.extends.as_ref()),
        });
    }
}

pub(crate) fn qualified(ancestors: &[Node<'_, '_>], name: &[u8]) -> String {
    let name = String::from_utf8_lossy(name);
    match namespace_of(ancestors) {
        Some(ns) => format!("{ns}\\{name}"),
        None => name.into_owned(),
    }
}

pub(crate) fn has_modifier(method: &Method<'_>, f: fn(&Modifier<'_>) -> bool) -> bool {
    method.modifiers.iter().any(f)
}

pub(crate) fn is_private(method: &Method<'_>) -> bool {
    has_modifier(method, |m| matches!(m, Modifier::Private(_)))
}

pub(crate) fn is_protected(method: &Method<'_>) -> bool {
    has_modifier(method, |m| matches!(m, Modifier::Protected(_)))
}

pub(crate) fn is_public(method: &Method<'_>) -> bool {
    !is_private(method) && !is_protected(method)
}

pub(crate) fn is_final(method: &Method<'_>) -> bool {
    has_modifier(method, |m| matches!(m, Modifier::Final(_)))
}

pub(crate) fn is_static(method: &Method<'_>) -> bool {
    has_modifier(method, |m| matches!(m, Modifier::Static(_)))
}

pub(crate) fn is_magic(name: &[u8]) -> bool {
    MAGIC
        .iter()
        .any(|m| m.as_bytes().eq_ignore_ascii_case(name))
}

pub(crate) fn member_doc<'s>(
    source: &'s str,
    class: &ClassInfo<'_, '_>,
    index: usize,
) -> Option<&'s str> {
    let member = class.members.get(index)?;
    let from = member_from(class.members, class.left_brace, index);
    doc_comment(source, from, node_span(member).start).map(|d| &source[d.start..d.end])
}

pub(crate) fn doc_tags(doc: &str) -> Vec<&str> {
    doc.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let line = line.strip_prefix("/**").unwrap_or(line);
            let line = line.strip_prefix('*').unwrap_or(line).trim_start();
            let rest = line.strip_prefix('@')?;
            let len = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '\\' | ':')))
                .unwrap_or(rest.len());
            (len > 0).then(|| &line[..=len])
        })
        .collect()
}

const KNOWN_TAGS: &[&str] = &[
    "@param",
    "@phpstan-param",
    "@psalm-param",
    "@phan-param",
    "@param-immediately-invoked-callable",
    "@phpstan-param-immediately-invoked-callable",
    "@param-later-invoked-callable",
    "@phpstan-param-later-invoked-callable",
    "@param-closure-this",
    "@phpstan-param-closure-this",
    "@pure-unless-callable-is-impure",
    "@phpstan-pure-unless-callable-is-impure",
    "@pure-unless-parameter-passed",
    "@phpstan-pure-unless-parameter-passed",
    "@var",
    "@phpstan-var",
    "@psalm-var",
    "@phan-var",
    "@return",
    "@phpstan-return",
    "@psalm-return",
    "@phan-return",
    "@phan-real-return",
    "@throws",
    "@phpstan-throws",
    "@mixin",
    "@phan-mixin",
    "@psalm-require-extends",
    "@phpstan-require-extends",
    "@psalm-require-implements",
    "@phpstan-require-implements",
    "@psalm-inheritors",
    "@phpstan-sealed",
    "@deprecated",
    "@property",
    "@property-read",
    "@property-write",
    "@phpstan-property",
    "@phpstan-property-read",
    "@phpstan-property-write",
    "@psalm-property",
    "@psalm-property-read",
    "@psalm-property-write",
    "@phan-property",
    "@phan-property-read",
    "@phan-property-write",
    "@method",
    "@phpstan-method",
    "@psalm-method",
    "@phan-method",
    "@template",
    "@phpstan-template",
    "@psalm-template",
    "@phan-template",
    "@template-covariant",
    "@phpstan-template-covariant",
    "@psalm-template-covariant",
    "@template-contravariant",
    "@phpstan-template-contravariant",
    "@psalm-template-contravariant",
    "@extends",
    "@phpstan-extends",
    "@phan-extends",
    "@phan-inherits",
    "@template-extends",
    "@implements",
    "@phpstan-implements",
    "@template-implements",
    "@use",
    "@phpstan-use",
    "@template-use",
    "@phpstan-type",
    "@psalm-type",
    "@phan-type",
    "@phpstan-import-type",
    "@psalm-import-type",
    "@phpstan-assert",
    "@phpstan-assert-if-true",
    "@phpstan-assert-if-false",
    "@psalm-assert",
    "@psalm-assert-if-true",
    "@psalm-assert-if-false",
    "@phan-assert",
    "@phan-assert-if-true",
    "@phan-assert-if-false",
    "@phpstan-this-out",
    "@phpstan-self-out",
    "@psalm-this-out",
    "@psalm-self-out",
    "@param-out",
    "@phpstan-param-out",
    "@psalm-param-out",
];

pub(crate) fn is_controller_class_method(
    source: &str,
    class: &ClassInfo<'_, '_>,
    method: &Method<'_>,
    index: usize,
) -> bool {
    if !is_public(method) {
        return false;
    }
    if !class
        .extends
        .as_ref()
        .is_some_and(|p| p.ends_with("Controller") || p.ends_with("Presenter"))
    {
        return false;
    }
    member_doc(source, class, index)
        .is_some_and(|doc| doc_tags(doc).iter().any(|t| !KNOWN_TAGS.contains(t)))
}

pub(crate) fn is_named_constructor(class: &ClassInfo<'_, '_>, method: &Method<'_>) -> bool {
    if !method.name.value.eq_ignore_ascii_case(b"__construct") {
        return false;
    }
    if is_private(method) {
        return true;
    }
    if class.is_final {
        return false;
    }
    is_protected(method)
}

pub(crate) fn has_parent_method_or_interface_method(
    facts: &Facts,
    class: &ClassInfo<'_, '_>,
    method: &[u8],
) -> bool {
    let codebase = &facts.codebase;
    codebase
        .get_class_like(class.name.as_bytes())
        .is_some_and(|own| {
            own.all_parent_classes.iter().any(|p| {
                codebase.method_exists(p.as_bytes(), method)
                    || codebase.method_exists(p.as_bytes(), b"__call")
            }) || own
                .all_parent_interfaces
                .iter()
                .any(|i| codebase.method_exists(i.as_bytes(), method))
        })
}

pub(crate) fn walk<'ast, 'arena>(
    root: Node<'ast, 'arena>,
    f: &mut dyn FnMut(Node<'ast, 'arena>, &[Node<'ast, 'arena>]) -> bool,
) {
    let mut path: Vec<Node<'ast, 'arena>> = Vec::new();
    let mut stack = vec![(root, 0)];
    while let Some((node, depth)) = stack.pop() {
        path.truncate(depth);
        if !f(node, &path) {
            continue;
        }
        path.push(node);
        let mut children = Vec::new();
        node.visit_children(|child| children.push(child));
        stack.extend(children.into_iter().rev().map(|c| (c, depth + 1)));
    }
}

fn scope_of(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    path: &[Node<'_, '_>],
) -> (String, Option<String>) {
    let inner = path
        .iter()
        .enumerate()
        .skip(1)
        .rev()
        .find_map(|(i, n)| match n {
            Node::AnonymousClass(c) => Some(Ok(*c)),
            Node::Class(c) => Some(Err((i, *c))),
            _ => None,
        });
    let inner = match inner {
        Some(Err((i, c))) => {
            let around = [ancestors, &path[..i]].concat();
            return (
                qualified(&around, c.name.value),
                c.extends
                    .as_ref()
                    .and_then(|e| e.types.first())
                    .map(|id| resolve_class_name(&around, id)),
            );
        }
        Some(Ok(c)) => Some(c),
        None => None,
    };
    match inner {
        Some(c) => (
            facts.anonymous_class_name(mago_span::HasSpan::span(c)),
            (c.extends.as_ref())
                .and_then(|e| e.types.first())
                .map(|id| resolve_class_name(ancestors, id)),
        ),
        None => (class.name.clone(), class.extends.clone()),
    }
}

fn this_class(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    path: &[Node<'_, '_>],
    object: &Expression<'_>,
) -> Option<String> {
    let Expression::Variable(Variable::Direct(var)) = unparenthesize(object) else {
        return None;
    };
    if var.name != b"$this" {
        return None;
    }
    let scope = path
        .iter()
        .rposition(|n| matches!(n, Node::AnonymousClass(_)))
        .map_or(path, |i| &path[i + 1..]);
    let undefined = scope.iter().any(|n| match n {
        Node::Method(m) => m.modifiers.contains_static(),
        Node::Closure(c) => c.r#static.is_some(),
        Node::ArrowFunction(f) => f.r#static.is_some(),
        _ => false,
    });
    (!undefined).then(|| scope_of(facts, ancestors, class, path).0)
}

struct CallerType {
    names: Vec<String>,
    never: bool,
    mixed: bool,
}

impl CallerType {
    fn of(t: Option<&TUnion>) -> CallerType {
        CallerType {
            names: t.map_or_default(object_class_names),
            never: t.is_some_and(mago_codex::ttype::union::TUnion::is_never),
            mixed: t.is_none_or(mago_codex::ttype::union::TUnion::is_mixed),
        }
    }

    fn named(name: String) -> CallerType {
        CallerType {
            names: vec![name],
            never: false,
            mixed: false,
        }
    }
}

pub(crate) fn type_of<'f>(facts: &'f Facts, expr: &Expression<'_>) -> Option<&'f TUnion> {
    let span = node_span(unparenthesize(expr));
    facts.full_type(span).or_else(|| facts.native_type(span))
}

pub(crate) fn object_class_names(t: &TUnion) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for atomic in t.types.iter() {
        let name = match atomic {
            TAtomic::Object(TObject::Named(n)) => {
                String::from_utf8_lossy(n.name.as_bytes()).into_owned()
            }
            TAtomic::Object(TObject::Enum(e)) => {
                String::from_utf8_lossy(e.get_name().as_bytes()).into_owned()
            }
            _ => continue,
        };
        if !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

fn selector_name<'a>(selector: &ClassLikeMemberSelector<'a>) -> Option<&'a [u8]> {
    match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    }
}

fn should_skip(selector: &ClassLikeMemberSelector<'_>, method: &[u8]) -> bool {
    selector_name(selector).is_none_or(|n| n.eq_ignore_ascii_case(method))
}

enum CallSite<'ast, 'arena> {
    Object(
        &'ast Expression<'arena>,
        &'ast ClassLikeMemberSelector<'arena>,
    ),
    Static(
        &'ast Expression<'arena>,
        &'ast ClassLikeMemberSelector<'arena>,
    ),
}

fn call_site<'ast, 'arena>(node: Node<'ast, 'arena>) -> Option<CallSite<'ast, 'arena>> {
    Some(match node {
        Node::MethodCall(c) => CallSite::Object(c.object, &c.method),
        Node::NullSafeMethodCall(c) => CallSite::Object(c.object, &c.method),
        Node::MethodPartialApplication(c) => CallSite::Object(c.object, &c.method),
        Node::StaticMethodCall(c) => CallSite::Static(c.class, &c.method),
        Node::StaticMethodPartialApplication(c) => CallSite::Static(c.class, &c.method),
        _ => return None,
    })
}

pub(crate) fn is_class_method_used(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    method: &[u8],
) -> bool {
    let mut used = false;
    walk(class.node, &mut |node, path| {
        if used {
            return false;
        }
        if let Some(site) = call_site(node) {
            used = is_call_of(facts, ancestors, class, path, &site, method);
        } else if let Node::Array(Array { elements, .. })
        | Node::LegacyArray(LegacyArray { elements, .. }) = node
        {
            used = is_array_call_of(facts, ancestors, class, elements, path, method);
        }
        !used
    });
    used || does_method_exist_in_trait(facts, class, method)
}

fn is_call_of(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    path: &[Node<'_, '_>],
    site: &CallSite<'_, '_>,
    method: &[u8],
) -> bool {
    // Each way below to a use names the method, in the call or in its chain of calls: a call that names it nowhere
    // needs no types.
    let (CallSite::Object(_, selector) | CallSite::Static(_, selector)) = site;
    let in_chain = |object: &Expression<'_>| {
        let mut current = unparenthesize(object);
        while let Expression::Call(Call::Method(call)) = current {
            if selector_name(&call.method).unwrap_or(b"") == method {
                return true;
            }
            current = unparenthesize(call.object);
        }
        false
    };
    if !should_skip(selector, method)
        && !matches!(site, CallSite::Object(object, _) if in_chain(object))
    {
        return false;
    }
    let (selector, caller, root, self_static) = match site {
        CallSite::Object(object, selector) => {
            let t = type_of(facts, object);
            let caller = match this_class(facts, ancestors, class, path, object) {
                Some(name) if t.is_none() => CallerType::named(name),
                _ => CallerType::of(t),
            };
            (*selector, caller, Some(*object), false)
        }
        CallSite::Static(class_expr, selector) => {
            let (caller, self_static) = static_caller(facts, ancestors, class, path, class_expr);
            (*selector, caller, None, self_static)
        }
    };
    if caller.names.len() != 1 {
        if caller.never && should_skip(selector, method) {
            return true;
        }
        if caller.mixed
            && let Some(root) = root
        {
            let mut names: Vec<&[u8]> = vec![selector_name(selector).unwrap_or(b"")];
            let mut current = unparenthesize(root);
            while let Expression::Call(Call::Method(call)) = current {
                names.push(selector_name(&call.method).unwrap_or(b""));
                if let Expression::Variable(Variable::Direct(var)) = unparenthesize(call.object)
                    && var.name == b"$this"
                {
                    return names.contains(&method);
                }
                current = unparenthesize(call.object);
            }
        }
        return false;
    }
    if self_static && should_skip(selector, method) {
        return true;
    }
    caller.names[0] == class.name && should_skip(selector, method)
}

fn static_caller(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    path: &[Node<'_, '_>],
    class_expr: &Expression<'_>,
) -> (CallerType, bool) {
    match class_expr {
        Expression::Self_(k) | Expression::Static(k) => {
            let (name, _) = scope_of(facts, ancestors, class, path);
            let exact = k.value == b"self" || k.value == b"static";
            if !exact {
                return (
                    CallerType::named(String::from_utf8_lossy(k.value).into_owned()),
                    false,
                );
            }
            (CallerType::named(name), true)
        }
        Expression::Parent(_) => {
            let (name, parent) = scope_of(facts, ancestors, class, path);
            let parents: Vec<String> = facts.codebase.get_class_like(name.as_bytes()).map_or_else(
                || parent.into_iter().collect(),
                |c| {
                    c.all_parent_classes
                        .iter()
                        .map(|p| String::from_utf8_lossy(p.as_bytes()).into_owned())
                        .collect()
                },
            );
            let caller = CallerType {
                mixed: parents.is_empty(),
                names: parents,
                never: false,
            };
            (caller, false)
        }
        Expression::Identifier(id) => (CallerType::named(resolve_class_name(ancestors, id)), false),
        other => (CallerType::of(type_of(facts, other)), false),
    }
}

fn is_array_call_of(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    elements: &TokenSeparatedSequence<'_, ArrayElement<'_>>,
    path: &[Node<'_, '_>],
    method: &[u8],
) -> bool {
    match match_array_callable(facts, ancestors, class, elements, path, method) {
        Some(ArrayCallable::Dynamic) => true,
        Some(ArrayCallable::Method {
            class: c,
            method: m,
        }) => c.eq_ignore_ascii_case(&class.name) && m.eq_ignore_ascii_case(method),
        None => false,
    }
}

pub(crate) fn is_array_callable(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    elements: &TokenSeparatedSequence<'_, ArrayElement<'_>>,
    path: &[Node<'_, '_>],
    method: &[u8],
) -> bool {
    match_array_callable(facts, ancestors, class, elements, path, method).is_some()
}

enum ArrayCallable {
    Dynamic,
    Method { class: String, method: Vec<u8> },
}

enum ItemValue {
    Constant,
    Object,
    Unknown,
}

fn match_array_callable(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    elements: &TokenSeparatedSequence<'_, ArrayElement<'_>>,
    path: &[Node<'_, '_>],
    method: &[u8],
) -> Option<ArrayCallable> {
    let items: Vec<&ArrayElement<'_>> = elements.iter().collect();
    let [first, second] = items[..] else {
        return None;
    };
    let (first_key, first_value) = element_parts(first)?;
    let (second_key, second_value) = element_parts(second)?;
    let caller = caller_class(facts, ancestors, class, path, first_value, method)?;
    if path.iter().any(|n| matches!(n, Node::Attribute(_))) {
        return None;
    }
    let mut keys: Vec<Vec<u8>> = Vec::new();
    for (index, (key, item)) in [(first_key, first_value), (second_key, second_value)]
        .into_iter()
        .enumerate()
    {
        let key = match key {
            None => index.to_string().into_bytes(),
            Some(Expression::Literal(Literal::Integer(i))) => i.value?.to_string().into_bytes(),
            Some(Expression::Literal(Literal::String(s))) => s.value?.to_vec(),
            Some(_) => return Some(ArrayCallable::Dynamic),
        };
        match item_value(facts, item) {
            ItemValue::Constant => keys.push(key),
            ItemValue::Object => {}
            ItemValue::Unknown => return Some(ArrayCallable::Dynamic),
        }
    }
    let keys: Vec<&[u8]> = keys.iter().map(Vec::as_slice).collect();
    if keys != [b"0".as_slice(), b"1"] && keys != [b"1".as_slice()] {
        return None;
    }
    let Expression::Literal(Literal::String(name)) = second_value else {
        return None;
    };
    let name = name.value?;
    if is_callback_of(path, &["register_shutdown_function", "forward_static_call"]) {
        return None;
    }
    if name == b"__construct" {
        return None;
    }
    if !facts.codebase.method_exists(caller.as_bytes(), name) {
        return None;
    }
    Some(ArrayCallable::Method {
        class: caller,
        method: name.to_vec(),
    })
}

pub(crate) fn element_parts<'a, 'b>(
    e: &'a ArrayElement<'b>,
) -> Option<(Option<&'a Expression<'b>>, &'a Expression<'b>)> {
    match e {
        ArrayElement::KeyValue(kv) => Some((Some(kv.key), kv.value)),
        ArrayElement::Value(ValueArrayElement { value, .. })
        | ArrayElement::Variadic(VariadicArrayElement { value, .. }) => Some((None, value)),
        ArrayElement::Missing(_) => None,
    }
}

fn item_value(facts: &Facts, item: &Expression<'_>) -> ItemValue {
    match unparenthesize(item) {
        Expression::Literal(_) => return ItemValue::Constant,
        Expression::Access(Access::ClassConstant(c))
            if matches!(
                c.class,
                Expression::Identifier(_)
                    | Expression::Self_(_)
                    | Expression::Static(_)
                    | Expression::Parent(_)
            ) && matches!(&c.constant, ClassLikeConstantSelector::Identifier(id) if id.value == b"class") =>
        {
            return ItemValue::Constant;
        }
        _ => {}
    }
    let t = type_of(facts, item);
    let constant = t.is_some_and(|t| {
        matches!(&t.types[..], [TAtomic::Null])
            || matches!(&t.types[..], [TAtomic::Scalar(s)] if s.is_literal_value())
    });
    if constant {
        ItemValue::Constant
    } else if t.is_some_and(|t| object_class_names(t).len() == 1) {
        ItemValue::Object
    } else {
        ItemValue::Unknown
    }
}

fn is_callback_of(path: &[Node<'_, '_>], names: &[&str]) -> bool {
    let call = path.iter().rev().find(|n| {
        !matches!(
            n,
            Node::Expression(_)
                | Node::Argument(_)
                | Node::PositionalArgument(_)
                | Node::NamedArgument(_)
                | Node::ArgumentList(_)
                | Node::Call(_)
        )
    });
    let Some(Node::FunctionCall(call)) = call else {
        return false;
    };
    let Expression::Identifier(id) = call.function else {
        return false;
    };
    let written = String::from_utf8_lossy(id.value());
    names.contains(&written.trim_start_matches('\\'))
}

fn caller_class(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    path: &[Node<'_, '_>],
    expr: &Expression<'_>,
    method: &[u8],
) -> Option<String> {
    let reference = match unparenthesize(expr) {
        Expression::Access(Access::ClassConstant(c)) => {
            if !matches!(&c.constant, ClassLikeConstantSelector::Identifier(id) if id.value == b"class")
            {
                return None;
            }
            match c.class {
                Expression::Self_(_) | Expression::Static(_) => {
                    scope_of(facts, ancestors, class, path).0
                }
                Expression::Parent(_) => scope_of(facts, ancestors, class, path).1?,
                Expression::Identifier(name) => resolve_class_name(ancestors, name),
                _ => return None,
            }
        }
        Expression::MagicConstant(MagicConstant::Class(_)) => {
            scope_of(facts, ancestors, class, path).0
        }
        other => {
            let t = type_of(facts, other)?;
            let [atomic] = &t.types[..] else {
                return None;
            };
            return match atomic {
                TAtomic::Object(TObject::Named(n)) => {
                    Some(String::from_utf8_lossy(n.name.as_bytes()).into_owned())
                }
                TAtomic::Object(TObject::Enum(e)) => {
                    Some(String::from_utf8_lossy(e.get_name().as_bytes()).into_owned())
                }
                _ => None,
            };
        }
    };
    let codebase = &facts.codebase;
    let known = codebase.get_class_like(reference.as_bytes())?;
    if !codebase.method_exists(reference.as_bytes(), b"__construct")
        || known
            .appearing_method_ids
            .keys()
            .any(|m| m.as_bytes().eq_ignore_ascii_case(method))
    {
        return Some(reference);
    }
    let constructor = codebase.get_method(reference.as_bytes(), b"__construct")?;
    constructor
        .parameters
        .iter()
        .all(|p| p.flags.has_default())
        .then_some(reference)
}

fn does_method_exist_in_trait(facts: &Facts, class: &ClassInfo<'_, '_>, method: &[u8]) -> bool {
    let codebase = &facts.codebase;
    let mut traits: Vec<String> = Vec::new();
    let mut pending: Vec<String> = vec![class.name.clone()];
    pending.extend(
        (codebase.get_class_like(class.name.as_bytes()).into_iter())
            .flat_map(|own| own.all_parent_classes.iter())
            .map(|p| String::from_utf8_lossy(p.as_bytes()).into_owned()),
    );
    while let Some(name) = pending.pop() {
        let Some(c) = codebase.get_class_like(name.as_bytes()) else {
            continue;
        };
        for t in &c.used_traits {
            let t = String::from_utf8_lossy(t.as_bytes()).into_owned();
            if !traits.iter().any(|k| k.eq_ignore_ascii_case(&t)) {
                pending.push(t.clone());
                traits.push(t);
            }
        }
    }
    traits.iter().any(|name| {
        (codebase.get_class_like(name.as_bytes()))
            .and_then(|t| Some((t.name.to_string(), facts.source_of(t.span.file_id)?)))
            .is_some_and(|(t, source)| is_used_by_trait(&source, &t, &class.name, method))
    })
}

fn is_used_by_trait(source: &str, trait_name: &str, class_name: &str, method: &[u8]) -> bool {
    let arena = mago_allocator::LocalArena::new();
    let file = mago_database::file::File::ephemeral(
        std::borrow::Cow::Borrowed(b"trait.php"),
        std::borrow::Cow::Owned(source.as_bytes().to_vec()),
    );
    let program = mago_syntax::parser::parse_file(&arena, &file);
    let mut used = false;
    walk(Node::Program(program), &mut |node, path| {
        if used {
            return false;
        }
        let Node::Trait(t) = node else {
            return true;
        };
        if !qualified(path, t.name.value).eq_ignore_ascii_case(trait_name) {
            return false;
        }
        used = t.members.iter().any(|member| {
            let ClassLikeMember::Method(m) = member else {
                return false;
            };
            if m.name.value == method {
                return true;
            }
            let MethodBody::Concrete(body) = &m.body else {
                return false;
            };
            let mut called = false;
            for statement in &body.statements {
                walk(Node::Statement(statement), &mut |n, inner| {
                    if called || matches!(n, Node::Class(_) | Node::AnonymousClass(_) | Node::Function(_)) {
                        return false;
                    }
                    called = match call_site(n) {
                        Some(CallSite::Object(object, selector)) => {
                            matches!(object, Expression::Variable(Variable::Direct(v)) if v.name == b"$this")
                                && selector_name(selector).is_some_and(|s| s.eq_ignore_ascii_case(method))
                        }
                        Some(CallSite::Static(class_expr, selector)) => {
                            let named = match class_expr {
                                Expression::Self_(_) | Expression::Static(_) | Expression::Parent(_) => true,
                                Expression::Identifier(id) => {
                                    let mut full: Vec<Node<'_, '_>> = path.to_vec();
                                    full.extend_from_slice(inner);
                                    resolve_class_name(&full, id) == class_name
                                }
                                _ => false,
                            };
                            named && selector_name(selector).is_some_and(|s| s.eq_ignore_ascii_case(method))
                        }
                        None => false,
                    };
                    !called
                });
            }
            called
        });
        false
    });
    used
}

pub(crate) fn data_provider_methods(
    source: &str,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
) -> Vec<Vec<u8>> {
    (0..class.members.len())
        .flat_map(|index| method_data_providers(source, ancestors, class, index, true))
        .collect()
}

pub(crate) fn method_data_providers(
    source: &str,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    index: usize,
    doc_tags: bool,
) -> Vec<Vec<u8>> {
    let method_names: Vec<&[u8]> = class
        .members
        .iter()
        .filter_map(|m| match m {
            ClassLikeMember::Method(m) => Some(m.name.value),
            _ => None,
        })
        .collect();
    let mut out: Vec<Vec<u8>> = Vec::new();
    let mut add = |text: &[u8]| {
        let len = text
            .iter()
            .position(|b| !(b.is_ascii_alphanumeric() || *b == b'_'))
            .unwrap_or(text.len());
        if len == 0 {
            return;
        }
        if let Some(found) = method_names
            .iter()
            .find(|n| n.eq_ignore_ascii_case(&text[..len]))
        {
            out.push(found.to_vec());
        }
    };
    if let Some(ClassLikeMember::Method(method)) = class.members.get(index) {
        if let Some(doc) = member_doc(source, class, index).filter(|_| doc_tags) {
            for line in doc.lines() {
                let line = line.trim_start();
                let line = line.strip_prefix("/**").unwrap_or(line);
                let line = line.strip_prefix('*').unwrap_or(line).trim_start();
                if let Some(rest) = line.strip_prefix("@dataProvider")
                    && rest.starts_with([' ', '\t'])
                {
                    add(rest.trim_start().as_bytes());
                }
            }
        }
        for list in &method.attribute_lists {
            for attribute in &list.attributes {
                let name = resolve_class_name(ancestors, &attribute.name);
                let first = attribute
                    .argument_list
                    .as_ref()
                    .and_then(|l| l.arguments.first());
                if (name.eq_ignore_ascii_case("PHPUnit\\Framework\\Attributes\\DataProvider")
                    || name.eq_ignore_ascii_case("Codeception\\Attribute\\DataProvider"))
                    && let Some(
                        PartialArgument::Positional(PositionalArgument { value, .. })
                        | PartialArgument::Named(NamedArgument { value, .. }),
                    ) = first
                    && let Expression::Literal(Literal::String(s)) = value
                    && let Some(text) = s.value
                {
                    add(text);
                }
            }
        }
    }
    out
}

pub(crate) fn mapped_type_text(t: &TUnion) -> Option<String> {
    let mut texts: Vec<String> = Vec::new();
    let mut nullable = false;
    for atomic in t.types.iter() {
        let text = match atomic {
            TAtomic::Null => {
                nullable = true;
                continue;
            }
            TAtomic::Scalar(s) if s.is_int() => "int".to_string(),
            TAtomic::Scalar(s) if s.is_float() => "float".to_string(),
            TAtomic::Scalar(s) if s.is_true() => "true".to_string(),
            TAtomic::Scalar(s) if s.is_false() => "false".to_string(),
            TAtomic::Scalar(s) if s.is_bool() => "bool".to_string(),
            TAtomic::Scalar(s) if s.is_any_string() => "string".to_string(),
            TAtomic::Array(_) => "array".to_string(),
            TAtomic::Iterable(_) => "iterable".to_string(),
            TAtomic::Callable(_) => "callable".to_string(),
            TAtomic::Mixed(_) => "mixed".to_string(),
            TAtomic::Void => "void".to_string(),
            TAtomic::Never => "never".to_string(),
            TAtomic::Object(TObject::Any) => "object".to_string(),
            TAtomic::Object(TObject::Named(n)) if n.is_static => "static".to_string(),
            TAtomic::Object(TObject::Named(n)) => {
                let mut text = format!("\\{}", String::from_utf8_lossy(n.name.as_bytes()));
                for t in n.intersection_types.iter().flat_map(|v| v.iter()) {
                    if let TAtomic::Object(TObject::Named(m)) = t {
                        let _ = write!(text, "&\\{}", String::from_utf8_lossy(m.name.as_bytes()));
                    }
                }
                text
            }
            TAtomic::Object(TObject::Enum(e)) => {
                format!("\\{}", String::from_utf8_lossy(e.get_name().as_bytes()))
            }
            _ => return None,
        };
        if !texts.contains(&text) {
            texts.push(text);
        }
    }
    match &texts[..] {
        [one] if nullable && one != "mixed" => Some(format!("?{one}")),
        [one] => Some(one.clone()),
        [] if nullable => Some("null".to_string()),
        _ => None,
    }
}

pub(crate) fn hint_text(
    source: &str,
    ancestors: &[Node<'_, '_>],
    hint: &Hint<'_>,
) -> Option<String> {
    Some(match hint {
        Hint::Identifier(id) => format!("\\{}", resolve_class_name(ancestors, id)),
        Hint::Nullable(n) => format!("?{}", hint_text(source, ancestors, n.hint)?),
        Hint::Intersection(i) => format!(
            "{}&{}",
            hint_text(source, ancestors, i.left)?,
            hint_text(source, ancestors, i.right)?
        ),
        Hint::Union(_) | Hint::Parenthesized(_) => return None,
        keyword => {
            let span = node_span(keyword);
            source[span.start..span.end].to_string()
        }
    })
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ConstValue {
    Scalar(crate::rules::support::values::Value),
    EmptyArray,
}

pub(crate) fn expr_value(expr: &Expression<'_>) -> Option<ConstValue> {
    use crate::rules::support::values::{Value, value};
    match unparenthesize(expr) {
        Expression::Array(a) if a.elements.is_empty() => Some(ConstValue::EmptyArray),
        Expression::LegacyArray(a) if a.elements.is_empty() => Some(ConstValue::EmptyArray),
        e => Some(ConstValue::Scalar(match value(e)? {
            Value::Constant(name) => Value::Str(name),
            v => v,
        })),
    }
}

pub(crate) fn default_value(
    facts: &Facts,
    param: &mago_codex::metadata::parameter::FunctionLikeParameterMetadata,
) -> Option<ConstValue> {
    inferred_default(param).or_else(|| written_default(facts, param))
}

fn written_default(
    facts: &Facts,
    param: &mago_codex::metadata::parameter::FunctionLikeParameterMetadata,
) -> Option<ConstValue> {
    let span = param.get_span();
    let source = facts.source_of(span.file_id)?;
    let text = source.get(span.start.offset as usize..span.end.offset as usize)?;
    let code = format!("<?php function f({text}) {{}}");
    let arena = mago_allocator::LocalArena::new();
    let file = mago_database::file::File::ephemeral(
        std::borrow::Cow::Borrowed(b"default.php"),
        std::borrow::Cow::Owned(code.into_bytes()),
    );
    let program = mago_syntax::parser::parse_file(&arena, &file);
    let f = program.statements.iter().find_map(|s| match s {
        Statement::Function(f) => Some(f),
        _ => None,
    })?;
    let default = f
        .parameter_list
        .parameters
        .first()?
        .default_value
        .as_ref()?
        .value;
    (!matches!(unparenthesize(default), Expression::ConstantAccess(_)))
        .then(|| expr_value(default))
        .flatten()
}

fn inferred_default(
    param: &mago_codex::metadata::parameter::FunctionLikeParameterMetadata,
) -> Option<ConstValue> {
    use crate::rules::support::values::Value;
    let t = &param.get_default_type()?.type_union;
    Some(match &t.types[..] {
        [TAtomic::Null] => ConstValue::Scalar(Value::Null),
        [TAtomic::Scalar(s)] if s.is_true() => ConstValue::Scalar(Value::Bool(true)),
        [TAtomic::Scalar(s)] if s.is_false() => ConstValue::Scalar(Value::Bool(false)),
        [TAtomic::Scalar(s)] if s.is_literal_int() => {
            ConstValue::Scalar(Value::Int(s.get_literal_int_value()?))
        }
        [TAtomic::Scalar(s)] if s.is_known_literal_string() => ConstValue::Scalar(Value::Str(
            String::from_utf8(s.get_known_literal_string_value()?.to_vec()).ok()?,
        )),
        [TAtomic::Array(a)] if a.is_empty() => ConstValue::EmptyArray,
        _ => return None,
    })
}

pub(crate) fn method<'a>(
    members: &'a Sequence<'a, ClassLikeMember<'a>>,
    name: &[u8],
) -> Option<&'a Method<'a>> {
    members.iter().find_map(|m| match m {
        ClassLikeMember::Method(m) if m.name.value.eq_ignore_ascii_case(name) => Some(m),
        _ => None,
    })
}

pub(crate) fn member_from(
    members: &Sequence<'_, ClassLikeMember<'_>>,
    left_brace: mago_span::Span,
    index: usize,
) -> usize {
    match index.checked_sub(1).and_then(|i| members.get(i)) {
        Some(previous) => node_span(previous).end,
        None => left_brace.end.offset as usize,
    }
}

type IsModifier = fn(&Modifier<'_>) -> bool;

pub(crate) fn modifiers_text(modifiers: &Sequence<'_, Modifier<'_>>) -> String {
    let order: [(IsModifier, &str); 8] = [
        (|m| matches!(m, Modifier::Final(_)), "final "),
        (|m| matches!(m, Modifier::Abstract(_)), "abstract "),
        (|m| matches!(m, Modifier::Public(_)), "public "),
        (|m| matches!(m, Modifier::Protected(_)), "protected "),
        (|m| matches!(m, Modifier::Private(_)), "private "),
        (
            |m| {
                matches!(
                    m,
                    Modifier::PublicSet(_) | Modifier::ProtectedSet(_) | Modifier::PrivateSet(_)
                )
            },
            "",
        ),
        (|m| matches!(m, Modifier::Static(_)), "static "),
        (|m| matches!(m, Modifier::Readonly(_)), "readonly "),
    ];
    order
        .iter()
        .filter(|(pick, _)| modifiers.iter().any(pick))
        .map(|(_, text)| *text)
        .collect()
}

pub(crate) fn method_without_params(ctx: &crate::Context<'_>, method: &Method<'_>) -> String {
    let source = ctx.source;
    let span = node_span(method);
    let indent = line_indent(source, span.start);
    let mut out = String::new();
    for list in &method.attribute_lists {
        let s = node_span(list);
        out.push_str(&source[s.start..s.end]);
        out.push('\n');
        out.push_str(indent);
    }
    out.push_str(&modifiers_text(&method.modifiers));
    out.push_str("function ");
    if method.ampersand.is_some() {
        out.push('&');
    }
    out.push_str(std::str::from_utf8(method.name.value).unwrap_or_default());
    out.push_str("()");
    if let Some(hint) = &method.return_type_hint {
        let s = node_span(&hint.hint);
        out.push_str(": ");
        out.push_str(&source[s.start..s.end]);
    }
    match &method.body {
        MethodBody::Concrete(block) => {
            out.push('\n');
            out.push_str(indent);
            out.push_str(&braced(ctx, &block_list(source, block), indent));
        }
        MethodBody::Abstract(_) => out.push(';'),
    }
    out
}

pub(crate) fn param_used(method: &Method<'_>, param: &FunctionLikeParameter<'_>) -> bool {
    if !param.modifiers.is_empty() {
        return true;
    }
    match &method.body {
        MethodBody::Concrete(block) => uses_param(Node::Block(block), param.variable.name),
        MethodBody::Abstract(_) => false,
    }
}

fn uses_param(node: Node<'_, '_>, name: &[u8]) -> bool {
    match node {
        Node::FunctionCall(call)
            if [
                "func_get_arg",
                "func_get_args",
                "func_num_args",
                "get_defined_vars",
            ]
            .iter()
            .any(|f| named(call, f)) =>
        {
            return true;
        }
        Node::FunctionCall(call) if named(call, "compact") => {
            let names = call.argument_list.arguments.iter().any(|a| {
                matches!(value(a.value()), Some(Value::Str(s)) if s.as_bytes().eq_ignore_ascii_case(&name[1..]))
            });
            if names {
                return true;
            }
        }
        Node::Function(_)
        | Node::Class(_)
        | Node::Interface(_)
        | Node::Trait(_)
        | Node::Enum(_) => {
            return false;
        }
        Node::AnonymousClass(class) => {
            return class
                .argument_list
                .as_ref()
                .is_some_and(|args| uses_param(Node::PartialArgumentList(args), name));
        }
        Node::DirectVariable(v) => return v.name == name,
        _ => {}
    }
    let mut found = false;
    node.visit_children(|child| found = found || uses_param(child, name));
    found
}

pub(crate) fn remove_members(
    source: &str,
    members: &Sequence<'_, ClassLikeMember<'_>>,
    left_brace: mago_span::Span,
    right_brace: mago_span::Span,
    removed: &[bool],
) -> Vec<Edit> {
    let spans: Vec<Span> = members.iter().map(node_span).collect();
    let froms: Vec<usize> = (0..spans.len())
        .map(|i| member_from(members, left_brace, i))
        .collect();
    match remove_items(source, &spans, &froms, removed) {
        Some(edits) => edits,
        None => {
            let (open, close) = (
                left_brace.end.offset as usize,
                right_brace.start.offset as usize,
            );
            let indent = line_indent(source, open - 1);
            vec![Edit::new(Span::new(open, close), format!("\n{indent}"))]
        }
    }
}
