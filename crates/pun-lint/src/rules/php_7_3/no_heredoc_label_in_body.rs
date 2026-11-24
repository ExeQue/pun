//! Rector 2.8.4's `SensitiveHereNowDocRector`.

use mago_syntax::cst::{DocumentKind, Node, NodeKind, StringPart};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::statement_edits::line_indent;
use crate::syntax::{node_span, span};
use crate::{Context, Report, Rule};

/// `WRAP_SUFFIX`.
const WRAP_SUFFIX: &str = "_WRAP";

pub struct NoHeredocLabelInBody;

impl Rule for NoHeredocLabelInBody {
    fn id(&self) -> &'static str {
        "lint/php-7.3/no-heredoc-label-in-body"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70300)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::DocumentString])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::DocumentString(doc) = node else {
            return;
        };
        // A heredoc with interpolation is not a plain string.
        if doc
            .parts
            .iter()
            .any(|p| !matches!(p, StringPart::Literal(_)))
        {
            return;
        }
        let source = ctx.source;
        let label = String::from_utf8_lossy(doc.label).into_owned();
        let open = span(doc.open);
        let close = span(doc.close);
        let Some(at) = source[close.start..close.end].find(label.as_str()) else {
            return;
        };
        let label_start = close.start + at;
        let body = &source[open.end..label_start];
        let body = body
            .strip_prefix("\r\n")
            .or_else(|| body.strip_prefix('\n'))
            .unwrap_or(body);
        let Some(newline) = body.rfind('\n') else {
            return;
        };
        let (content, closing_indent) = (&body[..newline], &body[newline + 1..]);
        let content = content.strip_suffix('\r').unwrap_or(content);
        // the doc label is not in the string → ok
        if !content.contains(label.as_str()) {
            return;
        }
        let new_label = uniquate_doc_label(content, &label);
        let indent = line_indent(source, open.start);
        let lines: Vec<String> = content
            .split('\n')
            .map(|line| {
                format!(
                    "{indent}{}",
                    line.strip_prefix(closing_indent).unwrap_or(line)
                )
            })
            .collect();
        let quoted = match doc.kind {
            DocumentKind::Nowdoc => format!("'{new_label}'"),
            DocumentKind::Heredoc => new_label.clone(),
        };
        let code = format!("<<<{quoted}\n{}\n{indent}{new_label}", lines.join("\n"));
        let replaced = Span::new(open.start, label_start + label.len());
        out.push(Report::fixed(
            node_span(doc),
            format!("The body holds the closing label `{label}`, which ends the string early since PHP 7.3; rename it."),
            Safety::Safe,
            vec![Edit::new(replaced, code)],
        ));
    }
}

fn uniquate_doc_label(value: &str, label: &str) -> String {
    let base = format!("{label}{WRAP_SUFFIX}");
    let mut label = base.clone();
    let mut i = 0;
    while value.contains(label.as_str()) {
        i += 1;
        label = format!("{base}_{i}");
    }
    label
}
