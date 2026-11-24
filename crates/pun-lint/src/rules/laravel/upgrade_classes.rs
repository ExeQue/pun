//! rector-laravel 2.7.3's upgrade rules below Laravel 12 that rewrite a class, a method body or a statement:
//! `AddParentBootToModelClassMethodRector`, `AddMockConsoleOutputFalseToConsoleTestsRector`,
//! `AddGuardToLoginEventRector`, `ChangeQueryWhereDateValueWithCarbonRector` (laravel-5.7),
//! `PropertyDeferToDeferrableProviderToRector` (laravel-5.8), `AddParentRegisterToEventServiceProviderRector`
//! (laravel-8), `UnifyModelDatesWithCastsRector` (laravel-10), `ModelCastsPropertyToCastsMethodRector` and
//! `ReplaceQueueTraitsWithQueueableRector` (laravel-11).

use mago_syntax::cst::{
    Argument, ArrayElement, Assignment, AssignmentOperator, Block, Call, ClassLikeMember,
    Expression, Identifier, Literal, Method, MethodBody, Node, NodeKind, Property, PropertyItem,
    Statement, Variable,
};
use std::fmt::Write as _;

use super::object_type::is_object_type;
use super::property_attribute::{ClassView, Kind, MODEL, array_elements, report};
use super::{is_a, member_name};
use crate::problem::{Edit, Safety, Span};
use crate::rules::refactor::configured_codemods::single_quoted;
use crate::rules::support::class_members::remove_members;
use crate::rules::support::expressions::text;
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::statement_edits::{
    HOLDERS, block_list, first_comment, line_indent, previous_token_end, reindent, remove,
};
use crate::rules::support::values::{Value, value};
use crate::rules::types::class_scope;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

fn end(s: mago_span::Span) -> usize {
    s.end.offset as usize
}

fn is_parent_call(statement: &Statement<'_>, method: &str) -> bool {
    let Statement::Expression(e) = statement else {
        return false;
    };
    matches!(e.expression, Expression::Call(Call::StaticMethod(s))
        if matches!(s.class, Expression::Parent(_))
            && member_name(&s.method).is_some_and(|n| n.eq_ignore_ascii_case(method.as_bytes())))
}

/// The class the method belongs to is a strict subclass of `parent`.
fn in_subclass_of(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], parent: &str) -> bool {
    facts(ctx)
        .codebase
        .get_class_like(parent.as_bytes())
        .is_some()
        && class_scope(ancestors)
            .and_then(|s| s.name)
            .is_some_and(|c| !c.eq_ignore_ascii_case(parent) && is_a(ctx, &c, &[parent]))
}

/// `parent::<method>();` first in `<method>()` of a subclass of `parent`, moved there when it is elsewhere.
fn parent_call_first(
    node: Node<'_, '_>,
    ancestors: &[Node<'_, '_>],
    ctx: &Context<'_>,
    parent: &str,
    name: &str,
    out: &mut Vec<Report>,
) {
    let Node::Method(method) = node else { return };
    if !method.name.value.eq_ignore_ascii_case(name.as_bytes())
        || !in_subclass_of(ctx, ancestors, parent)
    {
        return;
    }
    let MethodBody::Concrete(block) = &method.body else {
        return;
    };
    let stmts: Vec<&Statement<'_>> = block.statements.iter().collect();
    let found: Vec<usize> = (0..stmts.len())
        .filter(|&i| is_parent_call(stmts[i], name))
        .collect();
    if found.first() == Some(&0) {
        return;
    }
    let call = format!("parent::{name}();");
    let mut edits = if found.is_empty() {
        Vec::new()
    } else {
        remove(ctx, &block_list(ctx.source, block), &found)
    };
    edits.push(prepend(ctx.source, method, block, &call));
    out.push(Report::fixed(
        node_span(method),
        format!("Call `parent::{name}()` first in `{name}()`."),
        Safety::Unsafe,
        edits,
    ));
}

