//! rector-phpunit's rules in no set (or only in rector-preset) that work on a test class or test method:
//! `PreferTestsWithCamelCaseRector`, `PreferTestsWithSnakeCaseRector`, `ExplicitMockExpectsCallRector`,
//! `AddKeysExistsAssertForKeyUseRector` and `AddSeeTestAnnotationRector`.

use crate::problem::{Edit, Safety, Span};
use mago_codex::ttype::atomic::TAtomic;
use mago_syntax::cst::{
    ClassLikeMember, Expression, Literal, MethodBody, Node, NodeKind, Statement, Variable,
};

use super::prefer_create_stub_expression::{is_mock_object, is_test_class_method};
use super::{
    ASSERT, AnyCall, TEST_CASE, TEST_CASE_LEGACY, class_is, is_in_test_class, own_class_reflection,
    receiver_is,
};
use crate::rules::laravel::is_a;
use crate::rules::support::class_members::qualified;
use crate::rules::support::conditions::indentation;
use crate::rules::support::docblocks::Doc;
use crate::rules::support::expressions::text;
use crate::rules::support::names::imported;
use crate::rules::support::statement_edits::line_indent;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

/// `PreferTestsWithCamelCaseRector::toCamelCase()`: the words between `_` (or `-`, ` `), each with an ASCII
/// upper-case first byte, joined, with an ASCII lower-case first byte.
fn to_camel_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for word in name.split(['-', '_', ' ']) {
        let mut chars = word.chars();
        out.extend(chars.next().map(|c| c.to_ascii_uppercase()));
        out.push_str(chars.as_str());
    }
    let mut chars = out.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_lowercase(), chars.as_str()),
        None => out,
    }
}

/// `PreferTestsWithSnakeCaseRector::toSnakeCase()`: `_` before every ASCII upper-case letter but the first byte,
/// then ASCII lower case (`ucwords()` and the whitespace removal change nothing in a method name).
fn to_snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.char_indices() {
        if i > 0 && c.is_ascii_uppercase() {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

/// Renames the test methods of a test class, in order, skipping a new name that another method already has.
fn rename_tests(
    node: Node<'_, '_>,
    ancestors: &[Node<'_, '_>],
    ctx: &Context<'_>,
    out: &mut Vec<Report>,
    rename: fn(&str) -> String,
    message: &str,
) {
    let members = match node {
        Node::Class(c) => &c.members,
        Node::AnonymousClass(c) => &c.members,
        _ => return,
    };
    let mut chain = ancestors.to_vec();
    chain.push(node);
    if !class_is(ctx, &chain, &[TEST_CASE, TEST_CASE_LEGACY]) {
        return;
    }
    let methods: Vec<_> = members
        .iter()
        .filter_map(|m| match m {
            ClassLikeMember::Method(m) => Some(m),
            _ => None,
        })
        .collect();
    let mut names: Vec<String> = methods
        .iter()
        .map(|m| String::from_utf8_lossy(m.name.value).into_owned())
        .collect();
    for (i, method) in methods.iter().enumerate() {
        if !is_test_class_method(ctx, &chain, method) {
            continue;
        }
        let new = rename(&names[i]);
        if new == names[i] || names.iter().any(|n| n.eq_ignore_ascii_case(&new)) {
            continue;
        }
        out.push(Report::fixed(
            node_span(&method.name),
            message,
            Safety::Unsafe,
            vec![Edit::new(node_span(&method.name), new.clone())],
        ));
        names[i] = new;
    }
}

/// `PreferTestsWithCamelCaseRector`.
pub struct PreferCamelCaseTestNames;

impl Rule for PreferCamelCaseTestNames {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-camel-case-test-names"
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
        rename_tests(
            node,
            ancestors,
            ctx,
            out,
            to_camel_case,
            "Name the test method in camelCase.",
        );
    }
}

/// `PreferTestsWithSnakeCaseRector`.
pub struct PreferSnakeCaseTestNames;

impl Rule for PreferSnakeCaseTestNames {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-snake-case-test-names"
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
        rename_tests(
            node,
            ancestors,
            ctx,
            out,
            to_snake_case,
            "Name the test method in snake_case.",
        );
    }
}

