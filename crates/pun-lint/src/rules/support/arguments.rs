use mago_syntax::cst::{Argument, ArgumentList, Expression};
use pun_problem::{Edit, Span};

use crate::rules::support::statement_edits::line_indent;
use crate::syntax::node_span;

pub(crate) fn value<'a>(arg: &'a Argument<'a>) -> &'a Expression<'a> {
    match arg {
        Argument::Positional(a) => a.value,
        Argument::Named(a) => a.value,
    }
}

pub(crate) fn unpacked(arg: &Argument<'_>) -> bool {
    matches!(arg, Argument::Positional(a) if a.ellipsis.is_some())
}

pub(crate) fn remove_argument(list: &ArgumentList<'_>, index: usize) -> Edit {
    let items = list.arguments.as_slice();
    let span = node_span(&items[index]);
    if index > 0 {
        Edit::new(Span::new(node_span(&items[index - 1]).end, span.end), "")
    } else {
        Edit::new(Span::new(span.start, node_span(&items[1]).start), "")
    }
}

pub(crate) fn append_argument(source: &str, list: &ArgumentList<'_>, code: &str) -> Edit {
    let spans: Vec<Span> = list.arguments.iter().map(node_span).collect();
    append_item(
        source,
        &spans,
        list.left_parenthesis.end.offset as usize,
        code,
    )
}

pub(crate) fn append_item(source: &str, spans: &[Span], open: usize, code: &str) -> Edit {
    let Some(last) = spans.last() else {
        return Edit::new(Span::new(open, open), code.to_string());
    };
    let multiline = spans.len() >= 2
        && spans
            .windows(2)
            .all(|w| source[w[0].end..w[1].end].contains('\n'));
    if multiline {
        let indent = line_indent(source, last.start);
        return Edit::new(Span::new(last.end, last.end), format!(",\n{indent}{code}"));
    }
    Edit::new(Span::new(last.end, last.end), format!(", {code}"))
}