/// The edit that puts `code` before the first statement of the method's body.
fn prepend(source: &str, method: &Method<'_>, block: &Block<'_>, code: &str) -> Edit {
    let open = end(block.left_brace);
    match block.statements.first() {
        Some(first) => {
            let span = node_span(first);
            let at = first_comment(source, open, span.start).unwrap_or(span.start);
            let inner = line_indent(source, span.start);
            Edit::new(Span::new(at, at), format!("{code}\n{inner}"))
        }
        None => {
            let inner = format!("{}    ", line_indent(source, node_span(method).start));
            Edit::new(Span::new(open, open), format!("\n{inner}{code}"))
        }
    }
}

/// The edit that puts `code` after the last statement of the method's body.
fn append(source: &str, method: &Method<'_>, block: &Block<'_>, code: &str) -> Edit {
    match block.statements.last() {
        Some(last) => {
            let span = node_span(last);
            let inner = line_indent(source, span.start);
            Edit::new(Span::new(span.end, span.end), format!("\n{inner}{code}"))
        }
        None => {
            let open = end(block.left_brace);
            let inner = format!("{}    ", line_indent(source, node_span(method).start));
            Edit::new(Span::new(open, open), format!("\n{inner}{code}"))
        }
    }
}

/// `AddParentBootToModelClassMethodRector`.
pub struct AddParentBootCall;

impl Rule for AddParentBootCall {
    fn id(&self) -> &'static str {
        "lint/laravel-5.7/add-parent-boot-call"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        parent_call_first(node, ancestors, ctx, MODEL, "boot", out);
    }
}

/// `AddParentRegisterToEventServiceProviderRector`.
pub struct AddParentRegisterCall;

impl Rule for AddParentRegisterCall {
    fn id(&self) -> &'static str {
        "lint/laravel-8/add-parent-register-call"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        parent_call_first(
            node,
            ancestors,
            ctx,
            "Illuminate\\Foundation\\Support\\Providers\\EventServiceProvider",
            "register",
            out,
        );
    }
}

/// The first property of the class whose first item is named `name`, with its member index.
fn first_property<'ast, 'arena>(
    class: &ClassView<'ast, 'arena>,
    name: &str,
) -> Option<(usize, &'ast PropertyItem<'arena>, &'ast Property<'arena>)> {
    class.members.iter().enumerate().find_map(|(i, m)| {
        let ClassLikeMember::Property(p) = m else {
            return None;
        };
        let item = match p {
            Property::Plain(p) => p.items.first()?,
            Property::Hooked(p) => &p.item,
        };
        (&item.variable().name[1..] == name.as_bytes()).then_some((i, item, p))
    })
}

fn removed(class: &ClassView<'_, '_>, index: usize) -> Vec<bool> {
    (0..class.members.len()).map(|i| i == index).collect()
}

/// `PropertyDeferToDeferrableProviderToRector`.
pub struct PreferDeferrableProvider;

const DEFERRABLE: &str = "\\Illuminate\\Contracts\\Support\\DeferrableProvider";

impl Rule for PreferDeferrableProvider {
    fn id(&self) -> &'static str {
        "lint/laravel-5.8/prefer-deferrable-provider"
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
        if !class.is_object_type(ctx, ancestors, "Illuminate\\Support\\ServiceProvider") {
            return;
        }
        let mut found = None;
        for (i, m) in class.members.iter().enumerate() {
            let ClassLikeMember::Property(p) = m else {
                continue;
            };
            let item = match p {
                Property::Plain(p) => p.items.first(),
                Property::Hooked(p) => Some(&p.item),
            };
            let Some(item) = item.filter(|item| &item.variable().name[1..] == b"defer") else {
                continue;
            };
            let PropertyItem::Concrete(c) = item else {
                return;
            };
            if matches!(value(c.value), Some(Value::Bool(true))) {
                found = Some(i);
                break;
            }
        }
        let Some(index) = found else { return };
        let mut edits = remove_members(
            ctx.source,
            class.members,
            class.left_brace,
            class.right_brace,
            &removed(&class, index),
        );
        let insert = match (class.implements, class.extends, class.kind) {
            (Some(implements), _, _) => {
                let at = node_span(implements.types.last().unwrap()).end;
                Edit::new(Span::new(at, at), format!(", {DEFERRABLE}"))
            }
            (None, extends, kind) => {
                let at = match (extends, kind) {
                    (Some(extends), _) => node_span(extends).end,
                    (None, Kind::Named(c)) => end(c.name.span),
                    (None, Kind::Anonymous(c)) => c
                        .argument_list
                        .as_ref()
                        .map_or(end(c.class.span), |a| node_span(a).end),
                };
                Edit::new(Span::new(at, at), format!(" implements {DEFERRABLE}"))
            }
        };
        edits.push(insert);
        out.push(report(
            &class,
            "Implement `DeferrableProvider` instead of setting `$defer = true`.",
            edits,
        ));
    }
}

