use crate::problem::Span;
use mago_span::HasSpan;
use mago_syntax::cst::{Access, Expression, Method, MethodBody, Node, Statement, Variable};

use crate::syntax::span;

pub(crate) fn class_like_header_span(node: Node<'_, '_>) -> Span {
    span(match node {
        Node::Class(class) => class.class.span().join(class.name.span()),
        Node::Trait(r#trait) => r#trait.r#trait.span().join(r#trait.name.span()),
        Node::Enum(r#enum) => r#enum.r#enum.span().join(r#enum.name.span()),
        Node::Interface(interface) => interface.interface.span().join(interface.name.span()),
        Node::AnonymousClass(class) => class.class.span(),
        _ => node.span(),
    })
}

pub(crate) fn is_method_setter_or_getter(method: &Method<'_>) -> bool {
    let MethodBody::Concrete(block) = &method.body else {
        return false;
    };
    let statements_len = block.statements.len();
    if statements_len > 2 {
        return false;
    }
    let Some(statement) = block.statements.first() else {
        return false;
    };
    match statement {
        Statement::Return(r#return) if method.parameter_list.parameters.is_empty() => {
            r#return
                .value
                .is_some_and(|value| is_accessing_property_of_this(value))
                && statements_len == 1
        }
        Statement::Expression(statement) if method.parameter_list.parameters.len() == 1 => {
            let Expression::Assignment(assignment) = statement.expression else {
                return false;
            };
            if !is_accessing_property_of_this(assignment.lhs) {
                return false;
            }
            match block.statements.last() {
                Some(Statement::Return(r#return)) => r#return
                    .value
                    .is_some_and(|value| is_variable_named(value, b"$this")),
                _ => false,
            }
        }
        _ => false,
    }
}

fn is_accessing_property_of_this(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::Access(Access::Property(access)) => is_variable_named(access.object, b"$this"),
        _ => false,
    }
}

fn is_variable_named(expression: &Expression<'_>, name: &[u8]) -> bool {
    matches!(expression, Expression::Variable(Variable::Direct(v)) if v.name == name)
}

pub(crate) fn function_like_scope<'ast, 'arena>(
    ancestors: &[Node<'ast, 'arena>],
) -> Option<Node<'ast, 'arena>> {
    ancestors.iter().rev().copied().find(|n| {
        matches!(
            n,
            Node::Function(_) | Node::Method(_) | Node::Closure(_) | Node::ArrowFunction(_)
        )
    })
}

pub(crate) fn namespace<'arena>(ancestors: &[Node<'_, 'arena>]) -> &'arena [u8] {
    ancestors
        .iter()
        .rev()
        .find_map(|n| match n {
            Node::Namespace(ns) => Some(ns.name.as_ref().map_or(&b""[..], |n| {
                n.value().strip_prefix(b"\\").unwrap_or(n.value())
            })),
            _ => None,
        })
        .unwrap_or(b"")
}
