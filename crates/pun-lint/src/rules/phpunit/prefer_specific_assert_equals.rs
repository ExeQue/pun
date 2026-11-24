//! rector-phpunit's `AssertEqualsParameterToSpecificMethodsTypeRector` (PHPUnit 7.5): `assertEquals()`'s `$delta`,
//! `$maxDepth`, `$canonicalize` and `$ignoreCase` parameters become `assertEqualsWithDelta()`,
//! `assertEqualsCanonicalizing()` and `assertEqualsIgnoringCase()`.

use crate::problem::Safety;
use mago_syntax::cst::{Argument, Node, NodeKind};

use super::{AnyCall, NewArg, is_test_case_call_named, rewrite_args, string_value};
use crate::rules::support::expressions::is_constant;
use crate::rules::support::values::{Value, value};
use crate::{Context, Report, Rule};

pub struct PreferSpecificAssertEquals;

impl Rule for PreferSpecificAssertEquals {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-specific-assert-equals"
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
        if !is_test_case_call_named(ctx, ancestors, &call, &["assertEquals", "assertNotEquals"]) {
            return;
        }
        let args = call.args();
        let Some((name, new)) = refactor(&args) else {
            return;
        };
        out.push(Report::fixed(
            call.span,
            match name {
                Some(name) => format!("Use `{name}()` instead of `assertEquals()`'s parameter."),
                None => "Remove the parameter `assertEquals()` no longer takes.".to_string(),
            },
            Safety::Safe,
            rewrite_args(ctx, &call, name, &new),
        ));
    }
}

/// The new name (`None`: the same) and arguments, as `refactor()` builds them.
fn refactor(args: &[&Argument<'_>]) -> Option<(Option<&'static str>, Vec<NewArg>)> {
    let kept = |removed: &[usize]| -> Vec<NewArg> {
        (0..args.len())
            .filter(|i| !removed.contains(i))
            .map(NewArg::Old)
            .collect()
    };
    // `$message` stays only when it is not ''
    let specific = |name, extra: Option<usize>| {
        let mut new = vec![NewArg::Old(0), NewArg::Old(1)];
        new.extend(extra.map(NewArg::Old));
        if string_value(args[2].value()).as_deref() != Some("") {
            new.push(NewArg::Old(2));
        }
        Some((Some(name), new))
    };
    // 1. `$ignoreCase`
    if let Some(ignore_case) = args.get(6) {
        if is_constant(ignore_case.value(), "false") {
            return Some((None, kept(&[6])));
        }
        return specific("assertEqualsIgnoringCase", None);
    }
    // 2. `$canonicalize`
    if let Some(canonicalize) = args.get(5) {
        if !is_constant(canonicalize.value(), "true") {
            return Some((None, kept(&[5])));
        }
        return specific("assertEqualsCanonicalizing", None);
    }
    // 3. `$maxDepth`, then `$delta`
    let mut removed = Vec::new();
    if args.get(4).is_some() {
        removed.push(4);
    }
    if let Some(delta) = args.get(3) {
        removed.push(3);
        if !matches!(value(delta.value()), Some(Value::Float(f)) if f == 0.0) {
            return specific("assertEqualsWithDelta", Some(3));
        }
    }
    (!removed.is_empty()).then(|| (None, kept(&removed)))
}