/// `ReplaceQueueTraitsWithQueueableRector`.
pub struct PreferQueueableTrait;

const QUEUE_TRAITS: [&str; 4] = [
    "Illuminate\\Foundation\\Bus\\Dispatchable",
    "Illuminate\\Queue\\InteractsWithQueue",
    "Illuminate\\Bus\\Queueable",
    "Illuminate\\Queue\\SerializesModels",
];

impl Rule for PreferQueueableTrait {
    fn id(&self) -> &'static str {
        "lint/laravel-11/prefer-queueable-trait"
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
        let mut scope = ancestors.to_vec();
        scope.push(node);
        let queue = |id: &Identifier<'_>| {
            let name = resolve_class_name(&scope, id);
            QUEUE_TRAITS.iter().any(|t| t.eq_ignore_ascii_case(&name))
        };
        let mut seen = [false; 4];
        for m in class.members {
            if let ClassLikeMember::TraitUse(t) = m {
                for id in &t.trait_names {
                    let name = resolve_class_name(&scope, id);
                    if let Some(i) = QUEUE_TRAITS
                        .iter()
                        .position(|q| q.eq_ignore_ascii_case(&name))
                    {
                        seen[i] = true;
                    }
                }
            }
        }
        if seen.contains(&false) {
            return;
        }
        let mut flags = vec![false; class.members.len()];
        let mut edits = Vec::new();
        let mut replaced = false;
        for (i, m) in class.members.iter().enumerate() {
            let ClassLikeMember::TraitUse(t) = m else {
                continue;
            };
            let names: Vec<&Identifier<'_>> = t.trait_names.iter().collect();
            if !names.iter().any(|id| queue(id)) {
                continue;
            }
            let mut kept = Vec::new();
            for id in &names {
                if !queue(id) {
                    kept.push(text(ctx, *id).to_string());
                } else if !replaced {
                    kept.push("\\Illuminate\\Foundation\\Queue\\Queueable".to_string());
                    replaced = true;
                }
            }
            if kept.is_empty() {
                flags[i] = true;
            } else {
                let span = Span::new(
                    node_span(names[0]).start,
                    node_span(*names.last().unwrap()).end,
                );
                edits.push(Edit::new(span, kept.join(", ")));
            }
        }
        if flags.contains(&true) {
            edits.extend(remove_members(
                ctx.source,
                class.members,
                class.left_brace,
                class.right_brace,
                &flags,
            ));
        }
        out.push(report(
            &class,
            "Use the `Queueable` trait instead of the four queue traits.",
            edits,
        ));
    }
}

/// The string value of a literal string, or `None`.
fn literal_string(expr: &Expression<'_>) -> Option<String> {
    match unparenthesize(expr) {
        Expression::Literal(Literal::String(s)) => {
            Some(String::from_utf8_lossy(s.value?).into_owned())
        }
        _ => None,
    }
}

/// `UnifyModelDatesWithCastsRector`.
pub struct PreferDateCasts;

