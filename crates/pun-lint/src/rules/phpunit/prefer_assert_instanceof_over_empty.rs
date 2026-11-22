use crate::problem::Safety;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{Node, NodeKind};

use super::{AnyCall, NewArg, expr_text, is_in_test_class, old_args, rewrite_args};
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const NAMES: &[&str] = &["assertNotEmpty", "assertNotNull"];

pub struct PreferAssertInstanceofOverEmpty;

impl Rule for PreferAssertInstanceofOverEmpty {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-assert-instanceof-over-empty"
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
        if !call.is_name(NAMES) || !is_in_test_class(ctx, ancestors, None) {
            return;
        }
        let args = call.args();
        let Some(first) = args.first().copied() else {
            return;
        };
        let value = first.value();
        let facts = facts(ctx);
        let Some(ty) = facts.full_type(node_span(value)) else {
            return;
        };
        if ty.types.len() < 2 {
            return;
        }
        let rest: Vec<&TAtomic> = ty
            .types
            .iter()
            .filter(|a| !matches!(a, TAtomic::Null))
            .collect();
        let class = match rest[..] {
            [TAtomic::Object(TObject::Named(named))]
                if named.intersection_types.is_none()
                    && !named.is_this
                    && !named.is_static
                    && !named.name.as_bytes().starts_with(b"{anonymous-class:") =>
            {
                named.name.to_string()
            }
            [TAtomic::Object(TObject::Enum(e))] => e.name.to_string(),
            _ => return,
        };
        let name = "assertInstanceOf";
        let mut new = vec![
            NewArg::New(format!("\\{class}::class")),
            NewArg::New(expr_text(ctx, value).to_string()),
        ];
        new.extend(old_args(&call, 1));
        out.push(Report::fixed(
            call.span,
            format!("Use `{name}()` on the nullable object."),
            Safety::Safe,
            rewrite_args(ctx, &call, Some(name), &new),
        ));
    }
}
