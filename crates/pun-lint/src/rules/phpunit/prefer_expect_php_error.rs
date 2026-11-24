//! rector-phpunit's `ExplicitPhpErrorApiRector` (PHPUnit 8.4): `expectException()` of PHPUnit's PHP error classes
//! becomes `expectNotice()`, `expectDeprecation()`, `expectError()` or `expectWarning()`.

use crate::problem::{Edit, Safety};
use mago_syntax::cst::{Access, ClassLikeConstantSelector, Expression, Node, NodeKind};

use super::{AnyCall, is_test_case_call_named, resolve};
use crate::syntax::unparenthesize;
use crate::{Context, Report, Rule};

const REPLACEMENTS: &[(&str, &str)] = &[
    ("PHPUnit\\Framework\\TestCase\\Notice", "expectNotice"),
    (
        "PHPUnit\\Framework\\TestCase\\Deprecated",
        "expectDeprecation",
    ),
    ("PHPUnit\\Framework\\TestCase\\Error", "expectError"),
    ("PHPUnit\\Framework\\TestCase\\Warning", "expectWarning"),
];

pub struct PreferExpectPhpError;

impl Rule for PreferExpectPhpError {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-expect-php-error"
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
        if !is_test_case_call_named(ctx, ancestors, &call, &["expectException"]) {
            return;
        }
        let Some(first) = call.args().first().copied() else {
            return;
        };
        let Expression::Access(Access::ClassConstant(access)) = unparenthesize(first.value())
        else {
            return;
        };
        let (Expression::Identifier(class), ClassLikeConstantSelector::Identifier(constant)) =
            (access.class, &access.constant)
        else {
            return;
        };
        if !constant.value.eq_ignore_ascii_case(b"class") {
            return;
        }
        let class = resolve(ancestors, class);
        let Some((_, method)) = REPLACEMENTS
            .iter()
            .find(|(c, _)| c.eq_ignore_ascii_case(&class))
        else {
            return;
        };
        out.push(Report::fixed(
            call.span,
            format!("Use `{method}()` instead of expecting PHPUnit's PHP error class."),
            Safety::Safe,
            vec![Edit::new(
                call.span,
                format!("{}{method}()", call.receiver_text(ctx)),
            )],
        ));
    }
}