/// `ExplicitMockExpectsCallRector`.
pub struct RequireExplicitMockExpects;

impl Rule for RequireExplicitMockExpects {
    fn id(&self) -> &'static str {
        "lint/phpunit/require-explicit-mock-expects"
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
        let Node::Method(method) = node else { return };
        if !is_in_test_class(ctx, ancestors, Some(method))
            || !is_test_class_method(ctx, ancestors, method)
        {
            return;
        }
        let MethodBody::Concrete(body) = &method.body else {
            return;
        };
        let facts = facts(ctx);
        let mut stack = vec![Node::Block(body)];
        while let Some(node) = stack.pop() {
            node.visit_children(|child| stack.push(child));
            let Node::MethodCall(call) = node else {
                continue;
            };
            let Some(any) = AnyCall::of(node) else {
                continue;
            };
            if !any.is_name(&["method"]) || !matches!(call.object, Expression::Variable(_)) {
                continue;
            }
            // a nullable mock counts, as for Rector's `isObjectType()`
            if !facts.full_type(node_span(call.object)).is_some_and(|t| {
                let mut kept = t
                    .types
                    .iter()
                    .filter(|a| !matches!(a, TAtomic::Null))
                    .peekable();
                kept.peek().is_some() && kept.all(|a| is_mock_object(&facts.codebase, a))
            }) {
                continue;
            }
            let at = node_span(call.object).end;
            out.push(Report::fixed(
                node_span(call),
                "Add an explicit `expects($this->atLeastOnce())` to the mocked method.",
                Safety::Unsafe,
                vec![Edit::new(
                    Span::new(at, at),
                    "->expects($this->atLeastOnce())",
                )],
            ));
        }
    }
}

/// The call of `name` on an `Assert` (a `TestCase` too) that the statement is.
fn assert_call<'a>(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    expr: &'a Expression<'a>,
    name: &str,
) -> Option<AnyCall<'a>> {
    let call = match expr {
        Expression::Call(mago_syntax::cst::Call::Method(m)) => AnyCall::of(Node::MethodCall(m)),
        Expression::Call(mago_syntax::cst::Call::StaticMethod(s)) => {
            AnyCall::of(Node::StaticMethodCall(s))
        }
        _ => None,
    }?;
    (call.is_name(&[name]) && receiver_is(ctx, ancestors, &call, &[ASSERT])).then_some(call)
}

/// `AddKeysExistsAssertForKeyUseRector`.
pub struct RequireArrayKeyAssert;

impl Rule for RequireArrayKeyAssert {
    fn id(&self) -> &'static str {
        "lint/phpunit/require-array-key-assert"
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
        let Node::Method(method) = node else { return };
        if !is_test_class_method(ctx, ancestors, method) {
            return;
        }
        let MethodBody::Concrete(body) = &method.body else {
            return;
        };
        let statements: Vec<&Statement<'_>> = body.statements.iter().collect();
        if statements.len() < 2 {
            return;
        }
        let mut chain = ancestors.to_vec();
        chain.push(node);
        let in_test_case =
            own_class_reflection(ctx, ancestors).is_some_and(|c| is_a(ctx, &c, &[TEST_CASE]));
        let mut known: Vec<(&[u8], Vec<u8>)> = Vec::new();
        for (i, statement) in statements.iter().enumerate() {
            let Statement::Expression(statement) = statement else {
                continue;
            };
            if let Some(call) = assert_call(ctx, &chain, statement.expression, "assertArrayHasKey")
            {
                let args = call.args();
                if let (Some(key), Some(array)) = (args.first(), args.get(1))
                    && let Expression::Literal(Literal::String(key)) = key.value()
                    && let Expression::Variable(Variable::Direct(array)) = array.value()
                {
                    known.push((array.name, key.value.unwrap_or_default().to_vec()));
                }
                continue;
            }
            let Some(call) = assert_call(ctx, &chain, statement.expression, "assertSame") else {
                continue;
            };
            let Some(asserted) = call.args().get(1).map(|a| a.value()) else {
                continue;
            };
            let Expression::ArrayAccess(fetch) = asserted else {
                continue;
            };
            let Expression::Variable(Variable::Direct(array)) = fetch.array else {
                continue;
            };
            // Rector inserts again on every run for a key that is not a string literal, which the inserted
            // `assertArrayHasKey()` never makes known; Pun leaves those alone
            let Expression::Literal(Literal::String(dim)) = fetch.index else {
                continue;
            };
            let dim = dim.value.map(<[u8]>::to_vec);
            if known.iter().any(|(name, key)| {
                name.eq_ignore_ascii_case(array.name) && dim.as_ref().is_some_and(|d| d == key)
            }) {
                continue;
            }
            let variable = String::from_utf8_lossy(array.name);
            let receiver = if in_test_case {
                "$this->"
            } else {
                "\\PHPUnit\\Framework\\Assert::"
            };
            let code = format!(
                "{receiver}assertArrayHasKey({}, {variable}, 'Existing keys are: ' . implode(', ', array_keys({variable})));",
                text(ctx, fetch.index)
            );
            let start = node_span(statement).start;
            let indent = line_indent(ctx.source, start);
            let edit = match i.checked_sub(1).map(|p| node_span(statements[p]).end) {
                Some(end) => Edit::new(Span::new(end, end), format!("\n{indent}{code}")),
                None => Edit::new(Span::new(start, start), format!("{code}\n{indent}")),
            };
            out.push(Report::fixed(
                node_span(statement),
                "Assert that the array has the key before asserting its value.",
                Safety::Unsafe,
                vec![edit],
            ));
        }
    }
}

