//! Rector 2.8.4's `GetDebugTypeRector`.

use mago_syntax::cst::{
    Access, Argument, Call, ClassLikeConstantSelector, Expression, FunctionCall, Node, NodeKind,
};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{call_to, equal, named, text};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferGetDebugType;

impl Rule for PreferGetDebugType {
    fn id(&self) -> &'static str {
        "lint/php-8.0/prefer-get-debug-type"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80000)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Conditional])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Conditional(ternary) = node else {
            return;
        };
        // `shouldSkip`.
        let Some(object) = call_to(ternary.condition, "is_object").and_then(first) else {
            return;
        };
        let Some(then) = ternary.then else {
            return;
        };
        let class = match unparenthesize(then) {
            Expression::Call(Call::Function(call)) if named(call, "get_class") => first(call),
            Expression::Access(Access::ClassConstant(fetch)) if matches!(&fetch.constant, ClassLikeConstantSelector::Identifier(c) if c.value == b"class") => {
                Some(fetch.class)
            }
            _ => return,
        };
        let Some(class) = class else {
            return;
        };
        let Some(r#type) = call_to(ternary.r#else, "gettype").and_then(first) else {
            return;
        };
        // `areValuesIdentical`.
        if !equal(ctx, object, class) || !equal(ctx, object, r#type) {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Use `get_debug_type()` instead of `is_object()`, `get_class()` and `gettype()`.",
            Safety::Unsafe,
            vec![Edit::new(
                node_span(&node),
                format!("get_debug_type({})", text(ctx, object)),
            )],
        ));
    }
}

/// The value of the first argument, when it is not unpacked or a first-class callable's `...`.
fn first<'a>(call: &'a FunctionCall<'a>) -> Option<&'a Expression<'a>> {
    match call.argument_list.arguments.first()? {
        Argument::Positional(a) if a.ellipsis.is_some() => None,
        arg => Some(value(arg)),
    }
}
