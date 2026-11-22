use crate::problem::{Edit, Safety, Span};
use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_codex::symbol::SymbolKind;
use mago_syntax::cst::{
    ClassLikeMember, ClassLikeMemberSelector, Expression, LocalIdentifier, Node, NodeKind, Sequence,
};
use serde_json::Value;

use super::super::types::facts::facts;
use super::{
    assert_class, assert_method, is_class, is_object_type, items, object, object_class_name, string,
};
use crate::rules::support::class_members::qualified;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Clone, Debug)]
pub struct MethodRename {
    pub class: String,
    pub old: String,
    pub new: String,
    pub array_key: Option<Value>,
}

#[derive(Default)]
pub struct RenameMethod {
    pub renames: Vec<MethodRename>,
}

fn has_method(class: &ClassLikeMetadata, name: &str) -> bool {
    (class.declaring_method_ids.keys())
        .chain(class.appearing_method_ids.keys())
        .any(|m| m.as_bytes().eq_ignore_ascii_case(name.as_bytes()))
}

fn is_enum_case_of(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    var: &Expression<'_>,
    class: &str,
) -> bool {
    let Expression::Access(mago_syntax::cst::Access::ClassConstant(fetch)) = unparenthesize(var)
    else {
        return false;
    };
    let Expression::Identifier(id) = fetch.class else {
        return false;
    };
    let name = crate::rules::support::names::resolve_class_name(ancestors, id);
    facts(ctx)
        .codebase
        .get_class_like(name.as_bytes())
        .is_some_and(|c| c.kind == SymbolKind::Enum && c.original_name.to_string() == class)
}

