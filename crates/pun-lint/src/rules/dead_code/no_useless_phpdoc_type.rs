use crate::problem::{Fix, Safety};
use mago_syntax::cst::{Node, NodeKind};
use pun_phpstan::phpdoc::Node as DocNode;

use crate::rules::phpdoc::no_useless_param_tag::function_like;
use crate::rules::support::docblocks::Doc;
use crate::rules::types::compare::classes::with_classes;
use crate::rules::types::compare::is_super;
use crate::rules::types::compare::mapper::TypeScope;
use crate::rules::types::compare::tags::{Kind, first_named, removed_by_type};
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoUselessPhpdocType;

impl Rule for NoUselessPhpdocType {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-useless-phpdoc-type"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80000)
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
        let Some((_, Some(return_type))) = function_like(&node) else {
            return;
        };
        let start = node_span(&node).start;
        let Some(doc) = Doc::last_of(ctx, ancestors, start) else {
            return;
        };
        let Some((_, tag)) = first_named(&doc, "@return") else {
            return;
        };
        let DocNode::Union(members) = &tag.ty else {
            return;
        };
        if tag.has_description {
            return;
        }
        if members
            .iter()
            .any(|m| matches!(m, DocNode::This) || matches!(m, DocNode::Ident(n) if n == "static"))
        {
            return;
        }
        let facts = facts(ctx);
        let mut chain = ancestors.to_vec();
        chain.push(node);
        let scope = TypeScope::new(&chain, facts.documented());
        let useless = with_classes(facts.documented(), || {
            let native = scope.map_hint(&return_type.hint);
            if !native.to_super().is_array().no() {
                return false;
            }
            is_super(&scope.map_doc(&tag.ty), &native).yes()
        });
        if !useless {
            return;
        }
        let removed = removed_by_type(&doc, Kind::Return, "");
        out.push(Report {
            span: doc.span,
            message: "Remove the `@return` union, which takes in the whole native return type."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![doc.remove_last(ctx.source, &removed)],
            }),
        });
    }
}
