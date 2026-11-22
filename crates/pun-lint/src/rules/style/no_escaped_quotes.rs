use crate::problem::{Edit, Fix, Safety};
use mago_syntax::cst::{LiteralStringKind, Node, NodeKind};

use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoEscapedQuotes;

impl Rule for NoEscapedQuotes {
    fn id(&self) -> &'static str {
        "lint/style/no-escaped-quotes"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::LiteralString])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::LiteralString(s) = node else { return };
        let string = s;
        let Some(value) = string.value else { return };
        let count = |q: u8| value.iter().filter(|&&b| b == q).count();
        let (double, single) = (count(b'"'), count(b'\''));
        if double == 0 && single == 0 {
            return;
        }
        if value.iter().any(|&b| b < 0x20)
            || (value.iter().any(|&b| b >= 0x80) && std::str::from_utf8(value).is_err())
        {
            return;
        }
        if value.iter().any(|&b| b == b'\\' || b == b'$') {
            return;
        }
        let new = match string.kind {
            LiteralStringKind::SingleQuoted if double == 0 && single > 0 => b'"',
            LiteralStringKind::DoubleQuoted if single == 0 && double > 0 => {
                let raw = string.raw;
                let inner = String::from_utf8_lossy(&raw[1..raw.len() - 1]).replace("\\\"", "\"");
                if inner.as_bytes() != value {
                    return;
                }
                b'\''
            }
            _ => return,
        } as char;
        let value = String::from_utf8_lossy(value);
        out.push(Report {
            span: node_span(string),
            message: "Use the other quotes so the string needs no escaped quotes.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(node_span(string), format!("{new}{value}{new}"))],
            }),
        });
    }
}
