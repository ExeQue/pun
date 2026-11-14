use mago_syntax::cst::{Node, NodeKind};
use pun_problem::{Fix, Safety};

use super::lint_b_doc::Doc;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoVoidOnMagicMethod;

impl Rule for NoVoidOnMagicMethod {
    fn id(&self) -> &'static str {
        "lint/phpdoc/no-void-on-magic-method"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Method(method) = node else { return };
        if method.return_type_hint.is_some()
            || !["__construct", "__destruct", "__clone"]
                .iter()
                .any(|n| method.name.value.eq_ignore_ascii_case(n.as_bytes()))
        {
            return;
        }
        let start = node_span(method).start;
        let Some(doc) = Doc::of(ctx, ancestors, start) else {
            return;
        };
        let words: Vec<Vec<&str>> = (0..doc.len()).map(|i| doc.words(i)).collect();
        let is_return = |w: &Vec<&str>| w.first() == Some(&"@return");
        if words.iter().any(|w| is_return(w) && w.len() == 1) {
            return;
        }
        if words.iter().find(|w| is_return(w)) != Some(&vec!["@return", "void"]) {
            return;
        }
        let removed: Vec<bool> = words.iter().map(is_return).collect();
        let edit = doc.remove(&removed, start);
        out.push(Report {
            span: doc.span,
            message: "Remove the useless `@return void` from this magic method.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![edit],
            }),
        });
    }
}
