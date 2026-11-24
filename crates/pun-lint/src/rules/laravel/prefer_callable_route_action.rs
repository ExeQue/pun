//! rector-laravel 2.7.3's `RouteActionCallableRector` and the `RouterRegisterNodeAnalyzer` it asks.

use std::path::Path;

use mago_syntax::cst::{
    ArgumentList, ArrayElement, ClassLikeMemberSelector, Expression, Literal, Node, NodeKind,
};

use super::object_type::is_object_type;
use super::{class_name, is_name, member_name};
use crate::problem::{Edit, Safety, Span};
use crate::rules::refactor::configured_codemods::single_quoted;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

const DEFAULT_NAMESPACE: &str = "App\\Http\\Controllers";

/// `RouteActionCallableRector`: `namespace` is the controllers' namespace, `routes` maps a route file's path to the
/// namespace of its controllers. A path matches when it ends with the other on whole path components, so a
/// project-relative path matches the absolute path a rector.php gives.
pub struct PreferCallableRouteAction {
    pub namespace: String,
    pub routes: Vec<(String, String)>,
}

impl Default for PreferCallableRouteAction {
    fn default() -> Self {
        PreferCallableRouteAction {
            namespace: DEFAULT_NAMESPACE.into(),
            routes: Vec::new(),
        }
    }
}

impl Rule for PreferCallableRouteAction {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-callable-route-action"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        *self = PreferCallableRouteAction::default();
        let map = match options {
            serde_json::Value::Null => return Ok(()),
            serde_json::Value::Object(map) => map,
            _ => return Err("options must be a table".into()),
        };
        for (key, value) in map {
            match key.as_str() {
                "namespace" => {
                    value
                        .as_str()
                        .ok_or("`namespace` must be a string")?
                        .clone_into(&mut self.namespace);
                }
                "routes" => {
                    let routes = value.as_object().ok_or("`routes` must be a table")?;
                    self.routes = routes
                        .iter()
                        .map(|(path, ns)| match ns.as_str() {
                            Some(ns) => Ok((path.clone(), ns.to_string())),
                            None => Err(format!("`routes.{path}` must be a string")),
                        })
                        .collect::<Result<_, _>>()?;
                }
                _ => {
                    return Err(format!(
                        "unknown option `{key}`; the options are namespace, routes"
                    )
                    .into());
                }
            }
        }
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall, NodeKind::StaticMethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some((selector, list)) = call_parts(node) else {
            return;
        };
        let Some(name) = member_name(selector) else {
            return;
        };
        if is_name(name, "group") || !is_register_call(ctx, node, ancestors, name) {
            return;
        }
        let span = node_span(&node);
        let group = match group_namespace_of(span, ancestors) {
            Namespace::Unreadable => return,
            Namespace::Value(ns) => Some(ns),
            Namespace::None => None,
        };
        let position = if is_name(name, "fallback") {
            0
        } else if is_name(name, "match") {
            2
        } else {
            1
        };
        let Some(argument) = list.arguments.iter().nth(position) else {
            return;
        };
        if argument.is_unpacked() {
            return;
        }
        let Some(action) = value(argument.value()) else {
            return;
        };
        let Some(uses) = action_string(&action) else {
            return;
        };
        let segments: Vec<&str> = uses.split('@').collect();
        let [controller, method] = segments.as_slice() else {
            return;
        };
        let mut namespace = self.namespace_for(ctx.path).to_string();
        if let Some(group) = group {
            namespace = format!("{namespace}\\{group}");
        }
        let controller = if controller.starts_with('\\') {
            (*controller).to_string()
        } else {
            format!("{namespace}\\{controller}")
        };
        let class = controller.trim_start_matches('\\');
        if !has_method(ctx, class, method) {
            return;
        }
        let value_span = node_span(unparenthesize(argument.value()));
        let mut edits = vec![Edit::new(
            value_span,
            format!("[\\{class}::class, {}]", single_quoted(method)),
        )];
        let mut tail = Vec::new();
        if let Value::Array(items) = &action {
            if let Some(Value::Str(name)) = get(items, "as") {
                tail.push(format!("->name({})", single_quoted(name)));
            }
            match get(items, "middleware") {
                Some(Value::Str(m)) => {
                    tail.push(format!("->middleware({})", single_quoted(m)));
                }
                Some(Value::Array(list)) => {
                    let strings: Option<Vec<String>> = list
                        .iter()
                        .map(|(_, v)| match v {
                            Value::Str(s) => Some(single_quoted(s)),
                            _ => None,
                        })
                        .collect();
                    if let Some(strings) = strings {
                        tail.push(format!("->middleware([{}])", strings.join(", ")));
                    }
                }
                _ => {}
            }
        }
        if !tail.is_empty() {
            edits.push(Edit::new(Span::new(span.end, span.end), tail.concat()));
        }
        out.push(Report::fixed(
            value_span,
            "Use a callable array instead of the `Controller@method` route action.",
            Safety::Unsafe,
            edits,
        ));
    }
}

