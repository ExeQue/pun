use mago_syntax::cst::*;

use crate::rules::support::statements::{List, array_item};
use crate::syntax::unparenthesize;

pub(crate) fn used_in(node: Node<'_, '_>, parent: Option<Node<'_, '_>>, name: &[u8]) -> bool {
    match node {
        Node::DirectVariable(v) => {
            return !matches!(
                parent,
                Some(Node::PropertyAbstractItem(_) | Node::PropertyConcreteItem(_))
            ) && same_name(name, &v.name[1..]);
        }
        Node::IndirectVariable(_) | Node::NestedVariable(_) => return true,
        Node::IncludeConstruct(_)
        | Node::IncludeOnceConstruct(_)
        | Node::RequireConstruct(_)
        | Node::RequireOnceConstruct(_) => return true,
        Node::StaticPropertyAccess(access) => {
            return used_in(Node::Expression(access.class), Some(node), name)
                || match &access.property {
                    Variable::Direct(_) => false,
                    Variable::Nested(n) => used_in(Node::Variable(n.variable), Some(node), name),
                    Variable::Indirect(i) => {
                        used_in(Node::Expression(i.expression), Some(node), name)
                    }
                };
        }
        Node::FunctionCall(call) if is_compact(call.function) => {
            let mut arguments = call.argument_list.arguments.iter().map(|a| match a {
                Argument::Positional(a) => a.value,
                Argument::Named(a) => a.value,
            });
            if arguments.any(|v| names_in_compact(v, name)) {
                return true;
            }
        }
        _ => {}
    }
    let mut found = false;
    node.visit_children(|child| found = found || used_in(child, Some(node), name));
    found
}

pub(crate) fn same_name(variable: &[u8], used: &[u8]) -> bool {
    match used {
        b"" => false,
        b"Object" => variable == used,
        _ => variable.eq_ignore_ascii_case(used),
    }
}

fn is_compact(function: &Expression<'_>) -> bool {
    match function {
        Expression::Identifier(Identifier::Local(id)) => id.value.eq_ignore_ascii_case(b"compact"),
        Expression::Identifier(Identifier::FullyQualified(id)) => {
            id.value.eq_ignore_ascii_case(b"\\compact")
        }
        _ => false,
    }
}

fn names_in_compact(value: &Expression<'_>, name: &[u8]) -> bool {
    match unparenthesize(value) {
        Expression::Literal(Literal::String(s)) => s.value == Some(name),
        Expression::Array(a) => array_names(&a.elements, name),
        Expression::LegacyArray(a) => array_names(&a.elements, name),
        _ => false,
    }
}

fn array_names(elements: &TokenSeparatedSequence<'_, ArrayElement<'_>>, name: &[u8]) -> bool {
    elements
        .iter()
        .filter_map(array_item)
        .any(|(_, value)| names_in_compact(value, name))
}

pub(crate) fn used_in_next(
    list: &List<'_, '_>,
    from: usize,
    try_: Option<&Try<'_>>,
    name: &[u8],
) -> bool {
    list.stmts[from..]
        .iter()
        .any(|s| used_in(Node::Statement(s), None, name))
        || try_.is_some_and(|t| {
            t.catch_clauses
                .iter()
                .any(|c| used_in(Node::TryCatchClause(c), None, name))
                || t.finally_clause
                    .iter()
                    .any(|f| used_in(Node::Block(&f.block), None, name))
        })
}

pub(crate) fn try_of<'ast, 'arena>(ancestors: &[Node<'ast, 'arena>]) -> Option<&'ast Try<'arena>> {
    let n = ancestors.len();
    match (
        ancestors.get(n.checked_sub(3)?),
        ancestors.get(n.checked_sub(2)?),
    ) {
        (Some(Node::Try(t)), Some(Node::Block(b))) if std::ptr::eq(*b, &raw const t.block) => {
            Some(t)
        }
        _ => None,
    }
}
