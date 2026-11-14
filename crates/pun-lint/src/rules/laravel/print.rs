use mago_syntax::cst::{ArgumentList, Expression, FunctionLikeParameter, Node};
use pun_problem::{Edit, Span};

use super::parent;
use crate::Context;
use crate::rules::lint_b_helpers::dereferenceable;
use crate::rules::rector_code_quality::moved;
use crate::rules::rector_conditions::{indentation, reprint};
use crate::syntax::{node_span, unparenthesize};

pub(crate) fn set_param_type(param: &FunctionLikeParameter<'_>, hint: &str) -> Edit {
    match &param.hint {
        Some(old) => Edit::new(node_span(old), hint),
        None => {
            let at = (param.ampersand)
                .or(param.ellipsis)
                .map_or(node_span(&param.variable).start, |s| {
                    s.start.offset as usize
                });
            Edit::new(Span::new(at, at), format!("{hint} "))
        }
    }
}

pub(crate) fn moved_with_edit(source: &str, span: Span, edit: &Edit, at: usize) -> String {
    let tokens = crate::rules::php_parser::lex(source);
    let adjust = indentation(source, at) as isize - indentation(source, span.start) as isize;
    format!(
        "{}{}{}",
        reprint(
            source,
            &tokens,
            Span::new(span.start, edit.span.start),
            adjust
        ),
        edit.text,
        reprint(source, &tokens, Span::new(edit.span.end, span.end), adjust)
    )
}

pub(crate) fn moved_value(
    ctx: &Context<'_>,
    span: Span,
    ancestors: &[Node<'_, '_>],
    value: &Expression<'_>,
) -> String {
    let code = moved(ctx, span, ancestors, value);
    let dereferenced = parent(span, ancestors).is_some_and(|(_, p)| {
        node_span(p).start == span.start
            && matches!(
                p,
                Node::ArrayAccess(_)
                    | Node::PropertyAccess(_)
                    | Node::NullSafePropertyAccess(_)
                    | Node::MethodCall(_)
                    | Node::NullSafeMethodCall(_)
                    | Node::StaticMethodCall(_)
                    | Node::StaticPropertyAccess(_)
                    | Node::ClassConstantAccess(_)
            )
    });
    if dereferenced && matches!(unparenthesize(value), Expression::Instantiation(_)) {
        format!("({code})")
    } else {
        code
    }
}

pub(crate) fn reprinted_call(
    source: &str,
    receiver: &Expression<'_>,
    call_end: usize,
    tail: &str,
) -> Vec<Edit> {
    let outer = node_span(receiver);
    let inner = node_span(unparenthesize(receiver));
    let needs = dereferenceable(source, receiver).starts_with('(');
    let (open, close) = if needs { ("(", ")") } else { ("", "") };
    let mut edits = Vec::new();
    if source[outer.start..inner.start] != *open {
        edits.push(Edit::new(Span::new(outer.start, inner.start), open));
    }
    edits.push(if source[inner.end..outer.end] == *close {
        Edit::new(Span::new(outer.end, call_end), tail)
    } else {
        Edit::new(Span::new(inner.end, call_end), format!("{close}{tail}"))
    });
    edits
}

pub(crate) fn kept_arguments(source: &str, list: &ArgumentList<'_>, at: usize) -> Vec<String> {
    kept_nodes(source, list.arguments.iter().map(node_span), at)
}

pub(crate) fn kept_nodes(
    source: &str,
    spans: impl Iterator<Item = Span>,
    at: usize,
) -> Vec<String> {
    let tokens = crate::rules::php_parser::lex(source);
    let level = indentation(source, at) as isize;
    spans
        .map(|span| {
            reprint(
                source,
                &tokens,
                span,
                level - indentation(source, span.start) as isize,
            )
        })
        .collect()
}
