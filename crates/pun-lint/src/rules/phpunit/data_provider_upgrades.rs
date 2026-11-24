//! rector-phpunit's upgrade rules for data providers and test docblocks: `PublicDataProviderClassMethodRector`
//! (PHPUnit 10.0), `RemoveDataProviderTestPrefixRector` and `ScenarioToTestAnnotationRector` (PHPUnit 7.0).

use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{ClassLikeMember, Method, Node, NodeKind};

use super::{class_body, is_in_test_class};
use crate::rules::refactor::configured_codemods::{
    PRIVATE, PROTECTED, PUBLIC, flag, print_modifiers,
};
use crate::rules::support::class_members::{member_from, method_data_providers, with_class_info};
use crate::rules::support::docblocks::doc_comment;
use crate::rules::support::rewrites::merge;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

/// `DataProviderClassMethodFinder::find()` on the class's own methods.
fn data_providers<'a>(
    ctx: &Context<'_>,
    node: Node<'a, 'a>,
    ancestors: &[Node<'a, 'a>],
) -> Vec<&'a Method<'a>> {
    let facts = facts(ctx);
    let mut out: Vec<&'a Method<'a>> = Vec::new();
    with_class_info(node, ancestors, &facts, |class| {
        let names: Vec<Vec<u8>> = (0..class.members.len())
            .flat_map(|i| method_data_providers(ctx.source, ancestors, &class, i, true))
            .collect();
        for member in class.members {
            if let ClassLikeMember::Method(method) = member
                && names
                    .iter()
                    .any(|n| n.eq_ignore_ascii_case(method.name.value))
                && !out.iter().any(|m| std::ptr::eq(*m, method))
            {
                out.push(method);
            }
        }
    });
    out
}

fn in_test_class(ctx: &Context<'_>, node: Node<'_, '_>, ancestors: &[Node<'_, '_>]) -> bool {
    let mut chain = ancestors.to_vec();
    chain.push(node);
    is_in_test_class(ctx, &chain, None)
}

/// `PublicDataProviderClassMethodRector`.
pub struct RequirePublicDataProvider;

impl Rule for RequirePublicDataProvider {
    fn id(&self) -> &'static str {
        "lint/phpunit/require-public-data-provider"
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
        if !in_test_class(ctx, node, ancestors) {
            return;
        }
        for method in data_providers(ctx, node, ancestors) {
            let flags = method.modifiers.iter().map(flag).fold(0, |a, f| a | f);
            if flags & (PRIVATE | PROTECTED) == 0 {
                continue;
            }
            // VisibilityManipulator::makePublic()
            let new = (flags & !(PRIVATE | PROTECTED)) | PUBLIC;
            let at = node_span(&method.function).start;
            let start = node_span(method.modifiers.first().unwrap()).start;
            out.push(Report::fixed(
                node_span(&method.name),
                "Make the data provider public; PHPUnit 10 requires it.",
                Safety::Safe,
                vec![Edit::new(Span::new(start, at), print_modifiers(new))],
            ));
        }
    }
}

/// The value of a `@dataProvider` tag on a docblock line, and where it starts.
fn data_provider_value(line: &str) -> Option<(usize, &str)> {
    let at = line.find("@dataProvider")?;
    if !line[..at]
        .trim_start()
        .trim_start_matches("/**")
        .trim_start_matches('*')
        .trim()
        .is_empty()
    {
        return None;
    }
    let after = &line[at + "@dataProvider".len()..];
    if !after.starts_with([' ', '\t']) {
        return None;
    }
    let value = after.trim_start();
    let start = line.len() - value.len();
    let value = value.trim_end();
    Some((start, value.strip_suffix("*/").unwrap_or(value).trim_end()))
}

/// `Strings::substring($name, 4)`, then `lcfirst()`.
fn without_test_prefix(name: &str) -> String {
    let rest = &name[4..];
    let mut chars = rest.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_lowercase(), chars.as_str()),
        None => String::new(),
    }
}

