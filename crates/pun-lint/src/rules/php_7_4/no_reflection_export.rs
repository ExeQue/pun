//! Rector 2.8.4's `ExportToReflectionFunctionRector`.

use mago_syntax::cst::{ClassLikeMemberSelector, Expression, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::laravel::is_a;
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{Prec, in_parent, text};
use crate::rules::support::names::class_name;
use crate::rules::support::values::{Value, value as literal};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoReflectionExport;

impl Rule for NoReflectionExport {
    fn id(&self) -> &'static str {
        "lint/php-7.4/no-reflection-export"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70400)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::StaticMethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::StaticMethodCall(call) = node else {
            return;
        };
        // `self`, `static` and `parent` need the enclosing class's type; only a written class name is read.
        let Expression::Identifier(class) = call.class else {
            return;
        };
        let ClassLikeMemberSelector::Identifier(method) = &call.method else {
            return;
        };
        if !method.value.eq_ignore_ascii_case(b"export") {
            return;
        }
        let args = call.argument_list.arguments.as_slice();
        let Some(first) = args.first() else {
            return;
        };
        // `$callerType->isSuperTypeOf(new ObjectType('ReflectionFunction'))`.
        let name = class_name(ancestors, class);
        if !is_a(ctx, "ReflectionFunction", &[&name]) {
            return;
        }
        let new = format!("new {}({})", text(ctx, class), text(ctx, value(first)));
        let as_string = args
            .get(1)
            .is_some_and(|second| literal(value(second)) == Some(Value::Bool(true)));
        let span = node_span(call);
        let code = if as_string {
            in_parent(span, ancestors, format!("(string) {new}"), Prec::Prefix(10))
        } else {
            new
        };
        out.push(Report::fixed(
            span,
            "`Reflection*::export()` is removed in PHP 8.0; create the reflection with `new`.",
            Safety::Safe,
            vec![Edit::new(span, code)],
        ));
    }
}
