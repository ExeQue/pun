use crate::problem::Safety;
use mago_syntax::cst::{MethodBody, Node, NodeKind, Statement};

use super::property_attribute::{
    ClassView, FORM_REQUEST, MODEL, array_elements, is_string_node, moved_value, report, rewrite,
};
use crate::rules::support::expressions::is_constant;
use crate::rules::types::facts::facts;
use crate::rules::types::silent_void::Place;
use crate::rules::types::{class_scope, has_only_return_with_expr, returns_scoped};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const CLASS_TARGETS: &[NodeKind] = &[NodeKind::Class, NodeKind::AnonymousClass];

pub struct PreferUnguardedAttribute;

impl Rule for PreferUnguardedAttribute {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-unguarded-attribute"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(CLASS_TARGETS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        const UNGUARDED: &str = "Illuminate\\Database\\Eloquent\\Attributes\\Unguarded";
        let class = ClassView::of(node).unwrap();
        if !class.is_object_type(ctx, ancestors, MODEL) || class.has_attribute(ancestors, UNGUARDED)
        {
            return;
        }
        let Some((index, property)) = class.property("guarded") else {
            return;
        };
        // Rector drops the whole statement, so the other properties it declares would go with it
        if !property.is_protected()
            || property.items.len() != 1
            || !property
                .default()
                .and_then(array_elements)
                .is_some_and(mago_syntax::cst::TokenSeparatedSequence::is_empty)
        {
            return;
        }
        let Some(edits) = rewrite(
            ctx.source,
            &class,
            &[format!("#[\\{UNGUARDED}]")],
            &[],
            &[index],
        ) else {
            return;
        };
        out.push(report(
            &class,
            "Changes model empty guarded property to use the Unguarded attribute.",
            edits,
        ));
    }
}

pub struct PreferRouteKeyAttribute;

impl Rule for PreferRouteKeyAttribute {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-route-key-attribute"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(CLASS_TARGETS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        const ROUTE_KEY: &str = "Illuminate\\Database\\Eloquent\\Attributes\\RouteKey";
        let class = ClassView::of(node).unwrap();
        if !class.is_object_type(ctx, ancestors, MODEL) || class.has_attribute(ancestors, ROUTE_KEY)
        {
            return;
        }
        let Some((index, method)) = class.method("getRouteKeyName") else {
            return;
        };
        let MethodBody::Concrete(body) = &method.body else {
            return;
        };
        let mut scope = ancestors.to_vec();
        scope.push(node);
        let class_scope = class_scope(&scope);
        let facts = facts(ctx);
        let place = Place {
            facts: &facts,
            ancestors: &scope,
            scope: class_scope.as_ref(),
        };
        let returns = returns_scoped(body);
        if !has_only_return_with_expr(&place, body, &returns) || returns.len() != 1 {
            return;
        }
        let Some(value) = returns[0].value.filter(|v| is_string_node(v)) else {
            return;
        };
        let group = format!(
            "#[\\{ROUTE_KEY}({})]",
            moved_value(ctx.source, &class, value)
        );
        let Some(edits) = rewrite(ctx.source, &class, &[group], &[], &[index]) else {
            return;
        };
        out.push(report(
            &class,
            "Changes model getRouteKeyName() method to use the RouteKey attribute.",
            edits,
        ));
    }
}

pub struct NoDefaultAuthorize;

impl Rule for NoDefaultAuthorize {
    fn id(&self) -> &'static str {
        "lint/laravel/no-default-authorize"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(CLASS_TARGETS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let class = ClassView::of(node).unwrap();
        if !class.is_object_type(ctx, ancestors, FORM_REQUEST) {
            return;
        }
        let Some((index, method)) = class.method("authorize") else {
            return;
        };
        let MethodBody::Concrete(body) = &method.body else {
            return;
        };
        let statements: Vec<&Statement<'_>> = body.statements.iter().collect();
        let returns_true = matches!(
            statements[..],
            [Statement::Return(r)] if r.value.is_some_and(|v| is_constant(v, "true"))
        );
        if !method.parameter_list.parameters.is_empty()
            || !returns_true
            || is_authorize_inherited(ctx, &class.name(ctx, ancestors))
        {
            return;
        }
        let Some(edits) = rewrite(ctx.source, &class, &[], &[], &[index]) else {
            return;
        };
        out.push(Report::fixed(
            node_span(&node),
            "Remove `authorize()`: it only returns `true`, which a form request without it does too.",
            Safety::Safe,
            edits,
        ));
    }
}

/// Removing the method would expose an `authorize()` from a parent, an interface or a trait.
fn is_authorize_inherited(ctx: &Context<'_>, name: &str) -> bool {
    let facts = facts(ctx);
    let codebase = &facts.codebase;
    let Some(class) = codebase.get_class_like(name.as_bytes()) else {
        return true;
    };
    let declares = |c: &mago_word::Word| codebase.method_exists(c.as_bytes(), b"authorize");
    class.all_parent_classes.iter().any(declares)
        || class.all_parent_interfaces.iter().any(declares)
        || class.used_traits.iter().any(declares)
}
