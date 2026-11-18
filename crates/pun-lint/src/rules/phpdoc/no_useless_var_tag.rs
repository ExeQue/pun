use mago_syntax::cst::{ClassLikeMember, Expression, Hint, Node, NodeKind, Property, Statement};
use pun_problem::{Fix, Safety};

use crate::rules::support::docblocks::Doc;
use crate::rules::types::compare::classes::with_classes;
use crate::rules::types::compare::mapper::{TypeScope, expr_type};
use crate::rules::types::compare::tags::{Kind, TypeTag, first_named, is_allowed, removed_by_type};
use crate::rules::types::compare::{
    RType, are_native_and_doc_types_equal, are_types_equal, equals, is_legal_without_template,
    is_super, remove_null,
};
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoUselessVarTag;

impl Rule for NoUselessVarTag {
    fn id(&self) -> &'static str {
        "lint/phpdoc/no-useless-var"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ClassLikeMember, NodeKind::Statement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let target = match node {
            Node::ClassLikeMember(ClassLikeMember::Property(p)) => Target::Hint(p.hint()),
            Node::ClassLikeMember(ClassLikeMember::Constant(c)) => Target::Hint(c.hint.as_ref()),
            Node::Statement(Statement::Expression(e)) => match e.expression {
                Expression::Assignment(a) if a.operator.is_assign() => Target::Expr(a.rhs),
                _ => Target::Other,
            },
            _ => return,
        };
        // The docblock comes before the attributes, which a member's span leaves out.
        let attributes = match node {
            Node::ClassLikeMember(ClassLikeMember::Property(Property::Plain(p))) => {
                p.attribute_lists.first()
            }
            Node::ClassLikeMember(ClassLikeMember::Property(Property::Hooked(p))) => {
                p.attribute_lists.first()
            }
            Node::ClassLikeMember(ClassLikeMember::Constant(c)) => c.attribute_lists.first(),
            _ => None,
        };
        let start = attributes.map_or(node_span(&node), node_span).start;
        let Some(doc) = Doc::last_of(ctx, ancestors, start) else {
            return;
        };
        let Some((_, tag)) = first_named(&doc, "@var") else {
            return;
        };
        let facts = facts(ctx);
        let scope = TypeScope::new(ancestors, facts.documented());
        let dead = with_classes(facts.documented(), || {
            is_dead(&tag, &target, &scope, &facts)
        });
        if !dead || is_allowed(&tag.ty) {
            return;
        }
        let removed = removed_by_type(&doc, Kind::Var, &tag.variable);
        out.push(Report {
            span: doc.span,
            message: "Remove the `@var` tag, which says what the native type says.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![doc.remove_last(ctx.source, &removed)],
            }),
        });
    }
}

pub(crate) enum Target<'n, 'arena> {
    Hint(Option<&'n Hint<'arena>>),
    Expr(&'n Expression<'arena>),
    Mapped(RType),
    Other,
}

pub(crate) fn is_dead(
    tag: &TypeTag,
    target: &Target<'_, '_>,
    scope: &TypeScope<'_, '_, '_, '_>,
    facts: &Facts,
) -> bool {
    if matches!(target, Target::Hint(None)) || tag.has_description {
        return false;
    }
    let property_type = match target {
        Target::Hint(Some(hint)) => scope.map_hint(hint),
        Target::Expr(e) => RType::T(expr_type(facts, scope.ancestors(), e, false)),
        Target::Mapped(t) => t.clone(),
        Target::Hint(None) | Target::Other => return false,
    };
    if matches!(tag.ty, pun_phpstan::phpdoc::Node::Generic { .. }) {
        return false;
    }
    let doc_type = scope.map_doc(&tag.ty);
    if let Target::Expr(e) = target {
        let native = RType::T(expr_type(facts, scope.ancestors(), e, true));
        if !equals(&doc_type, &native) {
            return false;
        }
    }
    if !is_legal_without_template(&doc_type) {
        return false;
    }
    if property_type.is_union() && !doc_type.is_union() {
        return !doc_type.is_intersection();
    }
    if property_type.is_object_type() && doc_type.is_object_type() {
        if equals(&doc_type, &property_type) {
            return true;
        }
        return is_super(&doc_type, &property_type).yes();
    }
    if are_native_and_doc_types_equal(&property_type, &doc_type, scope.class_info()) {
        return true;
    }
    doc_type.is_union() && are_types_equal(&remove_null(&doc_type), &property_type)
}

pub(crate) fn var_removal_if_not_comment(
    doc: &Doc<'_>,
    t: &RType,
    scope: &TypeScope<'_, '_, '_, '_>,
) -> Option<Vec<bool>> {
    let (_, tag) = first_named(doc, "@var")?;
    if tag.has_description || is_allowed(&tag.ty) {
        return None;
    }
    let var_type = scope.map_doc(&tag.ty);
    if !are_types_equal(t, &var_type) && crate::rules::types::compare::is_subtype(&var_type, t) {
        return None;
    }
    Some(removed_by_type(doc, Kind::Var, ""))
}
