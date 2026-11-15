use mago_syntax::cst::{
    AttributeList, ClassLikeMember, Extends, Implements, Modifier, Node, NodeKind, Sequence,
};
use pun_problem::{Edit, Safety, Span};
use serde_json::Value;
use std::fmt::Write as _;

use super::is_a;
use super::names::resolve_class_name;
use super::print::kept_nodes;
use crate::rules::support::class_members::modifiers_text;
use crate::rules::support::class_members::qualified;
use crate::rules::support::conditions::indentation;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

const TRAIT: &str = "Illuminate\\Database\\Eloquent\\Factories\\HasFactory";
const USE: &str = "use \\Illuminate\\Database\\Eloquent\\Factories\\HasFactory;";

#[derive(Default)]
pub struct AddHasFactoryTrait {
    pub items: Vec<String>,
}

struct Parts<'a> {
    attributes: &'a Sequence<'a, AttributeList<'a>>,
    modifiers: &'a Sequence<'a, Modifier<'a>>,
    named: String,
    extends: Option<&'a Extends<'a>>,
    implements: Option<&'a Implements<'a>>,
    left_brace: usize,
    right_brace: usize,
}

impl Rule for AddHasFactoryTrait {
    fn id(&self) -> &'static str {
        "lint/laravel/add-has-factory-trait"
    }

    fn configure(&mut self, options: &Value) -> Result<(), OptionsError> {
        let map = match options {
            Value::Null => return Ok(()),
            Value::Object(map) => map,
            _ => return Err("options must be a table".into()),
        };
        if let Some(key) = map.keys().find(|k| *k != "items") {
            return Err(format!("unknown option `{key}`").into());
        }
        let items = (map.get("items").and_then(Value::as_array)).and_then(|a| {
            a.iter()
                .map(|v| v.as_str().filter(|s| !s.contains('*')).map(str::to_string))
                .collect::<Option<Vec<_>>>()
        });
        self.items = items.ok_or("`items` must be a list of class names, without `*`")?;
        Ok(())
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
        let source = ctx.source;
        let (name, class, parts, start, members) = match node {
            Node::AnonymousClass(c) => {
                let start = c.new.span.start.offset as usize;
                let list = c.argument_list.as_ref().filter(|l| !l.arguments.is_empty());
                let parts = Parts {
                    attributes: &c.attribute_lists,
                    modifiers: &c.modifiers,
                    named: list.map_or(String::new(), |l| {
                        let args = kept_nodes(source, l.arguments.iter().map(node_span), start);
                        format!("({})", args.join(", "))
                    }),
                    extends: c.extends.as_ref(),
                    implements: c.implements.as_ref(),
                    left_brace: c.left_brace.start.offset as usize,
                    right_brace: c.right_brace.end.offset as usize,
                };
                let class = facts.anonymous_class_name(mago_span::HasSpan::span(c));
                (None, class, parts, start, &c.members)
            }
            _ => {
                let Node::Class(c) = node else { return };
                let name = qualified(ancestors, c.name.value);
                let parts = Parts {
                    attributes: &c.attribute_lists,
                    modifiers: &c.modifiers,
                    named: format!(" {}", String::from_utf8_lossy(c.name.value)),
                    extends: c.extends.as_ref(),
                    implements: c.implements.as_ref(),
                    left_brace: c.left_brace.start.offset as usize,
                    right_brace: c.right_brace.end.offset as usize,
                };
                let start = (c.attribute_lists.first().map(|a| node_span(a).start))
                    .or_else(|| c.modifiers.first().map(|m| node_span(m).start))
                    .unwrap_or(c.class.span.start.offset as usize);
                (Some(name.clone()), name, parts, start, &c.members)
            }
        };
        if !is_a(ctx, &class, &["Illuminate\\Database\\Eloquent\\Model"])
            || (!self.items.is_empty()
                && !name
                    .as_ref()
                    .is_some_and(|n| self.items.iter().any(|i| i.eq_ignore_ascii_case(n))))
        {
            return;
        }
        let own = members.iter().any(|m| match m {
            ClassLikeMember::TraitUse(u) => (u.trait_names.iter())
                .any(|t| resolve_class_name(ancestors, t).eq_ignore_ascii_case(TRAIT)),
            _ => false,
        });
        let meta = facts.codebase.get_class_like(class.as_bytes());
        let used = |t: &[u8]| t.eq_ignore_ascii_case(TRAIT.as_bytes());
        if own || meta.is_none_or(|m| m.used_traits.iter().any(|t| used(t.as_bytes()))) {
            return;
        }
        let inside = &source[parts.left_brace + 1..parts.right_brace - 1];
        let first = inside.len() - inside.trim_start().len();
        let edit = if inside.trim().is_empty() {
            Edit::new(
                Span::new(start, parts.right_brace),
                header(ctx, node, &parts, start),
            )
        } else {
            let at = parts.left_brace + 1 + first;
            let indent = " ".repeat(indentation(source, at));
            Edit::new(Span::new(at, at), format!("{USE}\n{indent}"))
        };
        out.push(Report::fixed(
            match node {
                Node::Class(c) => node_span(&c.name),
                _ => Span::new(start, parts.left_brace),
            },
            "Add the `HasFactory` trait to the model.",
            Safety::Unsafe,
            vec![edit],
        ));
    }
}

fn header(ctx: &Context<'_>, node: Node<'_, '_>, parts: &Parts<'_>, start: usize) -> String {
    let source = ctx.source;
    let text = |s: Span| &source[s.start..s.end];
    let nl = format!("\n{}", " ".repeat(indentation(source, start)));
    let mut out = String::new();
    if let Node::AnonymousClass(_) = node {
        out.push_str("new ");
    }
    for list in parts.attributes {
        out.push_str(text(node_span(list)));
        out.push_str(&nl);
    }
    out.push_str(&modifiers_text(parts.modifiers));
    out.push_str("class");
    out.push_str(&parts.named);
    let lists = [
        (" extends", parts.extends.map(|e| &e.types)),
        (" implements", parts.implements.map(|i| &i.types)),
    ];
    for (keyword, types) in lists.into_iter().filter_map(|(k, t)| Some((k, t?))) {
        let names: Vec<&str> = types.iter().map(|t| text(node_span(t))).collect();
        let _ = write!(out, "{keyword} {}", names.join(", "));
    }
    let _ = write!(out, "{nl}{{{nl}    {USE}{nl}}}");
    out
}
