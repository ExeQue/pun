use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{
    Access, Assignment, AssignmentOperator, Block, ClassLikeMember, ClassLikeMemberSelector,
    Expression, FunctionLikeParameterList, FunctionLikeReturnTypeHint, Hint, Method, MethodBody,
    Node, Property, PropertyItem, Return, Sequence, Statement, Variable,
};
use pun_phpstan::types::Type;

use super::silent_void::Place;
use super::{class_scope, has_only_return_with_expr, returns_scoped};
use crate::Report;
use crate::rules::support::phpdoc_writer::{DocTarget, PhpDocInfo};
use crate::syntax::is_this;
use crate::syntax::node_span;

pub(crate) struct FunctionLike<'ast, 'arena> {
    pub name: Span,
    pub name_text: &'arena [u8],
    pub start: usize,
    pub body: Option<&'ast Block<'arena>>,
    pub parameters: &'ast FunctionLikeParameterList<'arena>,
    pub return_type: Option<&'ast FunctionLikeReturnTypeHint<'arena>>,
    pub method: Option<&'ast Method<'arena>>,
}

impl<'ast, 'arena> FunctionLike<'ast, 'arena> {
    pub(crate) fn of(node: Node<'ast, 'arena>) -> Option<FunctionLike<'ast, 'arena>> {
        match node {
            Node::Function(f) => Some(FunctionLike {
                name: node_span(&f.name),
                name_text: f.name.value,
                start: node_span(f).start,
                body: Some(&f.body),
                parameters: &f.parameter_list,
                return_type: f.return_type_hint.as_ref(),
                method: None,
            }),
            Node::Method(m) => Some(FunctionLike {
                name: node_span(&m.name),
                name_text: m.name.value,
                start: node_span(m).start,
                body: match &m.body {
                    MethodBody::Concrete(body) => Some(body),
                    MethodBody::Abstract(_) => None,
                },
                parameters: &m.parameter_list,
                return_type: m.return_type_hint.as_ref(),
                method: Some(m),
            }),
            _ => None,
        }
    }

    pub(crate) fn has_non_array_return_type(&self) -> bool {
        self.return_type.is_some_and(|r| !is_array_hint(&r.hint))
    }

    pub(crate) fn is_private(&self) -> bool {
        self.method.is_some_and(|m| {
            m.modifiers
                .iter()
                .any(mago_syntax::cst::Modifier::is_private)
        })
    }

    pub(crate) fn only_return_with_expr(
        &self,
        place: &Place<'_, '_, '_>,
    ) -> Option<&'ast Return<'arena>> {
        let body = self.body?;
        let returns = returns_scoped(body);
        if !has_only_return_with_expr(place, body, &returns) || returns.len() != 1 {
            return None;
        }
        Some(returns[0])
    }

    pub(crate) fn target(&self, ctx: &crate::Context<'_>) -> DocTarget {
        DocTarget::of(ctx, self.start)
    }
}

pub(crate) fn is_array_hint(hint: &Hint<'_>) -> bool {
    matches!(hint, Hint::Array(_))
}

pub(crate) fn class_name(ancestors: &[Node<'_, '_>]) -> Option<String> {
    class_scope(ancestors).and_then(|s| s.name)
}

pub(crate) fn doc_report(
    span: Span,
    message: &str,
    source: &str,
    target: &DocTarget,
    info: &PhpDocInfo,
) -> Option<Report> {
    report(
        span,
        message,
        source,
        vec![target.edit(source, &info.print())],
    )
}

pub(crate) fn docs_report(
    span: Span,
    message: &str,
    source: &str,
    changes: &[(DocTarget, PhpDocInfo)],
) -> Option<Report> {
    let edits = changes
        .iter()
        .map(|(target, info)| target.edit(source, &info.print()))
        .collect();
    report(span, message, source, edits)
}

fn report(span: Span, message: &str, source: &str, mut edits: Vec<Edit>) -> Option<Report> {
    edits.retain(|edit| source[edit.span.start..edit.span.end] != edit.text);
    (!edits.is_empty()).then(|| Report::fixed(span, message.to_string(), Safety::Unsafe, edits))
}

pub(crate) fn referenced_classes(t: &Type) -> Vec<String> {
    match t {
        Type::Object(name, args, _) => {
            let mut out = vec![name.clone()];
            out.extend(args.iter().flat_map(referenced_classes));
            out
        }
        Type::EnumCase(name, _) | Type::This(name) => vec![name.clone()],
        Type::ClassString(Some(inner)) => referenced_classes(inner),
        Type::Union(types) | Type::Benevolent(types) | Type::Intersection(types) => {
            types.iter().flat_map(referenced_classes).collect()
        }
        Type::Array(key, item) | Type::Iterable(key, item) => {
            let mut out = referenced_classes(key);
            out.extend(referenced_classes(item));
            out
        }
        Type::ConstArray(a) => a
            .keys
            .iter()
            .chain(a.values.iter())
            .flat_map(referenced_classes)
            .collect(),
        _ => vec![],
    }
}

