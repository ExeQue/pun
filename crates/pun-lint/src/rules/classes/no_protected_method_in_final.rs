use crate::problem::{Fix, Safety};
use mago_syntax::cst::{ClassLikeMember, Node, NodeKind};

use crate::rules::support::class_members::is_magic;
use crate::rules::support::visibility::{final_class, is_protected, make_private};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoProtectedMethodInFinal;

impl Rule for NoProtectedMethodInFinal {
    fn id(&self) -> &'static str {
        "lint/classes/no-protected-method-in-final"
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
            let ClassLikeMember::Method(method) = member else {
                continue;
            };
            let name = method.name.value;
            if !is_protected(&method.modifiers) || is_magic(name) {
                continue;
            }
            out.push(Report {
                span: node_span(&method.name),
                message: "Make the protected method of a final class private.".to_string(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Unsafe,
                    edits: vec![make_private(ctx.source, &method.modifiers)],
                }),
            });
        }
    }
}
