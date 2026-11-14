use mago_syntax::cst::{ClassLikeMember, Method, Node};
use pun_problem::{Edit, Span};

use crate::rules::support::arguments::append_item;
use crate::syntax::node_span;

pub(crate) fn class_method<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a Method<'a>> {
    let mut found = None;
    node.visit_children(|child| {
        if let Node::ClassLikeMember(ClassLikeMember::Method(m)) = child
            && found.is_none()
            && m.name.value.eq_ignore_ascii_case(name.as_bytes())
        {
            found = Some(m);
        }
    });
    found
}

pub(crate) fn rename(method: &Method<'_>, name: &str) -> Edit {
    Edit::new(node_span(&method.name), name)
}

pub(crate) fn set_return_type(method: &Method<'_>, r#type: &str) -> Edit {
    match &method.return_type_hint {
        Some(hint) => Edit::new(node_span(&hint.hint), r#type),
        None => {
            let at = method.parameter_list.right_parenthesis.end.offset as usize;
            Edit::new(Span::new(at, at), format!(": {type}"))
        }
    }
}

pub(crate) fn append_parameter(source: &str, method: &Method<'_>, code: &str) -> Edit {
    let list = &method.parameter_list;
    let spans: Vec<Span> = list.parameters.iter().map(node_span).collect();
    append_item(
        source,
        &spans,
        list.left_parenthesis.end.offset as usize,
        code,
    )
}
