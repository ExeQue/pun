//! rector-phpunit's `SpecificAssertContainsWithoutIdentityRector` (PHPUnit 9.0).

use crate::problem::Safety;
use mago_codex::ttype::atomic::TAtomic;
use mago_syntax::cst::{Node, NodeKind};

use super::{AnyCall, NewArg, is_test_case_call_named, renamed, rewrite_args};
use crate::rules::support::expressions::is_constant;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const OLD_TO_NEW_METHOD_NAMES: &[(&str, &str)] = &[
    ("assertContains", "assertContainsEquals"),
    ("assertNotContains", "assertNotContainsEquals"),
];

pub struct PreferAssertContainsEquals;

impl Rule for PreferAssertContainsEquals {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-assert-contains-equals"
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
        let args = call.args();
        if args.len() < 2 {
            return;
        }
        // a string haystack: SpecificAssertContainsRector's case
        let is_string = facts(ctx)
            .full_type(node_span(args[1].value()))
            .is_some_and(|t| matches!(t.types.as_ref(), [TAtomic::Scalar(s)] if s.is_any_string()));
        if is_string {
            return;
        }
        let Some(identity) = args.get(4) else {
            return;
        };
        if is_constant(identity.value(), "true") {
            return;
        }
        let new = renamed(&call, OLD_TO_NEW_METHOD_NAMES).unwrap();
        let kept: Vec<NewArg> = (0..args.len())
            .filter(|i| !matches!(i, 3 | 4))
            .map(NewArg::Old)
            .collect();
        out.push(Report::fixed(
            call.name_span(),
            format!("Use `{new}()` instead of the non-identity flags."),
            Safety::Safe,
            rewrite_args(ctx, &call, Some(new), &kept),
        ));
    }
}
