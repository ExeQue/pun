//! rector-phpunit's `SpecificAssertContainsRector` (PHPUnit 7.5).

use crate::problem::{Edit, Safety};
use mago_codex::ttype::atomic::TAtomic;
use mago_syntax::cst::{Node, NodeKind};

use super::{AnyCall, is_test_case_call_named, renamed};
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const OLD_TO_NEW_METHOD_NAMES: &[(&str, &str)] = &[
    ("assertContains", "assertStringContainsString"),
    ("assertNotContains", "assertStringNotContainsString"),
];

pub struct NoAssertContainsOnString;

impl Rule for NoAssertContainsOnString {
    fn id(&self) -> &'static str {
        "lint/phpunit/no-assert-contains-on-string"
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
        let call = AnyCall::of(node).unwrap();
        if !is_test_case_call_named(
            ctx,
            ancestors,
            &call,
            &["assertContains", "assertNotContains"],
        ) {
            return;
        }
        let Some(haystack) = call.args().get(1).copied() else {
            return;
        };
        if !is_possibly_string_type(ctx, node_span(haystack.value())) {
            return;
        }
        let new = renamed(&call, OLD_TO_NEW_METHOD_NAMES).unwrap();
        out.push(Report::fixed(
            call.name_span(),
            format!("Use `{new}()` on a string haystack."),
            Safety::Safe,
            vec![Edit::new(call.name_span(), new)],
        ));
    }
}

/// `isPossiblyStringType()`: the type without `null` and its falsy members is a string.
fn is_possibly_string_type(ctx: &Context<'_>, span: crate::problem::Span) -> bool {
    facts(ctx).full_type(span).is_some_and(|t| {
        let kept: Vec<&TAtomic> = (t.types.iter())
            .filter(|a| !a.is_null() && !a.is_falsy())
            .collect();
        // an empty remainder is `never`, which PHPStan does not count as a string
        !kept.is_empty()
            && kept
                .iter()
                .all(|a| matches!(a, TAtomic::Scalar(s) if s.is_any_string()))
    })
}
