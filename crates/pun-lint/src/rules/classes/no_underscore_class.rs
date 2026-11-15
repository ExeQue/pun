use mago_syntax::cst::{Node, NodeKind};

use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoUnderscoreClass;

impl Rule for NoUnderscoreClass {
    fn id(&self) -> &'static str {
        "lint/classes/no-underscore-class"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80400)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::Class,
            NodeKind::Interface,
            NodeKind::Trait,
            NodeKind::Enum,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (name, kind) = match node {
            Node::Class(c) => (&c.name, "a class"),
            Node::Interface(i) => (&i.name, "an interface"),
            Node::Trait(t) => (&t.name, "a trait"),
            _ => {
                let Node::Enum(e) = node else { return };
                (&e.name, "an enum")
            }
        };
        if name.value != b"_" {
            return;
        }
        out.push(Report {
            span: node_span(name),
            message: format!("Using `_` as {kind} name is deprecated."),
            help: None,
            fix: None,
        });
    }
}
