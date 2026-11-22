use crate::problem::{Fix, Safety};
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::*;

use crate::rules::support::class_members::qualified;
use crate::rules::support::class_members::remove_members;
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::side_effects::any_node;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoUnusedPrivateConstant;

impl Rule for NoUnusedPrivateConstant {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-unused-private-constant"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass, NodeKind::Enum])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (members, left_brace, right_brace, name, extends) = match node {
            Node::Class(c) => (
                &c.members,
                c.left_brace,
                c.right_brace,
                Some(c.name.value),
                c.extends.as_ref(),
            ),
            Node::AnonymousClass(c) => (
                &c.members,
                c.left_brace,
                c.right_brace,
                None,
                c.extends.as_ref(),
            ),
            Node::Enum(e) => (
                &e.members,
                e.left_brace,
                e.right_brace,
                Some(e.name.value),
                None,
            ),
            _ => return,
        };
        let class = name.map(|n| qualified(ancestors, n));
        let parent = extends
            .and_then(|e| e.types.first())
            .map(|id| resolve_class_name(ancestors, id));
        if parent.as_ref().is_some_and(|p| p.ends_with("Enum")) {
            return;
        }
        let mut scopes = vec![node];
        if members
            .iter()
            .any(|m| matches!(m, ClassLikeMember::TraitUse(_)))
        {
            let mut holders = vec![members];
            let mut next = parent;
            while let Some(name) = next.take() {
                let Some(Node::Class(c)) = declared(ancestors[0], &name) else {
                    return;
                };
                next = c
                    .extends
                    .as_ref()
                    .and_then(|e| e.types.first())
                    .map(|id| resolve_class_name(ancestors, id));
                holders.push(&c.members);
            }
            let mut seen: Vec<String> = Vec::new();
            while let Some(holder) = holders.pop() {
                for member in holder {
                    let ClassLikeMember::TraitUse(r#use) = member else {
                        continue;
                    };
                    for name in &r#use.trait_names {
                        let name = resolve_class_name(ancestors, name);
                        if seen.iter().any(|s| s.eq_ignore_ascii_case(&name)) {
                            continue;
                        }
                        let Some(t @ Node::Trait(r#trait)) = declared(ancestors[0], &name) else {
                            return;
                        };
                        seen.push(name);
                        scopes.push(t);
                        holders.push(&r#trait.members);
                    }
                }
            }
        }
        for (index, member) in members.iter().enumerate() {
            let ClassLikeMember::Constant(constant) = member else {
                continue;
            };
            if !constant
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Private(_)))
                || constant.items.len() != 1
            {
                continue;
            }
            let constant_name = constant.items.first().unwrap().name.value;
            let fetched = scopes.iter().any(|scope| {
                any_node(*scope, &|n| match n {
                    Node::ClassConstantAccess(fetch) => {
                        is_name_match(ctx, ancestors, fetch, class.as_deref(), constant_name)
                    }
                    _ => false,
                })
            });
            if fetched {
                continue;
            }
            let mut removed = vec![false; members.len()];
            removed[index] = true;
            out.push(Report {
                span: node_span(constant),
                message: format!(
                    "Remove the private constant `{}`, which nothing reads.",
                    String::from_utf8_lossy(constant_name)
                ),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits: remove_members(ctx.source, members, left_brace, right_brace, &removed),
                }),
            });
        }
    }
}

fn is_name_match(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    fetch: &ClassConstantAccess<'_>,
    class: Option<&str>,
    name: &[u8],
) -> bool {
    let named = match &fetch.constant {
        ClassLikeConstantSelector::Identifier(id) => Some(id.value.eq_ignore_ascii_case(name)),
        _ => None,
    };
    match fetch.class {
        Expression::Self_(_) | Expression::Static(_) => named.unwrap_or(true),
        Expression::Identifier(id) => {
            named.unwrap_or(true)
                && class.is_some_and(|c| resolve_class_name(ancestors, id).eq_ignore_ascii_case(c))
        }
        Expression::Variable(Variable::Direct(v)) if v.name == b"$this" => named.unwrap_or(true),
        Expression::Parent(_) => false,
        object => {
            let Some(class) = class else { return false };
            let facts = facts(ctx);
            let is_class = facts.native_type(node_span(object)).is_some_and(|t| {
                t.types.iter().all(|a| {
                    matches!(a, TAtomic::Object(TObject::Named(n))
                        if facts.codebase.is_instance_of(n.name.as_bytes(), class.as_bytes()))
                })
            });
            is_class && named.unwrap_or(true)
        }
    }
}

fn declared<'ast, 'arena>(program: Node<'ast, 'arena>, name: &str) -> Option<Node<'ast, 'arena>> {
    let mut found = None;
    find(program, &mut Vec::new(), name, &mut found);
    found
}

fn find<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    path: &mut Vec<Node<'ast, 'arena>>,
    name: &str,
    found: &mut Option<Node<'ast, 'arena>>,
) {
    let declared = match node {
        Node::Class(c) => Some(c.name.value),
        Node::Trait(t) => Some(t.name.value),
        _ => None,
    };
    if let Some(declared) = declared
        && qualified(path, declared).eq_ignore_ascii_case(name)
    {
        *found = Some(node);
        return;
    }
    path.push(node);
    node.visit_children(|child| {
        if found.is_none() {
            find(child, path, name, found);
        }
    });
    path.pop();
}