/// `RemoveDataProviderTestPrefixRector`.
pub struct NoDataProviderTestPrefix;

impl Rule for NoDataProviderTestPrefix {
    fn id(&self) -> &'static str {
        "lint/phpunit/no-data-provider-test-prefix"
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
        if !in_test_class(ctx, node, ancestors) {
            return;
        }
        let mut edits = Vec::new();
        for method in data_providers(ctx, node, ancestors) {
            let name = String::from_utf8_lossy(method.name.value);
            if name.starts_with("test") {
                edits.push(Edit::new(
                    node_span(&method.name),
                    without_test_prefix(&name),
                ));
            }
        }
        if edits.is_empty() {
            return;
        }
        // DataProviderMethodRenamer::removeTestPrefix()
        let source = ctx.source;
        let (members, left, _) = class_body(node).unwrap();
        for (index, member) in members.iter().enumerate() {
            if !matches!(member, ClassLikeMember::Method(_)) {
                continue;
            }
            let from = member_from(members, left, index);
            let Some(doc) = doc_comment(source, from, node_span(member).start) else {
                continue;
            };
            let mut at = doc.start;
            for line in source[doc.start..doc.end].split_inclusive('\n') {
                if let Some((start, value)) = data_provider_value(line)
                    && value.starts_with("test")
                {
                    let start = at + start;
                    edits.push(Edit::new(
                        Span::new(start, start + value.len()),
                        without_test_prefix(value),
                    ));
                }
                at += line.len();
            }
        }
        out.push(Report::fixed(
            node_span(&node),
            "Remove the `test` prefix from the data provider's name.",
            Safety::Unsafe,
            merge(edits),
        ));
    }
}

/// `ScenarioToTestAnnotationRector`.
pub struct NoScenarioAnnotation;

impl Rule for NoScenarioAnnotation {
    fn id(&self) -> &'static str {
        "lint/phpunit/no-scenario-annotation"
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
        if !is_in_test_class(ctx, ancestors, Some(method)) {
            return;
        }
        let source = ctx.source;
        let span = node_span(method);
        let Some((members, left, _)) = ancestors.iter().rev().find_map(|a| class_body(*a)) else {
            return;
        };
        let Some(index) = members
            .iter()
            .position(|m| node_span(m).start == span.start)
        else {
            return;
        };
        let Some(doc) = doc_comment(source, member_from(members, left, index), span.start) else {
            return;
        };
        let Some(new) = scenario_to_test(&source[doc.start..doc.end]) else {
            return;
        };
        out.push(Report::fixed(
            node_span(&method.name),
            "Replace the `@scenario` tag, which PHPUnit 7 dropped, with `@test`.",
            Safety::Safe,
            vec![Edit::new(doc, new)],
        ));
    }
}

fn is_scenario(tag: &str) -> bool {
    tag.strip_prefix("@scenario")
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
}

/// The docblock with its `@scenario` tags removed and a `@test` tag appended for each.
fn scenario_to_test(doc: &str) -> Option<String> {
    if !doc.contains('\n') {
        let inner = doc.strip_prefix("/**")?.strip_suffix("*/")?.trim();
        return is_scenario(inner).then(|| "/** @test */".to_string());
    }
    let lines: Vec<&str> = doc.split('\n').collect();
    let last = lines.len() - 1;
    let mut kept: Vec<&str> = Vec::new();
    let mut found = 0;
    for (i, line) in lines.iter().enumerate() {
        let body = line.trim_start().strip_prefix('*').map(str::trim);
        if i > 0 && i < last && body.is_some_and(is_scenario) {
            found += 1;
            continue;
        }
        kept.push(line);
    }
    if found == 0 {
        return None;
    }
    // one `@test` per removed `@scenario`, after the other tags
    let close = kept.pop()?;
    let indent = &close[..close.len() - close.trim_start().len()];
    let tag = format!("{indent}* @test");
    kept.extend(std::iter::repeat_n(tag.as_str(), found));
    kept.push(close);
    Some(kept.join("\n"))
}