impl Rule for PreferDateCasts {
    fn id(&self) -> &'static str {
        "lint/laravel-10/prefer-date-casts"
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
        if !class.is_object_type(ctx, ancestors, MODEL) {
            return;
        }
        let Some((dates_index, PropertyItem::Concrete(dates), _)) = first_property(&class, "dates")
        else {
            return;
        };
        let Some(elements) = array_elements(unparenthesize(dates.value)) else {
            return;
        };
        let mut names = Vec::new();
        for e in elements {
            let v = match e {
                ArrayElement::KeyValue(kv) => kv.value,
                ArrayElement::Value(v) => v.value,
                _ => return,
            };
            let Some(s) = literal_string(v) else { return };
            names.push(s);
        }
        if names.is_empty() {
            return;
        }
        let source = ctx.source;
        let mut edits = Vec::new();
        match first_property(&class, "casts") {
            Some((_, PropertyItem::Concrete(casts), _)) => {
                let Some(elements) = array_elements(unparenthesize(casts.value)) else {
                    return;
                };
                let mut keys = Vec::new();
                for e in elements {
                    match e {
                        ArrayElement::KeyValue(kv) => match value(kv.key) {
                            Some(Value::Str(s)) => keys.push(s),
                            Some(Value::Int(i)) => keys.push(i.to_string()),
                            _ => return,
                        },
                        ArrayElement::Value(_) => keys.push(String::new()),
                        _ => return,
                    }
                }
                let missing: Vec<String> = (names.iter())
                    .filter(|n| !keys.contains(n))
                    .map(|n| format!("{} => 'datetime'", single_quoted(n)))
                    .collect();
                if !missing.is_empty() {
                    let added = missing.join(", ");
                    edits.push(match elements.last() {
                        Some(last) => {
                            let at = node_span(last).end;
                            Edit::new(Span::new(at, at), format!(", {added}"))
                        }
                        None => {
                            let array = node_span(unparenthesize(casts.value));
                            let at = array.end - 1;
                            Edit::new(Span::new(at, at), added)
                        }
                    });
                }
                edits.extend(remove_members(
                    source,
                    class.members,
                    class.left_brace,
                    class.right_brace,
                    &removed(&class, dates_index),
                ));
            }
            Some(_) => return,
            None => {
                let items: Vec<String> = (names.iter())
                    .map(|n| format!("{} => 'datetime'", single_quoted(n)))
                    .collect();
                let casts = format!("protected $casts = [{}];", items.join(", "));
                let members: Vec<&ClassLikeMember<'_>> = class.members.iter().collect();
                // ClassInsertManipulator::addAsFirstMethod(): before the first method or after the first run of
                // properties.
                let at = members.iter().enumerate().find_map(|(i, m)| match m {
                    ClassLikeMember::Method(_) => Some((i, true)),
                    ClassLikeMember::Property(_)
                        if !matches!(members.get(i + 1), Some(ClassLikeMember::Property(_))) =>
                    {
                        Some((i, false))
                    }
                    _ => None,
                });
                let span = node_span(members[dates_index]);
                let indent = line_indent(source, span.start);
                match at {
                    Some((i, false)) if i == dates_index => {
                        edits.push(Edit::new(span, casts));
                    }
                    _ => return,
                }
                let _ = indent;
            }
        }
        out.push(report(
            &class,
            "Cast the `$dates` attributes in `$casts` instead.",
            edits,
        ));
    }
}

/// `ModelCastsPropertyToCastsMethodRector`.
pub struct PreferCastsMethodFix;