impl PreferCallableRouteAction {
    /// `getNamespace()`: the namespace `routes` gives the file, else `namespace`.
    fn namespace_for(&self, path: &Path) -> &str {
        let path = path.strip_prefix("./").unwrap_or(path);
        self.routes
            .iter()
            .find(|(file, _)| {
                let file = Path::new(file);
                let file = file.strip_prefix("./").unwrap_or(file);
                file.ends_with(path) || path.ends_with(file)
            })
            .map_or(&self.namespace, |(_, ns)| ns)
    }
}

fn call_parts<'ast, 'arena>(
    node: Node<'ast, 'arena>,
) -> Option<(
    &'ast ClassLikeMemberSelector<'arena>,
    &'ast ArgumentList<'arena>,
)> {
    match node {
        Node::MethodCall(m) => Some((&m.method, &m.argument_list)),
        Node::StaticMethodCall(s) => Some((&s.method, &s.argument_list)),
        _ => None,
    }
}

/// `isRegisterMethodStaticCall()`: a verb, `match`, `any` or `fallback` on a `Router` or `RouteRegistrar`, or on the
/// `Route` facade or its `Route` alias.
fn is_register_call(
    ctx: &Context<'_>,
    node: Node<'_, '_>,
    ancestors: &[Node<'_, '_>],
    name: &[u8],
) -> bool {
    const VERBS: [&str; 9] = [
        "delete", "get", "options", "patch", "post", "put", "match", "any", "fallback",
    ];
    if !VERBS.iter().any(|v| is_name(name, v)) {
        return false;
    }
    match node {
        Node::MethodCall(m) => [
            "Illuminate\\Routing\\Router",
            "Illuminate\\Routing\\RouteRegistrar",
        ]
        .iter()
        .any(|class| is_object_type(ctx, ancestors, m.object, class)),
        Node::StaticMethodCall(s) => class_name(s.class, ancestors).is_some_and(|c| {
            c.eq_ignore_ascii_case("Illuminate\\Support\\Facades\\Route")
                || c.eq_ignore_ascii_case("Route")
        }),
        _ => false,
    }
}

enum Namespace {
    None,
    Value(String),
    Unreadable,
}

/// The `laravel_route_group_namespace` Rector's `group()` calls leave on the routes in their second argument, from
/// the outermost group in: a group's own `namespace` key, after the namespace of the group around it.
fn group_namespace_of(span: Span, ancestors: &[Node<'_, '_>]) -> Namespace {
    let mut namespace = Namespace::None;
    for ancestor in ancestors {
        let Some((selector, list)) = call_parts(*ancestor) else {
            continue;
        };
        if !member_name(selector).is_some_and(|n| is_name(n, "group")) {
            continue;
        }
        let Some(body) = list.arguments.iter().nth(1) else {
            continue;
        };
        let body = node_span(body.value());
        if body.start > span.start || span.end > body.end {
            continue;
        }
        namespace = match namespace {
            Namespace::Unreadable => Namespace::Unreadable,
            Namespace::None => group_namespace(list),
            Namespace::Value(outer) => Namespace::Value(match group_namespace(list) {
                Namespace::Value(own) => format!("{outer}\\{own}"),
                _ => format!("{outer}\\"),
            }),
        };
    }
    namespace
}

