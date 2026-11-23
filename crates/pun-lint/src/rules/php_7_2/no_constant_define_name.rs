//! Rector 2.8.4's `StringifyDefineRector`.

use mago_syntax::cst::{Argument, Expression, Identifier, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::named;
use crate::rules::support::names::{imported_member, resolve_constant_name};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoConstantDefineName;

impl Rule for NoConstantDefineName {
    fn id(&self) -> &'static str {
        "lint/php-7.2/no-constant-define-name"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70200)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        if !named(call, "define") {
            return;
        }
        let Some(first) = call.argument_list.arguments.first() else {
            return;
        };
        if matches!(first, Argument::Positional(a) if a.ellipsis.is_some()) {
            return;
        }
        // `$firstArg->value instanceof ConstFetch`: a bare constant name becomes its string.
        let Expression::ConstantAccess(constant) = unparenthesize(value(first)) else {
            return;
        };
        // `getName()`: the resolved name, but an unqualified one that is not imported falls back to the global name.
        let name = match &constant.name {
            Identifier::Local(local) => imported_member(ancestors, local.value, false)
                .unwrap_or_else(|| String::from_utf8_lossy(local.value).into_owned()),
            other => resolve_constant_name(ancestors, other),
        };
        out.push(Report::fixed(
            node_span(constant),
            format!("Quote the constant name `{name}` passed to `define()`."),
            Safety::Safe,
            vec![Edit::new(node_span(constant), format!("'{name}'"))],
        ));
    }
}