impl Rule for PreferCastsMethodFix {
    fn id(&self) -> &'static str {
        "lint/laravel-11/prefer-casts-method"
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
        if !class.is_object_type(ctx, ancestors, MODEL) || class.method("casts").is_some() {
            return;
        }
        let found = class.members.iter().enumerate().find_map(|(i, m)| {
            let ClassLikeMember::Property(Property::Plain(p)) = m else {
                return None;
            };
            let item = p.items.first()?;
            let protected =
                (p.modifiers.iter()).any(|m| matches!(m, mago_syntax::cst::Modifier::Protected(_)));
            (&item.variable().name[1..] == b"casts" && protected).then_some((i, item))
        });
        let Some((index, PropertyItem::Concrete(casts))) = found else {
            return;
        };
        let source = ctx.source;
        let member = class.members.get(index).unwrap();
        let span = node_span(member);
        let indent = line_indent(source, span.start);
        let from = crate::rules::support::class_members::member_from(
            class.members,
            class.left_brace,
            index,
        );
        let doc = first_comment(source, from, span.start)
            .map(|s| &source[s..span.start])
            .filter(|d| d.trim_start().starts_with("/**"));
        let value = unparenthesize(casts.value);
        let body = reindent(source, node_span(value), indent, &format!("{indent}    "));
        let mut method = String::new();
        if let Some(doc) = doc {
            method.push_str(&return_doc(doc.trim_end()));
            method.push('\n');
            method.push_str(indent);
        }
        let _ = write!(
            method,
            "protected function casts(): array\n{indent}{{\n{indent}    return {body};\n{indent}}}"
        );
        let last = class.members.len() - 1;
        let edits = if index == last {
            let at = doc.map_or(span.start, |_| {
                first_comment(source, from, span.start).unwrap()
            });
            vec![Edit::new(Span::new(at, span.end), method)]
        } else {
            let mut edits = remove_members(
                source,
                class.members,
                class.left_brace,
                class.right_brace,
                &removed(&class, index),
            );
            let at = node_span(class.members.get(last).unwrap()).end;
            edits.push(Edit::new(
                Span::new(at, at),
                format!("\n\n{indent}{method}"),
            ));
            edits
        };
        out.push(report(
            &class,
            "Define the casts in a `casts()` method instead of the `$casts` property.",
            edits,
        ));
    }
}

/// The doc comment with its `@var` tag turned into `@return` (without the variable name).
fn return_doc(doc: &str) -> String {
    let mut out = Vec::new();
    for line in doc.split('\n') {
        match line.find("@var") {
            Some(at) => {
                let rest = line[at + 4..].replace(" $casts", "");
                out.push(format!("{}@return{rest}", &line[..at]));
            }
            None => out.push(line.to_string()),
        }
    }
    out.join("\n")
}

/// `AddMockConsoleOutputFalseToConsoleTestsRector`.
pub struct AddMockConsoleOutputFalse;

const ARTISAN: &str = "Illuminate\\Support\\Facades\\Artisan";

impl Rule for AddMockConsoleOutputFalse {
    fn id(&self) -> &'static str {
        "lint/laravel-5.7/add-mock-console-output-false"
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
        if !class.is_object_type(ctx, ancestors, "Illuminate\\Foundation\\Testing\\TestCase") {
            return;
        }
        let mut outputs = false;
        let mut already = false;
        let mut stack: Vec<(Node<'_, '_>, Vec<Node<'_, '_>>)> = Vec::new();
        let mut scope = ancestors.to_vec();
        scope.push(node);
        node.visit_children(|c| stack.push((c, scope.clone())));
        while let Some((n, path)) = stack.pop() {
            match n {
                Node::StaticMethodCall(s)
                    if member_name(&s.method)
                        .is_some_and(|m| m.eq_ignore_ascii_case(b"output"))
                        && is_artisan(ctx, &path, s.class) =>
                {
                    outputs = true;
                }
                Node::Assignment(Assignment {
                    lhs,
                    operator: AssignmentOperator::Assign(_),
                    rhs,
                    ..
                }) if is_this_property(lhs, "mockConsoleOutput")
                    && matches!(value(rhs), Some(Value::Bool(false))) =>
                {
                    already = true;
                }
                _ => {}
            }
            let mut inner = path.clone();
            inner.push(n);
            n.visit_children(|c| stack.push((c, inner.clone())));
        }
        if !outputs || already {
            return;
        }
        let source = ctx.source;
        let assign = "$this->mockConsoleOutput = false;";
        let edit = match class.method("setUp") {
            Some((_, method)) => {
                let MethodBody::Concrete(block) = &method.body else {
                    return;
                };
                append(source, method, block, assign)
            }
            None => {
                let Some(first) = class.members.first() else {
                    return;
                };
                let span = node_span(first);
                let at =
                    first_comment(source, end(class.left_brace), span.start).unwrap_or(span.start);
                let indent = line_indent(source, span.start);
                let returns = setup_return_type(ctx);
                Edit::new(
                    Span::new(at, at),
                    format!(
                        "protected function setUp(){returns}\n{indent}{{\n{indent}    parent::setUp();\n\
                         {indent}    {assign}\n{indent}}}\n{indent}"
                    ),
                )
            }
        };
        out.push(report(
            &class,
            "Set `$this->mockConsoleOutput = false` in `setUp()` of a test that reads the console output.",
            vec![edit],
        ));
    }
}

