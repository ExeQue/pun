use mago_syntax::cst::{Expression, Literal, Node, NodeKind, PartialArgument};

use super::property_attribute::{ClassView, MODEL, is_string_node, moved_value, report, rewrite};
use crate::rules::support::names::resolve_class_name;
use crate::syntax::unparenthesize;
use crate::{Context, Report, Rule};

const TABLE: &str = "Illuminate\\Database\\Eloquent\\Attributes\\Table";

pub struct PreferTableAttribute;

impl Rule for PreferTableAttribute {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-table-attribute"
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
        let Some(class) = ClassView::of(node) else {
            return;
        };
        if !class.is_object_type(ctx, ancestors, MODEL) {
            return;
        }
        let has_existing = class.has_attribute(ancestors, TABLE);
        let table_property = class.property("table");
        let mut table_value = None;
        if let Some((_, property)) = &table_property {
            if !property.is_protected() {
                return;
            }
            match property.default() {
                Some(v) if is_string_node(v) => table_value = Some(v),
                _ => return,
            }
        }
        if table_value.is_none() && has_existing {
            match existing_arg(&class, ancestors, "name") {
                Some(v) if is_string_node(v) => table_value = Some(v),
                _ => return,
            }
        }
        let Some(table_value) = table_value else {
            return;
        };
        let mut options = Vec::new();
        if has_existing {
            for name in ["key", "keyType", "incrementing", "timestamps", "dateFormat"] {
                if let Some(arg) = existing_arg(&class, ancestors, name) {
                    set(&mut options, name, arg);
                }
            }
        }
        let mut removed: Vec<usize> = table_property.iter().map(|(i, _)| *i).collect();
        let mut any_removed = table_property.is_some();
        for (property, option) in [
            ("primaryKey", "key"),
            ("keyType", "keyType"),
            ("incrementing", "incrementing"),
        ] {
            if let Some((index, p)) = class.property(property)
                && p.is_protected()
                && let Some(value) = p.default()
                && !(property == "primaryKey"
                    && matches!(unparenthesize(value), Expression::Literal(Literal::Null(_))))
            {
                set(&mut options, option, value);
                removed.push(index);
                any_removed = true;
            }
        }
        if has_existing && !any_removed {
            return;
        }
        let mut args = vec![format!(
            "name: {}",
            moved_value(ctx.source, &class, table_value)
        )];
        args.extend(
            options
                .iter()
                .map(|(name, value)| format!("{name}: {}", moved_value(ctx.source, &class, value))),
        );
        let group = format!("#[\\{TABLE}({})]", args.join(", "));
        let replaced: Vec<usize> = class
            .attribute_lists
            .iter()
            .position(|list| {
                list.attributes
                    .iter()
                    .any(|a| resolve_class_name(ancestors, &a.name).eq_ignore_ascii_case(TABLE))
            })
            .into_iter()
            .collect();
        let Some(edits) = rewrite(ctx.source, &class, &[group], &replaced, &removed) else {
            return;
        };
        out.push(report(
            &class,
            "Changes model table-related properties to use the Table attribute.",
            edits,
        ));
    }
}

fn set<T>(options: &mut Vec<(&'static str, T)>, name: &'static str, value: T) {
    match options.iter_mut().find(|(n, _)| *n == name) {
        Some(option) => option.1 = value,
        None => options.push((name, value)),
    }
}

fn existing_arg<'ast, 'arena>(
    class: &ClassView<'ast, 'arena>,
    ancestors: &[Node<'_, '_>],
    name: &str,
) -> Option<&'ast Expression<'arena>> {
    class.attribute_lists.iter().find_map(|list| {
        list.attributes
            .iter()
            .filter(|a| resolve_class_name(ancestors, &a.name).eq_ignore_ascii_case(TABLE))
            .find_map(|a| {
                a.argument_list
                    .as_ref()?
                    .arguments
                    .iter()
                    .find_map(|arg| match arg {
                        PartialArgument::Named(n) if n.name.value == name.as_bytes() => {
                            Some(crate::syntax::unparenthesize(n.value))
                        }
                        _ => None,
                    })
            })
    })
}
