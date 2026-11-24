//! rector-phpunit's `TestListenerToHooksRector` (PHPUnit 9.0): a `TestListener` becomes the `PHPUnit\Runner\*Hook`
//! interfaces of its non-empty methods, which take the hooks' names; its empty methods are removed.

use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{ClassLikeMember, MethodBody, Node, NodeKind};

use super::resolve;
use crate::rules::support::class_members::remove_members;
use crate::rules::support::rewrites::merge;
use crate::rules::support::statement_edits::line_indent;
use crate::rules::types::class_scope;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const TEST_LISTENER: &str = "PHPUnit\\Framework\\TestListener";

const LISTENER_METHOD_TO_HOOK_INTERFACES: &[(&str, &str, &str)] = &[
    (
        "addIncompleteTest",
        "PHPUnit\\Runner\\AfterIncompleteTestHook",
        "executeAfterIncompleteTest",
    ),
    (
        "addRiskyTest",
        "PHPUnit\\Runner\\AfterRiskyTestHook",
        "executeAfterRiskyTest",
    ),
    (
        "addSkippedTest",
        "PHPUnit\\Runner\\AfterSkippedTestHook",
        "executeAfterSkippedTest",
    ),
    (
        "addError",
        "PHPUnit\\Runner\\AfterTestErrorHook",
        "executeAfterTestError",
    ),
    (
        "addFailure",
        "PHPUnit\\Runner\\AfterTestFailureHook",
        "executeAfterTestFailure",
    ),
    (
        "addWarning",
        "PHPUnit\\Runner\\AfterTestWarningHook",
        "executeAfterTestWarning",
    ),
    (
        "startTest",
        "PHPUnit\\Runner\\BeforeTestHook",
        "executeBeforeTest",
    ),
    (
        "endTest",
        "PHPUnit\\Runner\\AfterTestHook",
        "executeAfterTest",
    ),
    (
        "startTestSuite",
        "PHPUnit\\Runner\\BeforeFirstTestHook",
        "executeBeforeFirstTest",
    ),
    (
        "endTestSuite",
        "PHPUnit\\Runner\\AfterLastTestHook",
        "executeAfterLastTest",
    ),
];

pub struct NoTestListener;

impl Rule for NoTestListener {
    fn id(&self) -> &'static str {
        "lint/phpunit/no-test-listener"
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
        let source = ctx.source;
        let text = |s: Span| &source[s.start..s.end];
        let implemented: Vec<(Span, String)> = class
            .implements
            .iter()
            .flat_map(|i| i.types.iter())
            .map(|t| (node_span(t), resolve(ancestors, t)))
            .collect();
        let declared = implemented
            .iter()
            .any(|(_, n)| n.eq_ignore_ascii_case(TEST_LISTENER));
        if !declared && !is_test_listener(ctx, node, ancestors) {
            return;
        }
        let mut names: Vec<String> = implemented
            .iter()
            .filter(|(_, n)| !n.eq_ignore_ascii_case(TEST_LISTENER))
            .map(|(s, _)| text(*s).to_string())
            .collect();
        let mut removed = vec![false; class.members.len()];
        let mut edits = Vec::new();
        for (index, member) in class.members.iter().enumerate() {
            let ClassLikeMember::Method(method) = member else {
                continue;
            };
            for (old, hook, new) in LISTENER_METHOD_TO_HOOK_INTERFACES {
                if !method.name.value.eq_ignore_ascii_case(old.as_bytes()) {
                    continue;
                }
                let empty = match &method.body {
                    MethodBody::Concrete(block) => {
                        let inner = Span::new(
                            block.left_brace.end.offset as usize,
                            block.right_brace.start.offset as usize,
                        );
                        text(inner).trim().is_empty()
                    }
                    MethodBody::Abstract(_) => true,
                };
                if empty {
                    removed[index] = true;
                } else {
                    names.push(format!("\\{hook}"));
                    edits.push(Edit::new(node_span(&method.name), *new));
                }
            }
        }
        // the implements list, reprinted
        let header_from = class
            .extends
            .as_ref()
            .map_or(node_span(&class.name).end, |e| node_span(e).end);
        let header = match names.is_empty() {
            true => String::new(),
            false => format!(" implements {}", names.join(", ")),
        };
        let header_to = class
            .implements
            .as_ref()
            .map_or(header_from, |i| node_span(i).end);
        if !removed.is_empty() && removed.iter().all(|r| *r) {
            let indent = line_indent(source, node_span(class).start);
            let start = node_span(class).start;
            let head = &source[start..header_from];
            edits = vec![Edit::new(
                Span::new(start, class.right_brace.end.offset as usize),
                format!("{head}{header}\n{indent}{{\n{indent}}}"),
            )];
        } else {
            if header_to != header_from || !header.is_empty() {
                edits.push(Edit::new(Span::new(header_from, header_to), header));
            }
            if removed.contains(&true) {
                edits.extend(remove_members(
                    source,
                    &class.members,
                    class.left_brace,
                    class.right_brace,
                    &removed,
                ));
            }
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report::fixed(
            node_span(&class.name),
            "Implement PHPUnit's hook interfaces instead of `TestListener`.",
            Safety::Unsafe,
            merge(edits),
        ));
    }
}

fn is_test_listener(ctx: &Context<'_>, node: Node<'_, '_>, ancestors: &[Node<'_, '_>]) -> bool {
    let mut chain = ancestors.to_vec();
    chain.push(node);
    let Some(name) = class_scope(&chain).and_then(|s| s.name) else {
        return false;
    };
    facts(ctx)
        .codebase
        .is_instance_of(name.as_bytes(), TEST_LISTENER.as_bytes())
}
