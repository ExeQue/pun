//! Rector 2.8.4's `ArrayKeyExistsOnPropertyRector`.

use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{Node, NodeKind};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{named, text};
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoArrayKeyExistsOnObject;

impl Rule for NoArrayKeyExistsOnObject {
    fn id(&self) -> &'static str {
        "lint/php-7.4/no-array-key-exists-on-object"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70400)
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
        if !named(call, "array_key_exists") {
            return;
        }
        let args = call.argument_list.arguments.as_slice();
        let Some(second) = args.get(1) else {
            return;
        };
        // `$this->getType($node->args[1]->value) instanceof ObjectType`.
        let is_object = facts(ctx)
            .full_type(node_span(value(second)))
            .is_some_and(|t| matches!(t.types.as_ref(), [TAtomic::Object(TObject::Named(_))]));
        if !is_object {
            return;
        }
        let reversed: Vec<&str> = args.iter().rev().map(|a| text(ctx, a)).collect();
        let list = Span::new(
            node_span(&args[0]).start,
            node_span(&args[args.len() - 1]).end,
        );
        out.push(Report::fixed(
            node_span(call),
            "`array_key_exists()` on an object is removed in PHP 8.0; use `property_exists()`.",
            Safety::Unsafe,
            vec![
                Edit::new(node_span(call.function), "property_exists"),
                Edit::new(list, reversed.join(", ")),
            ],
        ));
    }
}
