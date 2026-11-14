use mago_codex::symbol::SymbolKind;
use mago_span::{HasSpan, Span};
use mago_syntax::cst::{AnonymousClass, Function, Method, Statement};
use pun_phpstan::phpdoc::{self, TagValue};
use pun_phpstan::resolve::resolve_in;
use pun_phpstan::types::{Level, Type, intersect, is_super_type_of, union};

use super::doc::NameScope;
use super::facts::DocEntry;
use super::{Checker, declared, require_tags, var_tags};

pub(super) const IDENTIFIERS: &[&str] = &["throws.notThrowable"];

pub(super) fn statement(c: &mut Checker, statement: &Statement) {
    if matches!(
        statement,
        Statement::Class(_)
            | Statement::Interface(_)
            | Statement::Enum(_)
            | Statement::Trait(_)
            | Statement::Function(_)
            | Statement::OpeningTag(_)
            | Statement::ClosingTag(_)
            | Statement::Inline(_)
            | Statement::Block(_)
    ) {
        return;
    }
    node(c, statement.span());
}

pub(super) fn function(c: &mut Checker, function: &Function) {
    check(c, function.span(), |c, entry| {
        declared::function_scope(c, function, &entry.doc)
    });
}

pub(super) fn method(c: &mut Checker, method: &Method) {
    let (Some(context), Some(declaring)) = (
        c.class_scope(),
        super::missing_types::declaring(c, method.span()),
    ) else {
        return;
    };
    check(c, method.span(), |c, entry| {
        declared::method_scope(c, method, &entry.doc, declaring, context)
    });
}

pub(super) fn anonymous(c: &mut Checker, class: &AnonymousClass) {
    let start = class
        .attribute_lists
        .first()
        .map(HasSpan::span)
        .or_else(|| class.modifiers.first().map(HasSpan::span))
        .unwrap_or_else(|| class.class.span());
    node(c, start.join(class.span()));
}

pub(super) fn node(c: &mut Checker, at: Span) {
    check(c, at, |c, _| statement_scope(c, at));
}

fn statement_scope(c: &Checker, at: Span) -> NameScope {
    let scope = var_tags::scope(c, at);
    let Some(context) = c.class_scope() else {
        return scope;
    };
    let owner = c
        .in_trait()
        .and_then(|name| c.codebase.get_class_like(name.as_bytes()))
        .unwrap_or(context);
    for property in owner.properties.values() {
        for hook in property.hooks.values() {
            if hook.span.file_id == at.file_id
                && hook.span.start.offset <= at.start.offset
                && at.end.offset <= hook.span.end.offset
            {
                let templates = c
                    .doc_before(c.file.id, hook.span.start.offset)
                    .map_or(&[][..], |(_, entry)| &entry.doc.templates);
                let name = format!("{}::{}", property.name.0, hook.name);
                return declared::with_method_templates(&scope, context, &name, templates);
            }
        }
    }
    scope
}

fn check(c: &mut Checker, at: Span, scope: impl FnOnce(&Checker, &DocEntry) -> NameScope) {
    if c.level < 2 {
        return;
    }
    let Some((_, entry)) = c.doc_before(c.file.id, at.start.offset) else {
        return;
    };
    let message = declared::with_classes(c, || {
        let scope = scope(c, entry);
        let ty = throws_type(&entry.phpdoc, &scope)?;
        (!is_throws_valid(c, &ty)).then(|| {
            format!(
                "PHPDoc tag @throws with type {} is not subtype of Throwable",
                ty.describe(Level::TypeOnly)
            )
        })
    });
    if let Some(message) = message {
        c.report(at, "throws.notThrowable", message, None);
    }
}

pub(super) fn throws_type(tags: &[phpdoc::Tag], scope: &NameScope) -> Option<Type> {
    for name in ["@phpstan-throws", "@throws"] {
        let types: Vec<_> = tags
            .iter()
            .filter(|t| t.name == name)
            .filter_map(|t| match &t.value {
                TagValue::Type(node) => Some(resolve_in(node, scope)),
                _ => None,
            })
            .collect();
        if !types.is_empty() {
            return Some(union(types));
        }
    }
    None
}

fn is_throws_valid(c: &Checker, ty: &Type) -> bool {
    if ty.is_void().yes() {
        return true;
    }
    if let Type::Conditional(conditional) = ty {
        return is_throws_valid(c, &conditional.then) && is_throws_valid(c, &conditional.otherwise);
    }
    if let Type::Union(types) = ty {
        return types
            .iter()
            .all(|inner| !inner.is_void().yes() && is_throws_valid(c, inner));
    }
    let mut constrained = vec![ty.clone()];
    let mut reflected = ty;
    while let Type::Template(template) = reflected {
        reflected = &template.bound;
    }
    for name in reflected.object_class_names() {
        if let Some(class) = c.codebase.get_class_like(name.as_bytes())
            && class.kind == SymbolKind::Interface
        {
            constrained.extend(require_tags::tag_types(
                c,
                class,
                ["@psalm-require-extends", "@phpstan-require-extends"],
            ));
        }
    }
    is_super_type_of(
        &Type::Object("Throwable".into(), vec![], false),
        &intersect(constrained),
    )
    .yes()
}
