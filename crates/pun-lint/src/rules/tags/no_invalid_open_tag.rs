use crate::problem::{Edit, Fix, Safety, Span};
use mago_span::HasSpan;
use mago_syntax::cst::{Node, NodeKind};

use crate::{Context, Report, Rule};

const INVALID_TAGS: &[&str] = &["<php?", "<ph?p", "<p?hp", "<ph?", "<p?"];

pub struct NoInvalidOpenTag;

impl Rule for NoInvalidOpenTag {
    fn id(&self) -> &'static str {
        "lint/tags/no-invalid-open-tag"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Inline])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Inline(inline) = node else { return };
        let content = inline.value;
        let start = content
            .iter()
            .position(|b| !b.is_ascii_whitespace())
            .unwrap_or(content.len());
        let trimmed = &content[start..];
        let Some(tag) = INVALID_TAGS.iter().find(|tag| {
            trimmed.len() >= tag.len() && trimmed[..tag.len()].eq_ignore_ascii_case(tag.as_bytes())
        }) else {
            return;
        };
        let from = inline.span().start.offset as usize + start;
        let span = Span::new(from, from + tag.len());
        out.push(Report {
            span,
            message: format!(
                "Misspelled PHP opening tag `{}`.",
                String::from_utf8_lossy(&trimmed[..tag.len()])
            ),
            help: Some("Replace this with the correct `<?php` opening tag.".to_string()),
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, "<?php")],
            }),
        });
    }
}
