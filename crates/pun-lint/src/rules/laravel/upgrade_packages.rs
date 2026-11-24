//! rector-laravel 2.7.3's `ReplaceExpectsMethodsInTestsRector` (laravel-10), its package upgrade rules below the
//! current majors (`CashierStripeOptionsToStripeRector`, `LivewireComponentComputedMethodToComputedAttributeRector`,
//! `LivewireComponentQueryStringToUrlAttributeRector`, `ReplaceFakerPropertyFetchWithMethodCallRector`) and its Lumen
//! rules (`LumenRoutesStringActionToUsesArrayRector`, `LumenRoutesStringMiddlewareToArrayRector`).

use mago_syntax::cst::{
    Access, Argument, ArrayElement, ClassLikeMember, ClassLikeMemberSelector, Expression, Literal,
    Method, MethodBody, Modifier, Node, NodeKind, Property, Statement,
};

use super::is_written;
use super::object_type::is_object_type;
use super::property_attribute::{ClassView, MODEL, array_elements, report};
use super::{is_a, member_name};
use crate::problem::{Edit, Safety, Span};
use crate::rules::support::class_members::remove_members;
use crate::rules::support::expressions::text;
use crate::rules::support::names::has_attribute;
use crate::rules::support::rewrites::list_removal;
use crate::rules::support::statement_edits::{first_comment, line_indent};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

fn string_value(expr: &Expression<'_>) -> Option<String> {
    match expr {
        Expression::Literal(Literal::String(s)) => {
            Some(String::from_utf8_lossy(s.value?).into_owned())
        }
        _ => None,
    }
}

/// `CashierStripeOptionsToStripeRector`.
pub struct PreferStripeMethod;

impl Rule for PreferStripeMethod {
    fn id(&self) -> &'static str {
        "lint/cashier-13/prefer-stripe-method"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(class) = ClassView::of(node) else {
            return;
        };
        // Upstream's `usesBillableTrait()` returns true whether or not the class uses `Billable`.
        if !class.is_object_type(ctx, ancestors, MODEL) {
            return;
        }
        let Some((_, method)) = class.method("stripeOptions") else {
            return;
        };
        out.push(report(
            &class,
            "Rename `stripeOptions()` to `stripe()`: Cashier 13 calls `stripe()`.",
            vec![Edit::new(node_span(&method.name), "stripe")],
        ));
    }
}

/// `ReplaceFakerPropertyFetchWithMethodCallRector`.
pub struct NoFakerPropertyAccess;

impl Rule for NoFakerPropertyAccess {
    fn id(&self) -> &'static str {
        "lint/faker-1/no-faker-property-access"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::PropertyAccess])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::PropertyAccess(access) = node else {
            return;
        };
        if is_written(node_span(access), ancestors)
            || !is_object_type(ctx, ancestors, access.object, "Faker\\Generator")
        {
            return;
        }
        let name = match &access.property {
            ClassLikeMemberSelector::Identifier(_) | ClassLikeMemberSelector::Expression(_) => {
                text(ctx, &access.property).to_string()
            }
            ClassLikeMemberSelector::Variable(_) => {
                format!("{{{}}}", text(ctx, &access.property))
            }
            ClassLikeMemberSelector::Missing(_) => return,
        };
        let span = Span::new(node_span(&access.property).start, node_span(access).end);
        out.push(Report::fixed(
            node_span(access),
            "Call the Faker formatter as a method instead of reading it as a property.",
            Safety::Safe,
            vec![Edit::new(span, format!("{name}()"))],
        ));
    }
}

const COMPONENT: &str = "Livewire\\Component";

/// `LivewireComponentComputedMethodToComputedAttributeRector`.
pub struct PreferComputedAttribute;

const COMPUTED: &str = "Livewire\\Attributes\\Computed";

