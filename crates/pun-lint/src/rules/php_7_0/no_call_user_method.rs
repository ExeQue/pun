//! Rector 2.8.4's `CallUserMethodRector`.

use mago_syntax::cst::{Argument, Expression, Identifier, Node, NodeKind};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::arguments::{unpacked, value};
use crate::rules::support::expressions::text;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

/// `OLD_TO_NEW_FUNCTIONS`.
const OLD_TO_NEW_FUNCTIONS: &[(&str, &str)] = &[
    ("call_user_method", "call_user_func"),
    ("call_user_method_array", "call_user_func_array"),
];

pub struct NoCallUserMethod;

impl Rule for NoCallUserMethod {
    fn id(&self) -> &'static str {
        "lint/php-7.0/no-call-user-method"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70000)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        // `isset(self::OLD_TO_NEW_FUNCTIONS[$this->getName($node)])`: the name as written, case-sensitive.
        let written: &[u8] = match call.function {
            Expression::Identifier(Identifier::Local(id)) => id.value,
            Expression::Identifier(Identifier::FullyQualified(id)) => &id.value[1..],
            _ => return,
        };
        let Some((old, new)) = OLD_TO_NEW_FUNCTIONS
            .iter()
            .find(|(old, _)| old.as_bytes() == written)
        else {
            return;
        };
        // The method and the object become the callable `[$object, $method]`; Rector needs both.
        let [method, object, ..] = call.argument_list.arguments.as_slice() else {
            return;
        };
        if [method, object]
            .iter()
            .any(|a| unpacked(a) || matches!(a, Argument::Named(_)))
        {
            return;
        }
        let callable = format!(
            "[{}, {}]",
            text(ctx, value(object)),
            text(ctx, value(method))
        );
        out.push(Report::fixed(
            node_span(&node),
            format!("Call `{new}()` instead of `{old}()`, which PHP 7.0 removed."),
            Safety::Safe,
            vec![
                Edit::new(node_span(call.function), *new),
                Edit::new(
                    Span::new(node_span(method).start, node_span(object).end),
                    callable,
                ),
            ],
        ));
    }
}
