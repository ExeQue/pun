use mago_syntax::cst::{ClassLikeMember, Node, NodeKind};
use pun_problem::{Fix, Safety};

use crate::rules::support::visibility::{final_class, is_protected, make_private};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoProtectedConstantInFinal;

impl Rule for NoProtectedConstantInFinal {
    fn id(&self) -> &'static str {
        "lint/classes/no-protected-constant-in-final"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(class) = final_class(node) else {
            return;
        };
        for member in &class.members {
            let ClassLikeMember::Constant(constant) = member else {
                continue;
            };
            if constant.items.len() != 1 || !is_protected(&constant.modifiers) {
                continue;
            }
            out.push(Report {
                span: node_span(constant),
                message: "Make the protected constant of a final class private.".to_string(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Unsafe,
                    edits: vec![make_private(ctx.source, &constant.modifiers)],
                }),
            });
        }
    }
}