impl Rule for PreferComputedAttribute {
    fn id(&self) -> &'static str {
        "lint/livewire-3/prefer-computed-attribute"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(class) = ClassView::of(node) else {
            return;
        };
        if !class.is_object_type(ctx, ancestors, COMPONENT) {
            return;
        }
        let class_name = class.name(ctx, ancestors);
        let mut scope = ancestors.to_vec();
        scope.push(node);
        let mut edits = Vec::new();
        for member in class.members {
            let ClassLikeMember::Method(method) = member else {
                continue;
            };
            let public = !(method.modifiers.iter())
                .any(|m| matches!(m, Modifier::Private(_) | Modifier::Protected(_)));
            let name = method.name.value;
            let Some(middle) = name
                .strip_prefix(b"get")
                .and_then(|n| n.strip_suffix(b"Property"))
                .filter(|m| m.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_'))
            else {
                continue;
            };
            if !public {
                continue;
            }
            let mut new = String::from_utf8_lossy(middle).into_owned();
            if let Some(first) = new.get_mut(..1) {
                first.make_ascii_lowercase();
            }
            let codebase = crate::rules::types::facts::facts(ctx);
            if codebase
                .documented()
                .method_exists(class_name.as_bytes(), new.as_bytes())
                || has_attribute(&scope, &method.attribute_lists, COMPUTED)
            {
                continue;
            }
            edits.extend(method_attribute(
                ctx.source,
                method,
                &format!("#[\\{COMPUTED}]"),
            ));
            edits.push(Edit::new(node_span(&method.name), new));
        }
        if !edits.is_empty() {
            out.push(report(
                &class,
                "Use the `#[Computed]` attribute instead of a `get...Property()` method.",
                edits,
            ));
        }
    }
}

/// The edit that adds an attribute group to a method, after its other attributes.
fn method_attribute(source: &str, method: &Method<'_>, group: &str) -> Vec<Edit> {
    member_attribute(
        source,
        node_span(method).start,
        method.attribute_lists.last().map(|l| node_span(l).end),
        group,
    )
}

fn member_attribute(source: &str, start: usize, last: Option<usize>, group: &str) -> Vec<Edit> {
    let indent = line_indent(source, start);
    match last {
        Some(end) => vec![Edit::new(Span::new(end, end), format!("\n{indent}{group}"))],
        None => vec![Edit::new(
            Span::new(start, start),
            format!("{group}\n{indent}"),
        )],
    }
}

/// `LivewireComponentQueryStringToUrlAttributeRector`.
pub struct PreferUrlAttribute;

const URL: &str = "Livewire\\Attributes\\Url";

impl Rule for PreferUrlAttribute {
    fn id(&self) -> &'static str {
        "lint/livewire-3/prefer-url-attribute"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(class) = ClassView::of(node) else {
            return;
        };
        if !class.is_object_type(ctx, ancestors, COMPONENT) {
            return;
        }
        let name_of = |p: &Property<'_>| match p {
            Property::Plain(p) => p.items.first().map(|i| i.variable().name[1..].to_vec()),
            Property::Hooked(p) => Some(p.item.variable().name[1..].to_vec()),
        };
        let mut found = class
            .members
            .iter()
            .enumerate()
            .filter_map(|(i, m)| match m {
                ClassLikeMember::Property(p) if name_of(p).as_deref() == Some(b"queryString") => {
                    Some((i, p))
                }
                _ => None,
            });
        let Some((query_index, Property::Plain(query))) = found.next_back() else {
            return;
        };
        let Some(mago_syntax::cst::PropertyItem::Concrete(item)) = query.items.first() else {
            return;
        };
        let Some(elements) = array_elements(item.value) else {
            return;
        };
        let elements: Vec<&ArrayElement<'_>> = elements.iter().collect();
        if elements.is_empty() {
            return;
        }
        let mut properties: Vec<(String, Option<Vec<String>>)> = Vec::new();
        let mut filtered = vec![false; elements.len()];
        for (i, element) in elements.iter().enumerate() {
            let (key, value) = match element {
                ArrayElement::KeyValue(kv) => (Some(kv.key), kv.value),
                ArrayElement::Value(v) => (None, v.value),
                _ => continue,
            };
            if let (Some(key), Some(options)) = (key.and_then(string_value), array_elements(value))
            {
                let Some(args) = url_args(ctx, options.iter().collect()) else {
                    continue;
                };
                set(&mut properties, key, Some(args));
                filtered[i] = true;
                continue;
            }
            if let Some(name) = string_value(value) {
                set(&mut properties, name, None);
                filtered[i] = true;
            }
        }
        if properties.is_empty() {
            return;
        }
        let mut scope = ancestors.to_vec();
        scope.push(node);
        let source = ctx.source;
        let mut edits = Vec::new();
        for member in class.members {
            let ClassLikeMember::Property(p) = member else {
                continue;
            };
            let Some(name) = name_of(p) else { continue };
            let Some((_, args)) = properties.iter().find(|(n, _)| n.as_bytes() == name) else {
                continue;
            };
            let lists = match p {
                Property::Plain(p) => &p.attribute_lists,
                Property::Hooked(p) => &p.attribute_lists,
            };
            if has_attribute(&scope, lists, URL) {
                continue;
            }
            let args = args
                .as_ref()
                .map_or(String::new(), |a| format!("({})", a.join(", ")));
            edits.extend(member_attribute(
                source,
                node_span(p).start,
                lists.last().map(|l| node_span(l).end),
                &format!("#[\\{URL}{args}]"),
            ));
        }
        if filtered.iter().all(|f| *f) {
            let flags: Vec<bool> = (0..class.members.len()).map(|i| i == query_index).collect();
            edits.extend(remove_members(
                source,
                class.members,
                class.left_brace,
                class.right_brace,
                &flags,
            ));
        } else {
            let spans: Vec<Span> = elements.iter().map(|e| node_span(*e)).collect();
            edits.extend(list_removal(&spans, &filtered).unwrap_or_default());
        }
        out.push(report(
            &class,
            "Use the `#[Url]` attribute instead of the `$queryString` property.",
            edits,
        ));
    }
}