/// `getGroupNamespace()`: the first `'namespace'` key of the first argument's array.
fn group_namespace(list: &ArgumentList<'_>) -> Namespace {
    let Some(first) = list.arguments.iter().next() else {
        return Namespace::None;
    };
    let elements = match first.value() {
        Expression::Array(a) => a.elements.as_slice(),
        Expression::LegacyArray(a) => a.elements.as_slice(),
        _ => return Namespace::None,
    };
    for element in elements {
        let ArrayElement::KeyValue(kv) = element else {
            continue;
        };
        if literal_string(kv.key).as_deref() == Some("namespace") {
            return literal_string(kv.value).map_or(Namespace::Unreadable, Namespace::Value);
        }
    }
    Namespace::None
}

fn literal_string(expr: &Expression<'_>) -> Option<String> {
    match expr {
        Expression::Literal(Literal::String(s)) => {
            std::str::from_utf8(s.value?).ok().map(str::to_string)
        }
        _ => None,
    }
}

enum Key {
    Str(String),
    Int,
}

enum Value {
    Str(String),
    Array(Vec<(Key, Value)>),
    Other,
}

/// `ValueResolver::getValue()` on the values Pun reads: literals, string concatenations and arrays of them.
fn value(expr: &Expression<'_>) -> Option<Value> {
    match unparenthesize(expr) {
        Expression::Literal(Literal::String(s)) => {
            Some(Value::Str(std::str::from_utf8(s.value?).ok()?.to_string()))
        }
        Expression::Literal(_) => Some(Value::Other),
        Expression::Binary(b) if b.operator.is_concatenation() => {
            match (value(b.lhs)?, value(b.rhs)?) {
                (Value::Str(l), Value::Str(r)) => Some(Value::Str(l + &r)),
                _ => None,
            }
        }
        e @ (Expression::Array(_) | Expression::LegacyArray(_)) => {
            let mut items = Vec::new();
            for element in e.get_array_like_elements()? {
                match element {
                    ArrayElement::KeyValue(kv) => {
                        let key = match unparenthesize(kv.key) {
                            Expression::Literal(Literal::Integer(_)) => Key::Int,
                            k => Key::Str(literal_string(k)?),
                        };
                        items.push((key, value(kv.value)?));
                    }
                    ArrayElement::Value(v) => items.push((Key::Int, value(v.value)?)),
                    _ => return None,
                }
            }
            Some(Value::Array(items))
        }
        _ => None,
    }
}

fn get<'v>(items: &'v [(Key, Value)], key: &str) -> Option<&'v Value> {
    items
        .iter()
        .rev()
        .find(|(k, _)| matches!(k, Key::Str(s) if s == key))
        .map(|(_, v)| v)
}

/// `isActionString()` and the action `resolveControllerFromAction()` splits: a string with an `@`, or an array whose
/// keys are `uses` and maybe `as` and `middleware`, with a string `uses`.
fn action_string(action: &Value) -> Option<&str> {
    match action {
        Value::Str(s) => s.contains('@').then_some(s.as_str()),
        Value::Array(items) => {
            let known = items
                .iter()
                .all(|(k, _)| matches!(k, Key::Str(s) if ["as", "middleware", "uses"].contains(&s.as_str())));
            match get(items, "uses") {
                Some(Value::Str(uses)) if known => Some(uses),
                _ => None,
            }
        }
        Value::Other => None,
    }
}

/// `ReflectionResolver::resolveMethodReflection()`: the class has the method, its own, inherited or an `@method`.
fn has_method(ctx: &Context<'_>, class: &str, method: &str) -> bool {
    let facts = facts(ctx);
    let codebase = facts.documented();
    let Some(own) = codebase.get_class_like(class.as_bytes()) else {
        return false;
    };
    let same = |m: &[u8]| m.eq_ignore_ascii_case(method.as_bytes());
    codebase.method_exists(class.as_bytes(), method.as_bytes())
        || std::iter::once(&own.name)
            .chain(own.all_parent_classes.iter())
            .chain(own.all_parent_interfaces.iter())
            .filter_map(|name| codebase.get_class_like(name.as_bytes()))
            .any(|c| {
                c.pseudo_methods.iter().any(|m| same(m.as_bytes()))
                    || c.static_pseudo_methods.iter().any(|m| same(m.as_bytes()))
            })
}
