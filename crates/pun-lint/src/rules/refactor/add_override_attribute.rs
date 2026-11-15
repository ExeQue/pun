use mago_syntax::cst::{ClassLikeMember, Modifier, Node, NodeKind, Property};
use pun_problem::{Edit, Safety, Span};

use super::super::types::facts::facts;
use crate::rules::support::class_members::qualified;
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::statement_edits::line_indent;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct AddOverrideAttribute;

impl Rule for AddOverrideAttribute {
    fn id(&self) -> &'static str {
        "lint/refactor/add-override-attribute"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80500)
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
        let codebase = &facts(ctx).codebase;
        let mut parents = Vec::new();
        let mut next = codebase
            .get_class_like(qualified(ancestors, class.name.value).as_bytes())
            .and_then(|own| own.direct_parent_class);
        while let Some(name) = next {
            let Some(parent) = codebase.get_class_like(name.as_bytes()) else {
                break;
            };
            parents.push(parent);
            next = parent.direct_parent_class;
        }
        let overridden = |name: &[u8]| {
            parents.iter().find_map(|parent| {
                let (_, declaring) = parent
                    .declaring_property_ids
                    .iter()
                    .find(|(k, _)| k.as_bytes() == name)?;
                let declaring = codebase.get_class_like(declaring.as_bytes())?;
                let (_, property) = declaring
                    .properties
                    .iter()
                    .find(|(k, _)| k.as_bytes() == name)?;
                Some(!property.read_visibility.is_private())
            }) == Some(true)
        };
        let mut edits = Vec::new();
        for member in &class.members {
            let ClassLikeMember::Property(property) = member else {
                continue;
            };
            let (lists, modifiers) = match property {
                Property::Plain(p) => (&p.attribute_lists, &p.modifiers),
                Property::Hooked(p) => (&p.attribute_lists, &p.modifiers),
            };
            if modifiers.iter().any(|m| matches!(m, Modifier::Private(_))) {
                continue;
            }
            let has_override = lists.iter().any(|list| {
                list.attributes.iter().any(|a| {
                    resolve_class_name(ancestors, &a.name).eq_ignore_ascii_case("Override")
                })
            });
            if has_override || !property.variables().iter().any(|v| overridden(v.name)) {
                continue;
            }
            let start = node_span(property).start;
            let indent = line_indent(ctx.source, start);
            let width = indent
                .bytes()
                .map(|b| if b == b'\t' { 4 } else { 1 })
                .sum::<usize>();
            let nl = format!("\n{}", " ".repeat(width));
            edits.push(match lists.last() {
                Some(last) => {
                    let end = node_span(last).end;
                    Edit::new(Span::new(end, end), format!("{nl}#[\\Override]"))
                }
                None => Edit::new(Span::new(start, start), format!("#[\\Override]{nl}")),
            });
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report::fixed(
            node_span(&class.name),
            "Add `#[\\Override]` to the properties that override a parent's property.",
            Safety::Unsafe,
            edits,
        ));
    }
}