fn set(
    properties: &mut Vec<(String, Option<Vec<String>>)>,
    name: String,
    args: Option<Vec<String>>,
) {
    match properties.iter_mut().find(|(n, _)| *n == name) {
        Some(found) => found.1 = args,
        None => properties.push((name, args)),
    }
}

/// `processArrayOptionsIntoArgs()`: `except` and `as` with scalar values become named arguments.
fn url_args(ctx: &Context<'_>, options: Vec<&ArrayElement<'_>>) -> Option<Vec<String>> {
    let mut args = Vec::new();
    for option in &options {
        let ArrayElement::KeyValue(kv) = option else {
            continue;
        };
        let Some(key) = string_value(kv.key) else {
            continue;
        };
        let scalar = matches!(
            kv.value,
            Expression::Literal(_) | Expression::MagicConstant(_) | Expression::CompositeString(_)
        );
        if scalar && (key == "except" || key == "as") {
            args.push(format!("{key}: {}", text(ctx, kv.value)));
        }
    }
    (args.len() == options.len()).then_some(args)
}

/// `ReplaceExpectsMethodsInTestsRector`.
pub struct PreferFakeOverExpects;

const TEST_CASE: &str = "Illuminate\\Foundation\\Testing\\TestCase";

impl Rule for PreferFakeOverExpects {
    fn id(&self) -> &'static str {
        "lint/laravel-10/prefer-fake-over-expects"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(class) = ClassView::of(node) else {
            return;
        };
        let known = crate::rules::types::facts::facts(ctx)
            .codebase
            .get_class_like(TEST_CASE.as_bytes())
            .is_some();
        if !known || !is_a(ctx, &class.name(ctx, ancestors), &[TEST_CASE]) {
            return;
        }
        let mut scope = ancestors.to_vec();
        scope.push(node);
        let mut edits = Vec::new();
        for member in class.members {
            let ClassLikeMember::Method(method) = member else {
                continue;
            };
            let MethodBody::Concrete(block) = &method.body else {
                continue;
            };
            let mut method_scope = scope.clone();
            method_scope.push(Node::Method(method));
            let mut appended = Vec::new();
            for (facade, expects, doesnt) in [
                ("Bus", "expectsJobs", "doesntExpectJobs"),
                ("Event", "expectsEvents", "doesntExpectEvents"),
            ] {
                let Some(found) = expected_calls(ctx, &method_scope, method, expects, doesnt)
                else {
                    break;
                };
                if found.statements.is_empty() {
                    continue;
                }
                let facade = format!("\\Illuminate\\Support\\Facades\\{facade}");
                let mut fake = found.expected.clone();
                for item in &found.not_expected {
                    if !fake.contains(item) {
                        fake.push(item.clone());
                    }
                }
                let stmts: Vec<&Statement<'_>> = block.statements.iter().collect();
                for (n, &i) in found.statements.iter().enumerate() {
                    let span = node_span(stmts[i]);
                    if n == 0 {
                        edits.push(Edit::new(
                            span,
                            format!("{facade}::fake([{}]);", fake.join(", ")),
                        ));
                        continue;
                    }
                    let to = stmts.get(i + 1).map_or(span.end, |next| {
                        let next = node_span(*next);
                        first_comment(ctx.source, span.end, next.start).unwrap_or(next.start)
                    });
                    let from = if i + 1 < stmts.len() {
                        span.start
                    } else {
                        node_span(stmts[i - 1]).end
                    };
                    edits.push(Edit::new(Span::new(from, to), ""));
                }
                for item in unique(&found.expected) {
                    appended.push(format!("{facade}::assertDispatched({item});"));
                }
                for item in unique(&found.not_expected) {
                    appended.push(format!("{facade}::assertNotDispatched({item});"));
                }
            }
            if appended.is_empty() {
                continue;
            }
            let last = node_span(block.statements.last().unwrap());
            let indent = line_indent(ctx.source, last.start);
            let text: String = appended.iter().map(|a| format!("\n{indent}{a}")).collect();
            edits.push(Edit::new(Span::new(last.end, last.end), text));
        }
        if !edits.is_empty() {
            out.push(report(
                &class,
                "Fake the bus or the events and assert afterwards instead of `expectsJobs()` or `expectsEvents()`.",
                edits,
            ));
        }
    }
}

