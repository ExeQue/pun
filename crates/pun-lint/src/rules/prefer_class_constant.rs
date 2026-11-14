use mago_syntax::cst::{Literal, Node, NodeKind};
use pun_problem::{Edit, Fix, Safety};

use super::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const ZEND_SERVER: [&str; 4] = ["ZendAPI_Queue", "ZendAPI_Job", "java", "JavaException"];

pub struct PreferClassConstant;

impl Rule for PreferClassConstant {
    fn id(&self) -> &'static str {
        "lint/style/prefer-class-constant"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Literal])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Literal(Literal::String(string)) = node else {
            return;
        };
        let span = node_span(string);
        let raw = &ctx.source[span.start..span.end];
        let Some(inner) = raw.get(1..raw.len().saturating_sub(1)) else {
            return;
        };
        let name = inner.trim_start_matches('\\').replace("\\\\", "\\");
        if name.is_empty() {
            return;
        }
        let declared = facts(ctx)
            .codebase
            .get_class_like(name.as_bytes())
            .is_some_and(|c| c.original_name.as_bytes() == name.as_bytes());
        if !declared || ZEND_SERVER.iter().any(|z| z.eq_ignore_ascii_case(&name)) {
            return;
        }
        out.push(Report {
            span,
            message: "Use `::class` instead of a class name in a string.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Unsafe,
                edits: vec![Edit::new(span, format!("\\{name}::class"))],
            }),
        });
    }
}
