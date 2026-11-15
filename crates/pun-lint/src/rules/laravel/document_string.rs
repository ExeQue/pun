//! Heredoc and nowdoc values as php-parser reads them (`parseDocString`), for the rules that compare them.

use mago_allocator::{Arena, LocalArena};
use mago_syntax::cst::*;
use mago_syntax_core::utils::parse_literal_string_in;

/// A part of an interpolated string as php-parser holds it: the decoded text of an `InterpolatedStringPart`, or an
/// expression.
enum Part {
    Literal(Vec<u8>),
    Expression,
}

/// The value of a heredoc or nowdoc php-parser reads as a `String_` (no interpolated part), as [`document_parts`]
/// decodes it.
pub(crate) fn document_value(doc: &DocumentString<'_>) -> Option<Vec<u8>> {
    let mut value = Vec::new();
    for part in document_parts(doc)? {
        match part {
            Part::Literal(v) => value.extend(v),
            Part::Expression => return None,
        }
    }
    Some(value)
}

/// php-parser's `parseDocString`: the parts of a heredoc or nowdoc, each literal part without the closing label's
/// indentation at the start of its lines (`stripIndentation`), then decoded (a heredoc's escapes); a literal part left
/// empty goes. The newline before the closing label is not in Mago's last part. Mixed tabs and spaces count as no
/// indentation, as php-parser recovers from that error.
fn document_parts(doc: &DocumentString<'_>) -> Option<Vec<Part>> {
    let indent = match doc.indentation {
        DocumentIndentation::Whitespace(n) | DocumentIndentation::Tab(n) => n,
        DocumentIndentation::None | DocumentIndentation::Mixed(..) => 0,
    };
    let mut out = Vec::new();
    for (i, part) in doc.parts.iter().enumerate() {
        let literal = match part {
            StringPart::Literal(l) => l,
            StringPart::Expression(_) | StringPart::BracedExpression(_) => {
                out.push(Part::Expression);
                continue;
            }
        };
        let raw = strip_indentation(literal.raw, indent, i == 0);
        let value = match doc.kind {
            DocumentKind::Nowdoc => raw,
            DocumentKind::Heredoc => {
                // Mago decodes a double-quoted string's escapes; a heredoc keeps `\"` as it is, so that
                // backslash is doubled first.
                let mut kept = Vec::with_capacity(raw.len());
                let mut backslashes = 0;
                for &b in &raw {
                    if b == b'"' && backslashes % 2 == 1 {
                        kept.push(b'\\');
                    }
                    backslashes = if b == b'\\' { backslashes + 1 } else { 0 };
                    kept.push(b);
                }
                let arena = LocalArena::new();
                let kept = arena.alloc_slice_copy(&kept);
                parse_literal_string_in(&arena, kept, Some(b'"'), false)?.to_vec()
            }
        };
        if !value.is_empty() {
            out.push(Part::Literal(value));
        }
    }
    Some(out)
}

/// `stripIndentation`: up to `indent` blanks taken from the start of each line (and of the text, when it starts a
/// line).
fn strip_indentation(raw: &[u8], indent: usize, at_line_start: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut line_start = at_line_start;
    let mut i = 0;
    while i < raw.len() {
        if line_start {
            let blanks = raw[i..]
                .iter()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            i += blanks.min(indent);
            line_start = false;
            continue;
        }
        out.push(raw[i]);
        line_start = raw[i] == b'\n';
        i += 1;
    }
    out
}
