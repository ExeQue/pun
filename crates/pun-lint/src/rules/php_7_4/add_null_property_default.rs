//! Rector 2.8.4's `RestoreDefaultNullToNullableTypePropertyRector`.

use mago_syntax::cst::{ClassLikeMember, Hint, Modifier, Node, NodeKind, Property, PropertyItem};

use crate::problem::{Edit, Safety, Span};
use crate::rules::dead_code::no_redundant_property_default::is_property_assigned_conditionally;
use crate::rules::support::docblocks::Doc;
use crate::rules::types::add_property_type_constructor::is_test_case;
use crate::rules::types::facts::facts;
use crate::rules::types::property_type::ClassView;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct AddNullPropertyDefault;

impl Rule for AddNullPropertyDefault {
    fn id(&self) -> &'static str {
        "lint/php-7.4/add-null-property-default"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70400)
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
        let Node::Class(class) = node else {
            return;
        };
        // `isReadonlyClass`: native readonly, or an `@immutable` tag.
        if class
            .modifiers
            .iter()
            .any(|m| matches!(m, Modifier::Readonly(_)))
            || has_tag(ctx, ancestors, node_span(class).start, "@immutable")
        {
            return;
        }
        let facts = facts(ctx);
        let Some(view) = ClassView::of(&facts, node, ancestors) else {
            return;
        };
        let test_case = is_test_case(&facts, &view);
        let mut path = ancestors.to_vec();
        path.push(node);
        for member in class.members.iter() {
            // A hooked property has hooks.
            let ClassLikeMember::Property(Property::Plain(property)) = member else {
                continue;
            };
            let Some(hint) = &property.hint else {
                continue;
            };
            let items: Vec<&PropertyItem<'_>> = property.items.iter().collect();
            let [PropertyItem::Abstract(item)] = items[..] else {
                continue;
            };
            // Rector reads no nullable type on a static property.
            if property
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Readonly(_) | Modifier::Static(_)))
                || has_tag(ctx, &path, node_span(property).start, "@readonly")
            {
                continue;
            }
            if !is_nullable(hint) {
                continue;
            }
            // is variable assigned in constructor
            let name = &item.variable.name[1..];
            if is_property_assigned_conditionally(ctx, &view, test_case, name) {
                continue;
            }
            let at = node_span(&item.variable).end;
            out.push(Report::fixed(
                node_span(&item.variable),
                "Give the nullable typed property a `null` default.",
                Safety::Unsafe,
                vec![Edit::new(Span::new(at, at), " = null")],
            ));
        }
    }
}

fn is_nullable(hint: &Hint<'_>) -> bool {
    match hint {
        Hint::Nullable(_) | Hint::Null(_) => true,
        Hint::Union(union) => is_nullable(union.left) || is_nullable(union.right),
        _ => false,
    }
}

fn has_tag(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], start: usize, tag: &str) -> bool {
    Doc::of(ctx, ancestors, start)
        .is_some_and(|doc| (0..doc.len()).any(|i| doc.words(i).first() == Some(&tag)))
}