/// `SetUpMethodDecorator`: the return type of PHPUnit's own `TestCase::setUp()`, when PHPUnit is known.
fn setup_return_type(ctx: &Context<'_>) -> String {
    let facts = facts(ctx);
    let method = (facts.codebase).get_declaring_method(b"PHPUnit\\Framework\\TestCase", b"setUp");
    match method.and_then(|m| m.return_type_declaration_metadata.as_ref()) {
        Some(t) if t.type_union.is_void() => ": void".to_string(),
        _ => String::new(),
    }
}

fn is_artisan(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], class: &Expression<'_>) -> bool {
    let caller = match unparenthesize(class) {
        Expression::Identifier(id) => Some(resolve_class_name(ancestors, id)),
        e => facts(ctx)
            .full_type(node_span(e))
            .and_then(|t| match &t.types[..] {
                [a] => super::object_type::object_class(a),
                _ => None,
            }),
    };
    caller.is_some_and(|c| c.eq_ignore_ascii_case(ARTISAN) || is_a(ctx, ARTISAN, &[&c]))
}

fn is_this_property(expr: &Expression<'_>, name: &str) -> bool {
    let Expression::Access(mago_syntax::cst::Access::Property(p)) = unparenthesize(expr) else {
        return false;
    };
    matches!(p.object, Expression::Variable(Variable::Direct(v)) if v.name == b"$this")
        && member_name(&p.property).is_some_and(|n| n == name.as_bytes())
}

/// The start of the statement with the comments before it, when it sits in a statement list.
fn statement_start(
    ctx: &Context<'_>,
    node: Node<'_, '_>,
    ancestors: &[Node<'_, '_>],
) -> Option<usize> {
    let span = node_span(&node);
    let holder = ancestors
        .iter()
        .rev()
        .find(|a| node_span(*a) != span && !matches!(a, Node::Statement(_)))?;
    if !HOLDERS.contains(&holder.kind()) {
        return None;
    }
    let from = node_span(holder).start;
    let before = previous_token_end(ctx, from, span.start);
    Some(first_comment(ctx.source, before, span.start).unwrap_or(span.start))
}

/// `AddGuardToLoginEventRector`.
pub struct AddLoginEventGuard;

impl Rule for AddLoginEventGuard {
    fn id(&self) -> &'static str {
        "lint/laravel-5.7/add-login-event-guard"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ExpressionStatement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::ExpressionStatement(statement) = node else {
            return;
        };
        let (Expression::Assignment(Assignment {
            operator: AssignmentOperator::Assign(_),
            rhs: Expression::Instantiation(new),
            ..
        })
        | Expression::Instantiation(new)) = statement.expression
        else {
            return;
        };
        let Expression::Identifier(id) = new.class else {
            return;
        };
        if !resolve_class_name(ancestors, id)
            .eq_ignore_ascii_case("Illuminate\\Auth\\Events\\Login")
            || new.argument_list.as_ref().map_or(0, |l| l.arguments.len()) == 3
        {
            return;
        }
        let Some(at) = statement_start(ctx, node, ancestors) else {
            return;
        };
        let indent = line_indent(ctx.source, node_span(statement).start);
        let argument = match &new.argument_list {
            Some(list) => match list.arguments.first() {
                Some(first) => {
                    let s = node_span(first).start;
                    Edit::new(Span::new(s, s), "$guard, ")
                }
                None => {
                    let s = end(list.left_parenthesis);
                    Edit::new(Span::new(s, s), "$guard")
                }
            },
            None => {
                let s = node_span(new.class).end;
                Edit::new(Span::new(s, s), "($guard)")
            }
        };
        out.push(Report::fixed(
            node_span(new),
            "Pass the `$guard` to the `Login` event.",
            Safety::Unsafe,
            vec![
                Edit::new(
                    Span::new(at, at),
                    format!("$guard = config('auth.defaults.guard');\n{indent}"),
                ),
                argument,
            ],
        ));
    }
}

