use mago_syntax::cst::*;
use pun_problem::{Edit, Fix, Safety, Span};

use crate::rules::phpunit::maybe_multiline;
use crate::rules::support::class_members::modifiers_text;
use crate::rules::support::class_members::{
    ClassInfo, class_info, is_array_callable, is_private, is_static, walk,
};
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::statement_edits::line_indent;
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferInstanceMethod;

impl Rule for PreferInstanceMethod {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-instance-method"
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
        let Some(class) = class_info(node, ancestors, &facts) else {
            return;
        };
        let methods: Vec<&Method<'_>> = class_methods(class.members);
        if !methods.iter().any(|m| is_candidate(m)) {
            return;
        }
        let class_name = if class.is_anonymous {
            String::new()
        } else {
            class.name.clone()
        };
        let mut converted: Vec<Span> = Vec::new();
        let mut edits = Vec::new();
        for method in methods.iter().filter(|m| is_candidate(m)) {
            let name = method.name.value;
            if is_guarded_by_parent(&facts, &class, name) {
                continue;
            }
            let still_static: Vec<Span> = (methods.iter())
                .filter(|m| is_static(m) && !converted.contains(&node_span(**m)))
                .map(|m| node_span(*m))
                .collect();
            if is_called_in_static_method(
                &facts,
                ancestors,
                &class,
                &class_name,
                &still_static,
                name,
            ) {
                continue;
            }
            if is_never_called(class.node, name) {
                continue;
            }
            let mut calls = Vec::new();
            let mut in_static_closure = false;
            for m in &methods {
                walk(Node::Method(m), &mut |n, path| {
                    if let Some((target, called, arguments)) = static_call(n)
                        && called.eq_ignore_ascii_case(name)
                        && is_own(ancestors, target, &class_name)
                    {
                        in_static_closure |= path.iter().any(is_static_closure);
                        calls.push((node_span(&n), arguments));
                    }
                    true
                });
            }
            if in_static_closure {
                continue;
            }
            converted.push(node_span(*method));
            for (span, arguments) in calls {
                edits.push(this_call(ctx, span, arguments, name));
            }
            let modifiers =
                (method.modifiers.first()).map_or(node_span(*method).start, |m| node_span(m).start);
            let function = method.function.span.start.offset as usize;
            edits.push(Edit::new(
                Span::new(modifiers, function),
                modifiers_text(&method.modifiers).replace("static ", ""),
            ));
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report {
            span: node_span(&node),
            message:
                "This private static method is called on the instance; make it an instance method."
                    .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}

fn is_candidate(method: &Method<'_>) -> bool {
    is_private(method) && is_static(method)
}

fn class_methods<'ast, 'arena>(
    members: &'ast Sequence<'arena, ClassLikeMember<'arena>>,
) -> Vec<&'ast Method<'arena>> {
    (members.iter())
        .filter_map(|m| match m {
            ClassLikeMember::Method(m) => Some(m),
            _ => None,
        })
        .collect()
}

fn is_guarded_by_parent(facts: &Facts, class: &ClassInfo<'_, '_>, name: &[u8]) -> bool {
    let codebase = &facts.codebase;
    codebase
        .get_class_like(class.name.as_bytes())
        .is_some_and(|own| {
            (own.all_parent_classes.iter())
                .chain(own.all_parent_interfaces.iter())
                .any(|p| codebase.method_exists(p.as_bytes(), name))
        })
}

fn static_call<'ast, 'arena>(
    node: Node<'ast, 'arena>,
) -> Option<(&'ast Expression<'arena>, &'arena [u8], (usize, Vec<Span>))> {
    let (class, method, arguments) = match node {
        Node::StaticMethodCall(c) => (
            c.class,
            &c.method,
            (
                c.argument_list.left_parenthesis.end.offset as usize,
                c.argument_list.arguments.iter().map(node_span).collect(),
            ),
        ),
        Node::StaticMethodPartialApplication(c) => (
            c.class,
            &c.method,
            (
                c.argument_list.left_parenthesis.end.offset as usize,
                c.argument_list.arguments.iter().map(node_span).collect(),
            ),
        ),
        _ => return None,
    };
    match method {
        ClassLikeMemberSelector::Identifier(id) => Some((class, id.value, arguments)),
        _ => None,
    }
}

fn is_own(ancestors: &[Node<'_, '_>], class: &Expression<'_>, class_name: &str) -> bool {
    match class {
        Expression::Self_(_) | Expression::Static(_) => true,
        Expression::Identifier(id) => {
            !class_name.is_empty()
                && resolve_class_name(ancestors, id).eq_ignore_ascii_case(class_name)
        }
        _ => false,
    }
}

fn is_static_closure(node: &Node<'_, '_>) -> bool {
    match node {
        Node::Closure(c) => c.r#static.is_some(),
        Node::ArrowFunction(f) => f.r#static.is_some(),
        _ => false,
    }
}

fn is_called_in_static_method(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    class: &ClassInfo<'_, '_>,
    class_name: &str,
    still_static: &[Span],
    name: &[u8],
) -> bool {
    let mut found = false;
    walk(class.node, &mut |n, path| {
        if found {
            return false;
        }
        let in_static = (path.iter())
            .any(|p| matches!(p, Node::Method(m) if still_static.contains(&node_span(*m))));
        if !in_static {
            return true;
        }
        found = match n {
            Node::Array(Array { elements, .. })
            | Node::LegacyArray(LegacyArray { elements, .. }) => {
                is_array_callable(facts, ancestors, class, elements, path, name)
            }
            _ => static_call(n).is_some_and(|(target, called, _)| {
                called.eq_ignore_ascii_case(name) && is_own(ancestors, target, class_name)
            }),
        };
        !found
    });
    found
}

fn is_never_called(class: Node<'_, '_>, name: &[u8]) -> bool {
    let mut called = false;
    walk(class, &mut |n, _| {
        called = called
            || match n {
                Node::MethodCall(c) => {
                    matches!(c.object, Expression::Variable(Variable::Direct(v)) if v.name == b"$this")
                        && matches!(&c.method, ClassLikeMemberSelector::Identifier(id) if id.value.eq_ignore_ascii_case(name))
                }
                _ => static_call(n).is_some_and(|(target, called, _)| {
                    called.eq_ignore_ascii_case(name)
                        && matches!(target, Expression::Self_(_) | Expression::Static(_))
                }),
            };
        !called
    });
    !called
}

fn this_call(
    ctx: &crate::Context<'_>,
    span: Span,
    (from, items): (usize, Vec<Span>),
    name: &[u8],
) -> Edit {
    let source = ctx.source;
    let indent = line_indent(source, span.start);
    let args = maybe_multiline(ctx, from, &items, indent);
    let name = String::from_utf8_lossy(name);
    Edit::new(span, format!("$this->{name}({args})"))
}
