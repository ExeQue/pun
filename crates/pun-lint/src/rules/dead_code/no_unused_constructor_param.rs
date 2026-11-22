use crate::problem::{Fix, Safety};
use mago_span::HasSpan;
use mago_syntax::cst::*;

use crate::rules::support::class_members::qualified;
use crate::rules::support::class_members::{member_from, param_used};
use crate::rules::support::docblocks::doc_comment;
use crate::rules::support::names::has_attribute;
use crate::rules::support::rewrites::remove_params;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoUnusedConstructorParam;

impl Rule for NoUnusedConstructorParam {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-unused-constructor-param"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let facts = facts(ctx);
        let (members, left_brace, attributes, name) = match node {
            Node::Class(class) => {
                let name = qualified(ancestors, class.name.value);
                (
                    &class.members,
                    class.left_brace,
                    &class.attribute_lists,
                    name,
                )
            }
            Node::AnonymousClass(class) => (
                &class.members,
                class.left_brace,
                &class.attribute_lists,
                facts.anonymous_class_name(class.span()),
            ),
            _ => return,
        };
        let Some((index, method)) = members.iter().enumerate().find_map(|(i, m)| match m {
            ClassLikeMember::Method(m) if m.name.value.eq_ignore_ascii_case(b"__construct") => {
                Some((i, m))
            }
            _ => None,
        }) else {
            return;
        };
        let codebase = &facts.codebase;
        let Some(class) = codebase.get_class_like(name.as_bytes()).filter(|_| {
            !method.parameter_list.parameters.is_empty()
                && !matches!(method.body, MethodBody::Abstract(_))
        }) else {
            return;
        };
        if class.all_parent_interfaces.iter().any(|i| {
            codebase.get_class_like(i.as_bytes()).is_some_and(|i| {
                i.methods
                    .iter()
                    .any(|m| m.as_bytes().eq_ignore_ascii_case(b"__construct"))
            })
        }) {
            return;
        }
        let from = member_from(members, left_brace, index);
        let mut path = ancestors.to_vec();
        path.push(node);
        if has_attribute(&path, attributes, "Attribute")
            && doc_comment(ctx.source, from, node_span(method).start).is_some()
        {
            return;
        }
        let removed: Vec<bool> = method
            .parameter_list
            .parameters
            .iter()
            .map(|p| !param_used(method, p))
            .collect();
        let Some(first) = removed.iter().position(|r| *r) else {
            return;
        };
        if !removed[first..].iter().all(|r| *r) {
            return;
        }
        out.push(Report {
            span: node_span(method),
            message: "Remove the unused parameters of this constructor.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: remove_params(ctx, method, from, &removed),
            }),
        });
    }
}
