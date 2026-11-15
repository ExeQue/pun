use mago_syntax::cst::{Method, Modifier, Node, NodeKind};
use pun_problem::{Edit, Safety, Span};

use super::is_a;
use super::object_type::{is_hint_type, object_class};
use crate::rules::support::names::has_attribute;
use crate::rules::types::facts::facts;
use crate::rules::types::{ClassScope, class_scope};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const MODEL: &str = "Illuminate\\Database\\Eloquent\\Model";

pub struct PreferProtectedModelMethods;

impl Rule for PreferProtectedModelMethods {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-protected-model-methods"
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
        let has = |f: fn(&Modifier<'_>) -> bool| method.modifiers.iter().any(f);
        if has(|m| matches!(m, Modifier::Protected(_) | Modifier::Static(_)))
            || !(is_attribute_method(ctx, ancestors, method)
                || is_scope_method(ctx, ancestors, method))
        {
            return;
        }
        let Some(ClassScope {
            name: Some(own),
            parent_class,
            ..
        }) = class_scope(ancestors)
        else {
            return;
        };
        let is_trait = ancestors.iter().rev().find_map(|a| match a {
            Node::Class(_) | Node::AnonymousClass(_) | Node::Interface(_) | Node::Enum(_) => {
                Some(false)
            }
            Node::Trait(_) => Some(true),
            _ => None,
        }) == Some(true);
        if let Some(parent) = &parent_class
            && has_public_method(ctx, parent, method.name.value, &mut Vec::new())
        {
            return;
        }
        if !is_trait && !is_a(ctx, &own, &[MODEL]) {
            return;
        }
        out.push(Report::fixed(
            node_span(&method.name),
            "Make the model's attribute or scope method protected.",
            Safety::Unsafe,
            vec![make_protected(ctx.source, method)],
        ));
    }
}

fn is_attribute_method(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], method: &Method<'_>) -> bool {
    let name = method.name.value;
    if name.len() > b"getAttribute".len()
        && (name.starts_with(b"get") || name.starts_with(b"set"))
        && name.ends_with(b"Attribute")
    {
        return true;
    }
    method.return_type_hint.as_ref().is_some_and(|r| {
        is_hint_type(
            ctx,
            ancestors,
            &r.hint,
            "Illuminate\\Database\\Eloquent\\Casts\\Attribute",
        )
    })
}

fn is_scope_method(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], method: &Method<'_>) -> bool {
    let name = method.name.value;
    let named = name.len() > b"scopeA".len()
        && name.starts_with(b"scope")
        && name[5].is_ascii_uppercase()
        && method.parameter_list.parameters.first().is_some_and(|p| {
            p.hint.as_ref().is_none_or(|h| {
                is_hint_type(ctx, ancestors, h, "Illuminate\\Database\\Eloquent\\Builder")
            })
        });
    named
        || has_attribute(
            ancestors,
            &method.attribute_lists,
            "Illuminate\\Database\\Eloquent\\Attributes\\Scope",
        )
}

fn has_public_method(ctx: &Context<'_>, class: &str, name: &[u8], seen: &mut Vec<String>) -> bool {
    let facts = facts(ctx);
    let codebase = facts.documented();
    if codebase.method_exists(class.as_bytes(), name) {
        return codebase
            .get_declaring_method(class.as_bytes(), name)
            .and_then(|m| m.method_metadata.as_ref())
            .is_some_and(|m| m.visibility.is_public());
    }
    if seen.iter().any(|s| s.eq_ignore_ascii_case(class)) {
        return false;
    }
    seen.push(class.to_string());
    let Some(meta) = codebase.get_class_like(class.as_bytes()) else {
        return false;
    };
    meta.mixins.iter().any(|mixin| {
        let classes: Vec<Option<String>> =
            mixin.type_union.types.iter().map(object_class).collect();
        !classes.is_empty()
            && classes.iter().all(|c| {
                c.as_deref()
                    .is_some_and(|c| has_public_method(ctx, c, name, seen))
            })
    })
}

fn make_protected(source: &str, method: &Method<'_>) -> Edit {
    let has = |f: fn(&Modifier<'_>) -> bool| method.modifiers.iter().any(f);
    let text = [
        (has(|m| matches!(m, Modifier::Final(_))), "final "),
        (has(|m| matches!(m, Modifier::Abstract(_))), "abstract "),
        (true, "protected "),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .map(|(_, t)| *t)
    .collect::<String>();
    let Some(first) = method.modifiers.first() else {
        let at = method.function.span.start.offset as usize;
        return Edit::new(Span::new(at, at), text);
    };
    let end = node_span(method.modifiers.last().unwrap_or(first)).end;
    let rest = &source[end..];
    Edit::new(
        Span::new(
            node_span(first).start,
            end + rest.len() - rest.trim_start().len(),
        ),
        text,
    )
}
