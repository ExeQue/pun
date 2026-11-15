use mago_syntax::cst::{ArgumentList, BinaryOperator, Call, Expression, Node, NodeKind};
use pun_problem::{Edit, Safety, Span};

use super::{class_name, is_a, is_name, member_name};
use crate::rules::support::arguments::append_argument;
use crate::rules::support::expressions::{named, text};
use crate::rules::support::values::{Constant, constant};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferDefaultArgument;

const METHODS: [(&str, Option<&str>, usize); 4] = [
    ("config", None, 1),
    ("env", None, 1),
    ("data_get", None, 2),
    ("get", Some("Illuminate\\Support\\Env"), 1),
];

impl Rule for PreferDefaultArgument {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-default-argument"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Binary])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Binary(coalesce) = node else {
            return;
        };
        if !matches!(coalesce.operator, BinaryOperator::NullCoalesce(_)) {
            return;
        }
        if matches!(unparenthesize(coalesce.rhs), Expression::Throw(_)) {
            return;
        }
        let (name, class, list): (&[u8], Option<String>, &ArgumentList<'_>) =
            match unparenthesize(coalesce.lhs) {
                Expression::Call(Call::Function(f)) => {
                    let name = METHODS
                        .iter()
                        .find(|(m, ..)| named(f, m))
                        .map_or(&b""[..], |(m, ..)| m.as_bytes());
                    (name, None, &f.argument_list)
                }
                Expression::Call(Call::Method(m)) => {
                    let Some(name) = member_name(&m.method) else {
                        return;
                    };
                    (name, None, &m.argument_list)
                }
                Expression::Call(Call::StaticMethod(s)) => {
                    let Some(name) = member_name(&s.method) else {
                        return;
                    };
                    (name, class_name(s.class, ancestors), &s.argument_list)
                }
                _ => return,
            };
        let found = METHODS.iter().find(|(method, on, _)| {
            is_name(name, method)
                && match on {
                    Some(on) => class.as_deref().is_some_and(|c| is_a(ctx, c, &[on])),
                    None => true,
                }
        });
        let Some(&(method, _, position)) = found else {
            return;
        };
        if list.arguments.len() != position {
            return;
        }
        let call = node_span(unparenthesize(coalesce.lhs));
        let whole = node_span(coalesce);
        let mut edits = vec![Edit::new(Span::new(call.end, whole.end), "")];
        if whole.start < call.start {
            edits.push(Edit::new(Span::new(whole.start, call.start), ""));
        }
        if constant(ancestors, coalesce.rhs, node_span(coalesce).start) != Some(Constant::Null) {
            edits.push(append_argument(ctx.source, list, text(ctx, coalesce.rhs)));
        }
        out.push(Report::fixed(
            node_span(coalesce),
            format!("Pass the default to `{method}()` instead of using `??`."),
            Safety::Unsafe,
            edits,
        ));
    }
}
