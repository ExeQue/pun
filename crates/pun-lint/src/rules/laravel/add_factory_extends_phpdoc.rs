use mago_syntax::cst::{
    Access, ClassLikeMember, CompositeString, Expression, Literal, Node, NodeKind, Property,
    PropertyItem,
};
use pun_problem::{Edit, Safety, Span};

use super::document_string::document_value;
use super::is_a;
use super::names::resolve_class_name;
use crate::rules::support::class_members::qualified;
use crate::rules::support::conditions::indentation;
use crate::rules::support::docblocks::Doc;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const FACTORY: &str = "Illuminate\\Database\\Eloquent\\Factories\\Factory";

pub struct AddFactoryExtendsPhpdoc;

impl Rule for AddFactoryExtendsPhpdoc {
    fn id(&self) -> &'static str {
        "lint/laravel/add-factory-extends-phpdoc"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Class(class) = node else { return };
        let name = qualified(ancestors, class.name.value);
        if !is_a(ctx, &name, &[FACTORY]) {
            return;
        }
        let Some(item) = class.members.iter().find_map(|m| {
            let first = match m {
                ClassLikeMember::Property(Property::Plain(p)) => p.items.first(),
                ClassLikeMember::Property(Property::Hooked(p)) => Some(&p.item),
                _ => None,
            };
            first.filter(|i| item_name(i).eq_ignore_ascii_case(b"$model"))
        }) else {
            return;
        };
        let PropertyItem::Concrete(item) = item else {
            return;
        };
        let Some(model) = model_name(ctx, ancestors, item.value) else {
            return;
        };
        let tag = format!("@extends \\{FACTORY}<\\{}>", model.trim_start_matches('\\'));
        let start = node_span(class).start;
        let source = ctx.source;
        let edit = match Doc::of(ctx, ancestors, start) {
            Some(doc) => {
                if (0..doc.len()).any(|i| {
                    doc.words(i)
                        .first()
                        .is_some_and(|w| tag_name(w) == "@extends")
                }) {
                    return;
                }
                Edit::new(
                    doc.span,
                    doc.print_appending(&vec![false; doc.len()], &[&tag]),
                )
            }
            None => {
                let indent = " ".repeat(indentation(source, start));
                Edit::new(
                    Span::new(start, start),
                    format!("/**\n{indent} * {tag}\n{indent} */\n{indent}"),
                )
            }
        };
        out.push(Report::fixed(
            node_span(&class.name),
            "Add the `@extends` tag with the factory's model.",
            Safety::Unsafe,
            vec![edit],
        ));
    }
}

fn item_name<'a>(item: &PropertyItem<'a>) -> &'a [u8] {
    match item {
        PropertyItem::Abstract(a) => a.variable.name,
        PropertyItem::Concrete(c) => c.variable.name,
    }
}

fn model_name(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    value: &Expression<'_>,
) -> Option<String> {
    let bytes = match unparenthesize(value) {
        Expression::Access(Access::ClassConstant(c)) => {
            return match c.class {
                Expression::Identifier(id) => Some(resolve_class_name(ancestors, id)),
                e => matches!(
                    e,
                    Expression::Self_(_) | Expression::Static(_) | Expression::Parent(_)
                )
                .then(|| ctx.source[node_span(e).start..node_span(e).end].to_string()),
            };
        }
        Expression::Literal(Literal::String(s)) => s.value?.to_vec(),
        Expression::CompositeString(CompositeString::Document(d)) => document_value(d)?,
        _ => return None,
    };
    String::from_utf8(bytes).ok()
}

pub(crate) fn tag_name(word: &str) -> &str {
    let end = word
        .char_indices()
        .skip(1)
        .find(|(_, c)| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '\\' | ':')))
        .map_or(word.len(), |(i, _)| i);
    &word[..end]
}
