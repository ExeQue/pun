use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{Node, NodeKind};

use super::print::moved_value;
use super::replace;
use crate::rules::support::expressions::named;
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoUselessValueCall;

impl Rule for NoUselessValueCall {
    fn id(&self) -> &'static str {
        "lint/laravel/no-useless-value-call"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(c) = node else { return };
        if !named(c, "value") {
            return;
        }
        let [argument] = c.argument_list.arguments.as_slice() else {
            return;
        };
        if argument.is_unpacked() {
            return;
        }
        let value = unparenthesize(argument.value());
        let facts = facts(ctx);
        if facts
            .full_type(node_span(value))
            .is_none_or(|t| may_be_closure(&facts, t))
        {
            return;
        }
        let span = node_span(c);
        out.push(replace(
            span,
            "Remove the `value()` c: its argument is no closure.".to_string(),
            moved_value(ctx, span, ancestors, value),
        ));
    }
}

fn may_be_closure(facts: &Facts, t: &TUnion) -> bool {
    t.types.iter().any(|a| atomic_may_be_closure(facts, a))
}

fn atomic_may_be_closure(facts: &Facts, atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::Scalar(_)
        | TAtomic::Null
        | TAtomic::Void
        | TAtomic::Array(_)
        | TAtomic::Iterable(_)
        | TAtomic::Resource(_) => false,
        TAtomic::Object(TObject::Named(n)) => {
            n.name.as_bytes().eq_ignore_ascii_case(b"Closure")
                || facts.codebase.get_class_like(n.name.as_bytes()).is_none()
        }
        TAtomic::Object(TObject::Enum(_)) => false,
        _ => true,
    }
}