fn unique(items: &[String]) -> Vec<&String> {
    let mut out: Vec<&String> = Vec::new();
    for item in items {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

struct Expected {
    /// Indices of the top-level statements of the method that are one of the calls.
    statements: Vec<usize>,
    expected: Vec<String>,
    not_expected: Vec<String>,
}

/// `ExpectedClassMethodAnalyzer`: the `expects*()` and `doesntExpect*()` calls on the test case in the method, or
/// `None` when a first-class callable of one of them appears. Pun only rewrites calls that are statements of the
/// method's own body.
fn expected_calls(
    ctx: &Context<'_>,
    scope: &[Node<'_, '_>],
    method: &Method<'_>,
    expects: &str,
    doesnt: &str,
) -> Option<Expected> {
    let mut found = Expected {
        statements: Vec::new(),
        expected: Vec::new(),
        not_expected: Vec::new(),
    };
    let MethodBody::Concrete(block) = &method.body else {
        return Some(found);
    };
    let mut calls = Vec::new();
    let mut stack: Vec<(Node<'_, '_>, Vec<Node<'_, '_>>)> = Vec::new();
    Node::Block(block).visit_children(|c| stack.push((c, scope.to_vec())));
    while let Some((n, path)) = stack.pop() {
        if let Node::MethodCall(call) = n
            && is_object_type(ctx, &path, call.object, TEST_CASE)
        {
            let name = member_name(&call.method);
            let first_class = call.argument_list.arguments.len() == 1
                && matches!(call.argument_list.arguments.first(), Some(Argument::Positional(p)) if p.ellipsis.is_some());
            if name.is_some_and(|n| n.eq_ignore_ascii_case(expects.as_bytes())) {
                calls.push((call, true));
            } else {
                if name.is_some_and(|n| n.eq_ignore_ascii_case(doesnt.as_bytes())) {
                    calls.push((call, false));
                }
                if first_class {
                    return None;
                }
            }
        }
        let mut inner = path.clone();
        inner.push(n);
        n.visit_children(|c| stack.push((c, inner.clone())));
    }
    calls.sort_by_key(|(c, _)| node_span(*c).start);
    for (call, expected) in &calls {
        let items = items_to_fake(ctx, call.argument_list.arguments.first());
        if *expected {
            found.expected.extend(items);
        } else {
            found.not_expected.extend(items);
        }
    }
    for (i, statement) in block.statements.iter().enumerate() {
        if let Statement::Expression(e) = statement
            && calls
                .iter()
                .any(|(c, _)| node_span(*c) == node_span(e.expression))
        {
            found.statements.push(i);
        }
    }
    if found.statements.len() != calls.len() {
        found.statements.clear();
        return None;
    }
    Some(found)
}

fn items_to_fake(ctx: &Context<'_>, argument: Option<&Argument<'_>>) -> Vec<String> {
    let Some(argument) = argument else {
        return Vec::new();
    };
    let is_class = |e: &Expression<'_>| {
        matches!(e, Expression::Access(Access::ClassConstant(c))
            if matches!(&c.constant, mago_syntax::cst::ClassLikeConstantSelector::Identifier(id) if id.value.eq_ignore_ascii_case(b"class")))
    };
    let item = |e: &Expression<'_>| {
        (is_class(e) || matches!(e, Expression::Literal(Literal::String(_))))
            .then(|| text(ctx, e).to_string())
    };
    let value = argument.value();
    if let Some(elements) = array_elements(value) {
        return elements
            .iter()
            .filter_map(|e| match e {
                ArrayElement::KeyValue(kv) => item(kv.value),
                ArrayElement::Value(v) => item(v.value),
                _ => None,
            })
            .collect();
    }
    item(value).into_iter().collect()
}

