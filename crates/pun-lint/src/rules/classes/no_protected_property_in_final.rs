use crate::problem::{Fix, Safety};
use mago_syntax::cst::{ClassLikeMember, Node, NodeKind, Property};

use crate::rules::support::visibility::{final_class, is_protected, make_private};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoProtectedPropertyInFinal;

impl Rule for NoProtectedPropertyInFinal {
    fn id(&self) -> &'static str {
        "lint/classes/no-protected-property-in-final"
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
        let mut found = Vec::new();
        for member in &class.members {
            let ClassLikeMember::Property(property) = member else {
                continue;
            };
            let (modifiers, single) = match property {
                Property::Plain(p) => (&p.modifiers, p.items.len() == 1),
                Property::Hooked(p) => (&p.modifiers, true),
            };
            if single && is_protected(modifiers) {
                found.push((node_span(property), modifiers));
            }
        }
        let construct = class.members.iter().find_map(|m| match m {
            ClassLikeMember::Method(m) if m.name.value.eq_ignore_ascii_case(b"__construct") => {
                Some(m)
            }
            _ => None,
        });
        if let Some(construct) = construct {
            for param in &construct.parameter_list.parameters {
                if is_protected(&param.modifiers) {
                    found.push((node_span(param), &param.modifiers));
                }
            }
        }
        for (span, modifiers) in found {
            out.push(Report {
                span,
                message: "Make the protected property of a final class private.".to_string(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Unsafe,
                    edits: vec![make_private(ctx.source, modifiers)],
                }),
            });
        }
    }
}
