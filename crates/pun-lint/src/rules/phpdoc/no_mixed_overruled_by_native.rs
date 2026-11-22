use crate::problem::{Fix, Safety};
use mago_syntax::cst::{Node, NodeKind};

use crate::rules::support::docblocks::Doc;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoMixedOverruledByNative;

impl Rule for NoMixedOverruledByNative {
    fn id(&self) -> &'static str {
        "lint/phpdoc/no-mixed-overruled-by-native"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method, NodeKind::Function])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let mut list = None;
        let mut has_return_type = false;
        node.visit_children(|child| match child {
            Node::FunctionLikeParameterList(l) => list = Some(l),
            Node::FunctionLikeReturnTypeHint(_) => has_return_type = true,
            _ => {}
        });
        let Some(list) = list else { return };
        let start = node_span(&node).start;
        let Some(doc) = Doc::of(ctx, ancestors, start) else {
            return;
        };
        let typed: Vec<&[u8]> = list
            .parameters
            .iter()
            .filter(|p| p.hint.is_some())
            .map(|p| &p.variable.name[1..])
            .collect();
        let tags: Vec<Vec<&str>> = (0..doc.len()).map(|i| doc.words(i)).collect();
        let mut removed = vec![false; tags.len()];
        for (i, words) in tags.iter().enumerate() {
            if let ["@param", "mixed", name] = words[..]
                && let Some(name) = name.trim_start_matches(['&', '.']).strip_prefix('$')
            {
                removed[i] = typed.contains(&name.as_bytes());
            }
        }
        let returns: Vec<usize> = (0..tags.len())
            .filter(|&i| tags[i].first() == Some(&"@return") && tags[i].len() > 1)
            .collect();
        if has_return_type
            && let Some(&first) = returns.first()
            && tags[first][1..] == ["mixed"]
        {
            for i in returns {
                removed[i] = true;
            }
        }
        if !removed.contains(&true) {
            return;
        }
        out.push(Report {
            span: doc.span,
            message: "Remove the `mixed` tag that the native type overrules.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![doc.remove(&removed, start)],
            }),
        });
    }
}