const ROUTER: &str = "Laravel\\Lumen\\Routing\\Router";
const ROUTE_METHODS: [&str; 6] = ["delete", "get", "options", "patch", "post", "put"];

fn is_route(call: &mago_syntax::cst::MethodCall<'_>) -> bool {
    member_name(&call.method).is_some_and(|n| {
        ROUTE_METHODS
            .iter()
            .any(|m| n.eq_ignore_ascii_case(m.as_bytes()))
    })
}

/// `LumenRoutesStringActionToUsesArrayRector`.
pub struct PreferUsesArray;

impl Rule for PreferUsesArray {
    fn id(&self) -> &'static str {
        "lint/lumen/prefer-uses-array"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::MethodCall(call) = node else { return };
        if !is_object_type(ctx, ancestors, call.object, ROUTER) || !is_route(call) {
            return;
        }
        let Some(action) = call.argument_list.arguments.get(1) else {
            return;
        };
        let value = action.value();
        if !matches!(value, Expression::Literal(Literal::String(_))) {
            return;
        }
        out.push(Report::fixed(
            node_span(value),
            "Write the route action as `['uses' => ...]`.",
            Safety::Safe,
            vec![Edit::new(
                node_span(action),
                format!("['uses' => {}]", text(ctx, value)),
            )],
        ));
    }
}

/// `LumenRoutesStringMiddlewareToArrayRector`.
pub struct PreferMiddlewareArray;

impl Rule for PreferMiddlewareArray {
    fn id(&self) -> &'static str {
        "lint/lumen/prefer-middleware-array"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::MethodCall(call) = node else { return };
        if !is_object_type(ctx, ancestors, call.object, ROUTER) {
            return;
        }
        let group = member_name(&call.method).is_some_and(|n| n.eq_ignore_ascii_case(b"group"));
        let index = match (group, is_route(call)) {
            (true, _) => 0,
            (false, true) => 1,
            _ => return,
        };
        let Some(attributes) = call.argument_list.arguments.get(index) else {
            return;
        };
        let Expression::Array(array) = attributes.value() else {
            return;
        };
        let middleware = array.elements.iter().find_map(|e| match e {
            ArrayElement::KeyValue(kv) if string_value(kv.key).as_deref() == Some("middleware") => {
                Some(kv)
            }
            _ => None,
        });
        let Some(middleware) = middleware else { return };
        let Some(names) = string_value(middleware.value) else {
            return;
        };
        let items: Vec<String> = names
            .split('|')
            .map(crate::rules::refactor::configured_codemods::single_quoted)
            .collect();
        out.push(Report::fixed(
            node_span(middleware.value),
            "Write the route middleware as an array.",
            Safety::Safe,
            vec![Edit::new(
                node_span(middleware.value),
                format!("[{}]", items.join(", ")),
            )],
        ));
    }
}
