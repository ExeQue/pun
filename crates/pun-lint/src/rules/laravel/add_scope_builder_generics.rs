use mago_syntax::cst::{Hint, MethodBody, Node, NodeKind};

use super::is_a;
use super::names::resolve_class_name;
use super::prefer_scope_attribute::{ELOQUENT_BUILDER, SCOPE, is_named_scope};
use super::property_attribute::MODEL;
use crate::rules::support::names::has_attribute;
use crate::rules::support::phpdoc_writer::DocTarget;
use crate::rules::support::phpdoc_writer::type_node::TypeNode;
use crate::rules::types::class_scope;
use crate::rules::types::docblock::docs_report;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct AddScopeBuilderGenerics;

impl Rule for AddScopeBuilderGenerics {
    fn id(&self) -> &'static str {
        "lint/laravel/add-scope-builder-generics"
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
        if matches!(method.body, MethodBody::Abstract(_)) {
            return;
        }
        // a named class extending the model; an anonymous class, a trait and the model itself are skipped
        let Some(Node::Class(_)) = ancestors.iter().rev().find(|a| {
            matches!(
                a,
                Node::Class(_)
                    | Node::AnonymousClass(_)
                    | Node::Interface(_)
                    | Node::Trait(_)
                    | Node::Enum(_)
            )
        }) else {
            return;
        };
        let Some(scope) = class_scope(ancestors) else {
            return;
        };
        let Some(own) = scope.name.as_deref() else {
            return;
        };
        if own.eq_ignore_ascii_case(MODEL) || !is_a(ctx, own, &[MODEL]) {
            return;
        }
        if !is_named_scope(ctx, ancestors, method)
            && !has_attribute(ancestors, &method.attribute_lists, SCOPE)
        {
            return;
        }
        let Some(first) = method.parameter_list.parameters.first() else {
            return;
        };
        let param = String::from_utf8_lossy(&first.variable.name[1..]).into_owned();
        let expected = if scope.is_final { "self" } else { "static" };
        let generic = || TypeNode::Generic {
            base: format!("\\{ELOQUENT_BUILDER}"),
            args: vec![TypeNode::ident(expected)],
        };
        let correct = |t: &TypeNode| matches!(t, TypeNode::Generic { args, .. } if matches!(&args[..], [TypeNode::Identifier(n)] if n == expected));
        let target = DocTarget::of(ctx, node_span(method).start);
        let mut info = target.info(ctx.source);
        let mut changed = false;
        if !info.param_tag(&param).is_some_and(|t| correct(&t.ty)) {
            info.set_param_type(&param, false, generic());
            changed = true;
        }
        let returns_builder = matches!(
            method.return_type_hint.as_ref().map(|h| &h.hint),
            Some(Hint::Identifier(id)) if is_a(ctx, &resolve_class_name(ancestors, id), &[ELOQUENT_BUILDER])
        );
        if returns_builder && !info.return_tag().is_some_and(|t| correct(&t.ty)) {
            info.set_return_type(generic());
            changed = true;
        }
        if !changed {
            return;
        }
        out.extend(docs_report(
            node_span(&method.name),
            "Add the model's generic to the scope's `Builder` types.",
            ctx.source,
            &[(target, info)],
        ));
    }
}
