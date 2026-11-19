use mago_syntax::cst::*;
use pun_problem::{Fix, Safety};

use crate::rules::support::class_members::remove_members;
use crate::rules::support::class_members::{
    data_provider_methods, is_class_method_used, is_magic, is_private, walk, with_class_info,
};
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoUnusedPrivateMethod;

impl Rule for NoUnusedPrivateMethod {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-unused-private-method"
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
        // Only a private method can be removed: a class without one needs no more work.
        let Node::Class(c) = node else { return };
        if !(c.members.iter()).any(|m| matches!(m, ClassLikeMember::Method(m) if is_private(m))) {
            return;
        }
        let facts = facts(ctx);
        with_class_info(node, ancestors, &facts, |class| {
            let mut nested = false;
            walk(node, &mut |n, path| {
                nested = nested || (!path.is_empty() && matches!(n, Node::Class(_)));
                !nested
            });
            if nested {
                return;
            }
            let data_providers = data_provider_methods(ctx.source, ancestors, &class);
            let class_string_calls = class_string_static_calls(node);
            let has_call = facts
                .codebase
                .method_exists(class.name.as_bytes(), b"__call");
            let mut removed = vec![false; class.members.len()];
            let mut methods: Vec<&Method<'_>> = Vec::new();
            for (index, member) in class.members.iter().enumerate() {
                let ClassLikeMember::Method(method) = member else {
                    continue;
                };
                let name = method.name.value;
                if !is_private(method)
                    || has_dynamic_method_call_on_this(method)
                    || is_magic(name)
                    || has_call
                    || is_class_method_used(&facts, ancestors, &class, name)
                {
                    continue;
                }
                let named = |names: &[Vec<u8>]| names.iter().any(|n| n.eq_ignore_ascii_case(name));
                if named(&data_providers) || named(&class_string_calls) {
                    continue;
                }
                removed[index] = true;
                methods.push(method);
            }
            if !removed.contains(&true) {
                return;
            }
            let edits = remove_members(
                ctx.source,
                class.members,
                class.left_brace,
                class.right_brace,
                &removed,
            );
            for method in methods {
                out.push(Report {
                    span: node_span(method),
                    message: format!(
                        "Remove the private method `{}()`, which nothing calls.",
                        String::from_utf8_lossy(method.name.value)
                    ),
                    help: None,
                    fix: Some(Fix {
                        safety: Safety::Safe,
                        edits: edits.clone(),
                    }),
                });
            }
        });
    }
}

fn class_string_static_calls(class: Node<'_, '_>) -> Vec<Vec<u8>> {
    let mut names = Vec::new();
    walk(class, &mut |node, _| {
        let (class_expr, method) = match node {
            Node::StaticMethodCall(c) => (c.class, &c.method),
            Node::StaticMethodPartialApplication(c) => (c.class, &c.method),
            _ => return true,
        };
        if let Expression::Access(Access::ClassConstant(fetch)) = class_expr
            && let ClassLikeConstantSelector::Identifier(constant) = &fetch.constant
            && constant.value.eq_ignore_ascii_case(b"class")
            && let ClassLikeMemberSelector::Identifier(name) = method
        {
            names.push(name.value.to_vec());
        }
        true
    });
    names
}

fn has_dynamic_method_call_on_this(method: &Method<'_>) -> bool {
    let mut found = false;
    walk(Node::Method(method), &mut |node, _| {
        let (object, selector) = match node {
            Node::MethodCall(c) => (c.object, &c.method),
            Node::MethodPartialApplication(c) => (c.object, &c.method),
            _ => return !found,
        };
        let this =
            matches!(object, Expression::Variable(Variable::Direct(v)) if v.name == b"$this");
        let variable = match selector {
            ClassLikeMemberSelector::Variable(_) => true,
            ClassLikeMemberSelector::Expression(e) => {
                matches!(e.expression, Expression::Variable(_))
            }
            _ => false,
        };
        found = found || (this && variable);
        !found
    });
    found
}