/// `TestClassNameResolver::resolve()`: `<class>Test`, then `Tests\` and `Test\` inserted before each part.
fn possible_test_class_names(class: &str) -> Vec<String> {
    let parts: Vec<&str> = class.split('\\').collect();
    let mut names = vec![format!("{class}Test")];
    for i in 0..parts.len() {
        for inserted in ["Tests", "Test"] {
            let mut with = parts.clone();
            with.insert(i, inserted);
            names.push(format!("{}Test", with.join("\\")));
        }
    }
    names
}

/// `AddSeeTestAnnotationRector`.
pub struct AddSeeTestTag;

impl Rule for AddSeeTestTag {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-see-test-tag"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Class(class) = node else { return };
        if class.name.value.ends_with(b"Test") {
            return;
        }
        let facts = facts(ctx);
        let has_class = |name: &str| facts.codebase.get_class_like(name.as_bytes()).is_some();
        let start = node_span(class).start;
        let doc = Doc::of(ctx, ancestors, start);
        let sees: Vec<String> = doc.as_ref().map_or_else(Vec::new, |doc| {
            (0..doc.len())
                .filter_map(|i| {
                    let words = doc.words(i);
                    (words.first() == Some(&"@see")).then(|| words[1..].join(" "))
                })
                .collect()
        });
        let namespace = qualified(ancestors, b"");
        let namespace = namespace.trim_end_matches('\\');
        // an @see that already names a class, as written, imported or in the current namespace
        if sees.iter().any(|see| {
            let see = see.trim_start_matches('\\');
            has_class(see)
                || has_class(&imported(ancestors, see).unwrap_or_else(|| {
                    if namespace.is_empty() {
                        see.to_string()
                    } else {
                        format!("{namespace}\\{see}")
                    }
                }))
        }) {
            return;
        }
        let name = qualified(ancestors, class.name.value);
        let name = name.trim_start_matches('\\');
        let Some(test) = possible_test_class_names(name)
            .into_iter()
            .find(|n| has_class(n))
        else {
            return;
        };
        let tag = format!("@see \\{test}");
        if sees.contains(&format!("\\{test}")) {
            return;
        }
        let edit = match &doc {
            Some(doc) => Edit::new(
                doc.span,
                doc.print_appending(&vec![false; doc.len()], &[&tag]),
            ),
            None => {
                let indent = " ".repeat(indentation(ctx.source, start));
                Edit::new(
                    Span::new(start, start),
                    format!("/**\n{indent} * {tag}\n{indent} */\n{indent}"),
                )
            }
        };
        out.push(Report::fixed(
            node_span(&class.name),
            "Add an `@see` tag that names the class's test case.",
            Safety::Safe,
            vec![edit],
        ));
    }
}