/// `ChangeQueryWhereDateValueWithCarbonRector`.
pub struct AddWhereTime;

impl Rule for AddWhereTime {
    fn id(&self) -> &'static str {
        "lint/laravel-5.7/add-where-time"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ExpressionStatement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::ExpressionStatement(statement) = node else {
            return;
        };
        let Expression::Call(Call::Method(call)) = statement.expression else {
            return;
        };
        if !member_name(&call.method).is_some_and(|n| n.eq_ignore_ascii_case(b"whereDate"))
            || !is_object_type(
                ctx,
                ancestors,
                call.object,
                "Illuminate\\Database\\Query\\Builder",
            )
        {
            return;
        }
        let args: Vec<&Argument<'_>> = call.argument_list.arguments.iter().collect();
        if args.len() < 3
            || args
                .iter()
                .any(|a| matches!(a, Argument::Positional(p) if p.ellipsis.is_some()))
        {
            return;
        }
        let date = unparenthesize(args[2].value());
        if !is_object_type(ctx, ancestors, date, "DateTimeInterface")
            || is_carbon_today(ctx, ancestors, date)
        {
            return;
        }
        let sign = value(args[1].value());
        if matches!(&sign, Some(Value::Str(s)) if s == ">=" || s == "<=") {
            return;
        }
        let call_like = matches!(
            date,
            Expression::Call(Call::StaticMethod(_) | Call::Method(_))
        );
        if !call_like && !matches!(date, Expression::Variable(Variable::Direct(_))) {
            return;
        }
        let source = ctx.source;
        let mut edits = Vec::new();
        let new_sign = match args[1].value() {
            Expression::Literal(Literal::String(s)) => match s.value {
                Some(b"<") => Some("'<='"),
                Some(b">") => Some("'>='"),
                _ => None,
            },
            _ => None,
        };
        let sign_text =
            new_sign.map_or_else(|| text(ctx, args[1].value()).to_string(), str::to_string);
        if let Some(new_sign) = new_sign {
            edits.push(Edit::new(node_span(args[1].value()), new_sign));
        }
        let variable = if call_like {
            let Some(at) = statement_start(ctx, node, ancestors) else {
                return;
            };
            let indent = line_indent(source, node_span(statement).start);
            edits.push(Edit::new(
                Span::new(at, at),
                format!("$dateTime = {};\n{indent}", text(ctx, date)),
            ));
            edits.push(Edit::new(node_span(date), "$dateTime"));
            "$dateTime".to_string()
        } else {
            if statement_start(ctx, node, ancestors).is_none() {
                return;
            }
            text(ctx, date).to_string()
        };
        let indent = line_indent(source, node_span(statement).start);
        let at = node_span(statement).end;
        edits.push(Edit::new(
            Span::new(at, at),
            format!(
                "\n{indent}{}->whereTime({}, {sign_text}, {variable});",
                text(ctx, call.object),
                text(ctx, args[0]),
            ),
        ));
        out.push(Report::fixed(
            node_span(call),
            "Compare the time with `whereTime()` too: `whereDate()` compares only the date.",
            Safety::Unsafe,
            edits,
        ));
    }
}

fn is_carbon_today(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    let Expression::Call(Call::StaticMethod(s)) = expr else {
        return false;
    };
    member_name(&s.method).is_some_and(|n| n.eq_ignore_ascii_case(b"today"))
        && is_object_type(ctx, ancestors, s.class, "Carbon\\Carbon")
}