pub(crate) fn class_members<'ast, 'arena>(
    node: Node<'ast, 'arena>,
) -> Option<(&'ast Sequence<'arena, ClassLikeMember<'arena>>, bool, Span)> {
    match node {
        Node::Class(c) => Some((&c.members, false, node_span(&c.name))),
        Node::AnonymousClass(c) => Some((&c.members, true, node_span(&c.class))),
        _ => None,
    }
}

pub(crate) fn methods<'ast, 'arena>(
    members: &'ast Sequence<'arena, ClassLikeMember<'arena>>,
) -> impl Iterator<Item = &'ast Method<'arena>> {
    members.iter().filter_map(|m| match m {
        ClassLikeMember::Method(m) => Some(m),
        _ => None,
    })
}

pub(crate) fn method<'ast, 'arena>(
    members: &'ast Sequence<'arena, ClassLikeMember<'arena>>,
    name: &str,
) -> Option<&'ast Method<'arena>> {
    methods(members).find(|m| m.name.value.eq_ignore_ascii_case(name.as_bytes()))
}

pub(crate) struct PropertyView<'ast, 'arena> {
    pub start: usize,
    pub hint: Option<&'ast Hint<'arena>>,
    pub names: Vec<&'arena str>,
    pub default: Option<&'ast Expression<'arena>>,
    pub is_private: bool,
}

pub(crate) fn properties<'ast, 'arena>(
    members: &'ast Sequence<'arena, ClassLikeMember<'arena>>,
) -> Vec<PropertyView<'ast, 'arena>> {
    let name = |item: &'ast PropertyItem<'arena>| {
        let v = match item {
            PropertyItem::Abstract(i) => &i.variable,
            PropertyItem::Concrete(i) => &i.variable,
        };
        std::str::from_utf8(&v.name[1..]).unwrap_or("")
    };
    let default = |item: &'ast PropertyItem<'arena>| match item {
        PropertyItem::Concrete(i) => Some(i.value),
        PropertyItem::Abstract(_) => None,
    };
    members
        .iter()
        .filter_map(|m| match m {
            ClassLikeMember::Property(p) => Some(p),
            _ => None,
        })
        .map(|p| {
            let (modifiers, hint, items): (_, _, Vec<&'ast PropertyItem<'arena>>) = match p {
                Property::Plain(p) => (&p.modifiers, p.hint.as_ref(), p.items.iter().collect()),
                Property::Hooked(p) => (&p.modifiers, p.hint.as_ref(), vec![&p.item]),
            };
            PropertyView {
                start: node_span(p).start,
                hint,
                names: items.iter().map(|i| name(i)).collect(),
                default: items.first().and_then(|i| default(i)),
                is_private: modifiers.iter().any(mago_syntax::cst::Modifier::is_private),
            }
        })
        .collect()
}

pub(crate) fn variable_name<'a>(expr: &Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Variable(Variable::Direct(v)) => std::str::from_utf8(&v.name[1..]).ok(),
        _ => None,
    }
}

pub(crate) fn this_property<'a>(expr: &Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Access(Access::Property(p)) if is_this(p.object) => match &p.property {
            ClassLikeMemberSelector::Identifier(id) => std::str::from_utf8(id.value).ok(),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn method_statements<'ast, 'arena>(
    m: &'ast Method<'arena>,
) -> Option<Vec<&'ast Statement<'arena>>> {
    match &m.body {
        MethodBody::Concrete(b) => Some(b.statements.iter().collect()),
        MethodBody::Abstract(_) => None,
    }
}

pub(crate) fn returned_this_property<'arena>(m: &Method<'arena>) -> Option<&'arena str> {
    match method_statements(m)?.as_slice() {
        [Statement::Return(r)] => this_property(r.value?),
        _ => None,
    }
}

pub(crate) fn find_scoped<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    found: &mut dyn FnMut(Node<'ast, 'arena>),
) {
    node.visit_children(|child| {
        found(child);
        match child {
            Node::Function(_) | Node::Closure(_) | Node::Class(_) => {}
            Node::AnonymousClass(class) => {
                if let Some(args) = &class.argument_list {
                    find_scoped(Node::PartialArgumentList(args), found);
                }
            }
            _ => find_scoped(child, found),
        }
    });
}

pub(crate) fn assignments<'ast, 'arena>(node: Node<'ast, 'arena>) -> Vec<&'ast Assignment<'arena>> {
    let mut out = Vec::new();
    find_scoped(node, &mut |n| {
        if let Node::Assignment(a) = n
            && matches!(a.operator, AssignmentOperator::Assign(_))
        {
            out.push(a);
        }
    });
    out
}
