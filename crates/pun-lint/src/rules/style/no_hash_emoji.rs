use mago_syntax::cst::{Node, NodeKind, TriviaKind};
use pun_problem::{Edit, Fix, Safety, Span};

use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoHashEmoji;

const EMOJI: &str = "#\u{fe0f}\u{20e3}";

impl Rule for NoHashEmoji {
    fn id(&self) -> &'static str {
        "lint/style/no-hash-emoji"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Program])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Program(program) = node else { return };
        for trivia in program.trivia.nodes {
            if trivia.kind != TriviaKind::HashComment || !trivia.value.starts_with(EMOJI.as_bytes())
            {
                continue;
            }
            let span = node_span(trivia);
            out.push(Report {
                span,
                message: "Emoji-based hash (`#️⃣`) used instead of ASCII `#`.".to_string(),
                help: Some("Replace `#️⃣` with `#`.".to_string()),
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits: vec![Edit::new(
                        Span::new(span.start, span.start + EMOJI.len()),
                        "#",
                    )],
                }),
            });
        }
    }
}
