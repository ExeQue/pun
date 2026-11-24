//! rector-phpunit's `PropertyExistsWithoutAssertRector` (PHPUnit 10.0): the removed `assertClassHasAttribute()`
//! family becomes `assertTrue(property_exists(...))` or `assertFalse(property_exists(...))`.

use crate::problem::Safety;
use mago_syntax::cst::{Node, NodeKind};

use super::{AnyCall, NewArg, expr_text, is_test_case_call_named, renamed, rewrite_args};
use crate::{Context, Report, Rule};

const RENAME_METHODS_WITH_OBJECT_MAP: &[(&str, &str)] = &[
    ("assertClassHasAttribute", "assertTrue"),
    ("assertClassHasStaticAttribute", "assertTrue"),
    ("assertClassNotHasStaticAttribute", "assertFalse"),
    ("assertClassNotHasAttribute", "assertFalse"),
    ("objectHasAttribute", "assertTrue"),
    ("classHasAttribute", "assertTrue"),
    ("classHasStaticAttribute", "assertTrue"),
];

pub struct NoAssertClassHasAttribute;

impl Rule for NoAssertClassHasAttribute {
    fn id(&self) -> &'static str {
        "lint/phpunit/no-assert-class-has-attribute"
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
        let call = AnyCall::of(node).unwrap();
        let names: Vec<&str> = RENAME_METHODS_WITH_OBJECT_MAP
            .iter()
            .map(|(k, _)| *k)
            .collect();
        if !is_test_case_call_named(ctx, ancestors, &call, &names) {
            return;
        }
        let args = call.args();
        if args.len() < 2 {
            return;
        }
        let new_name = renamed(&call, RENAME_METHODS_WITH_OBJECT_MAP).unwrap();
        let mut new = vec![NewArg::New(format!(
            "property_exists({}, {})",
            expr_text(ctx, args[1].value()),
            expr_text(ctx, args[0].value())
        ))];
        new.extend((2..args.len()).map(NewArg::Old));
        out.push(Report::fixed(
            call.span,
            format!("Use `{new_name}(property_exists(...))`; PHPUnit 10 removed this assertion."),
            Safety::Unsafe,
            rewrite_args(ctx, &call, Some(new_name), &new),
        ));
    }
}