impl RenameMethod {
    fn visit_call(
        &self,
        node: Node<'_, '_>,
        ancestors: &[Node<'_, '_>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (receiver, selector, is_static) = match node {
            Node::MethodCall(c) => (c.object, &c.method, false),
            Node::NullSafeMethodCall(c) => (c.object, &c.method, false),
            Node::StaticMethodCall(c) => (c.class, &c.method, true),
            _ => return,
        };
        let ClassLikeMemberSelector::Identifier(name) = selector else {
            return;
        };
        let called = String::from_utf8_lossy(name.value);
        for rename in &self.renames {
            if !called.eq_ignore_ascii_case(&rename.old) {
                continue;
            }
            let matches = match is_static {
                true => is_object_type(ctx, ancestors, receiver, &rename.class),
                false => {
                    is_enum_case_of(ctx, ancestors, receiver, &rename.class)
                        || is_object_type(ctx, ancestors, receiver, &rename.class)
                }
            };
            if !matches || self.skips_call(ctx, receiver, rename) {
                continue;
            }
            let mut edits = vec![Edit::new(node_span(name), rename.new.clone())];
            if let Some(key) = &rename.array_key {
                let end = node_span(&node).end;
                edits.push(Edit::new(
                    Span::new(end, end),
                    format!("[{}]", key_text(key)),
                ));
            }
            out.push(Report::fixed(
                node_span(&node),
                format!("Call `{}()` instead of `{}()`.", rename.new, rename.old),
                Safety::Unsafe,
                edits,
            ));
            return;
        }
    }

    fn skips_call(
        &self,
        ctx: &Context<'_>,
        receiver: &Expression<'_>,
        rename: &MethodRename,
    ) -> bool {
        let facts = facts(ctx);
        let codebase = &facts.codebase;
        let Some(class) =
            object_class_name(ctx, receiver).and_then(|n| codebase.get_class_like(n.as_bytes()))
        else {
            return false;
        };
        let Some((_, id)) = (class.appearing_method_ids.iter())
            .find(|(m, _)| m.as_bytes().eq_ignore_ascii_case(rename.old.as_bytes()))
        else {
            return false;
        };
        let declaring = id.get_class_name();
        let private = (codebase.get_declaring_method(declaring.as_bytes(), rename.old.as_bytes()))
            .and_then(|m| m.method_metadata.as_ref())
            .is_some_and(|m| m.visibility.is_private());
        let source = match private {
            true => class,
            false => codebase
                .get_class_like(declaring.as_bytes())
                .unwrap_or(class),
        };
        codebase
            .get_class_like(rename.class.as_bytes())
            .is_some_and(|target| source.original_name != target.original_name)
            && has_method(source, &rename.new)
    }

    fn visit_class_like(
        &self,
        (name, members, kind): (
            &LocalIdentifier<'_>,
            &Sequence<'_, ClassLikeMember<'_>>,
            SymbolKind,
        ),
        ancestors: &[Node<'_, '_>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let span = node_span(name);
        let name = name.value;
        let codebase = &facts(ctx).codebase;
        let fqcn = qualified(ancestors, name);
        let own = codebase.get_class_like(fqcn.as_bytes());
        let declares = |method: &str| {
            members.iter().any(|m| {
                matches!(m, ClassLikeMember::Method(m) if m.name.value.eq_ignore_ascii_case(method.as_bytes()))
            })
        };
        let mut edits = Vec::new();
        for member in members {
            let ClassLikeMember::Method(method) = member else {
                continue;
            };
            let method_name = String::from_utf8_lossy(method.name.value);
            let renamed = self.renames.iter().find(|rename| {
                if !method_name.eq_ignore_ascii_case(&rename.old) {
                    return false;
                }
                let Some(own) = own else { return false };
                let of_type = is_class(ctx, &own.original_name.to_string(), &rename.class)
                    || uses_trait(ctx, own, &rename.class);
                if !of_type {
                    return false;
                }
                if kind == SymbolKind::Interface
                    && codebase
                        .get_class_like(rename.class.as_bytes())
                        .is_some_and(|c| {
                            c.all_parent_classes
                                .iter()
                                .chain(c.all_parent_interfaces.iter())
                                .filter_map(|p| codebase.get_class_like(p.as_bytes()))
                                .any(|p| has_method(p, &rename.old))
                        })
                {
                    return false;
                }
                !declares(&rename.new)
            });
            if let Some(rename) = renamed {
                edits.push(Edit::new(node_span(&method.name), rename.new.clone()));
            }
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report::fixed(
            span,
            "Rename the methods the configuration renames.",
            Safety::Unsafe,
            edits,
        ));
    }
}

fn uses_trait(ctx: &Context<'_>, class: &ClassLikeMetadata, r#trait: &str) -> bool {
    let codebase = &facts(ctx).codebase;
    let direct = |c: &ClassLikeMetadata| {
        (c.used_traits.iter()).any(|t| t.as_bytes().eq_ignore_ascii_case(r#trait.as_bytes()))
    };
    direct(class)
        || class
            .all_parent_classes
            .iter()
            .filter_map(|p| codebase.get_class_like(p.as_bytes()))
            .any(direct)
}

fn key_text(key: &Value) -> String {
    match key {
        Value::String(s) => format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'")),
        other => other.to_string(),
    }
}

impl Rule for RenameMethod {
    fn id(&self) -> &'static str {
        "lint/refactor/rename-method"
    }

    fn once(&self) -> bool {
        true
    }

    fn configure(&mut self, options: &Value) -> Result<(), OptionsError> {
        const KEYS: &[&str] = &["class", "old-method", "new-method", "array-key"];
        let mut renames = Vec::new();
        for item in items(options)? {
            let map = object(item, KEYS, &KEYS[..3])?;
            let rename = MethodRename {
                class: string(map, "class")?,
                old: string(map, "old-method")?,
                new: string(map, "new-method")?,
                array_key: match map.get("array-key") {
                    None => None,
                    Some(key @ Value::String(_)) => Some(key.clone()),
                    Some(key) if key.is_i64() => Some(key.clone()),
                    Some(_) => return Err("`array-key` must be a string or an integer".into()),
                },
            };
            assert_class(&rename.class)?;
            assert_method(&rename.old)?;
            assert_method(&rename.new)?;
            renames.push(rename);
        }
        self.renames = renames;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::MethodCall,
            NodeKind::NullSafeMethodCall,
            NodeKind::StaticMethodCall,
            NodeKind::Class,
            NodeKind::Interface,
            NodeKind::Trait,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if self.renames.is_empty() {
            return;
        }
        let class_like = match node {
            Node::Class(c) => (&c.name, &c.members, SymbolKind::Class),
            Node::Interface(i) => (&i.name, &i.members, SymbolKind::Interface),
            Node::Trait(t) => (&t.name, &t.members, SymbolKind::Trait),
            _ => return self.visit_call(node, ancestors, ctx, out),
        };
        self.visit_class_like(class_like, ancestors, ctx, out);
    }
}
