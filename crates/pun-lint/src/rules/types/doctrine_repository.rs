use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{
    Call, ClassLikeMemberSelector, Expression, Method, Modifier, Node, NodeKind, Variable,
};

use super::add_property_type_constructor::is_test_case;
use super::facts::facts;
use super::property_type::{ClassView, TYPED_PROPERTIES, VarTag, variable_span};
use crate::rules::phpunit::phpdoc_type::{TagType, is_name};
use crate::rules::support::names::imported;
use crate::syntax::is_this;
use crate::syntax::unparenthesize;
use crate::{Context, Report, Rule};

pub struct AddPropertyTypeGetRepository;

pub struct AddPropertyTypeContainerGet;

impl Rule for AddPropertyTypeGetRepository {
    fn id(&self) -> &'static str {
        "lint/types/add-property-type-get-repository"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(TYPED_PROPERTIES)
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
        type_from_var(
            node,
            ancestors,
            ctx,
            out,
            assigned_via_get_repository,
            "Type the repositories `setUp()` assigns from their `@var`.",
        );
    }
}

impl Rule for AddPropertyTypeContainerGet {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-property-type-container-get"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(TYPED_PROPERTIES)
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
        type_from_var(
            node,
            ancestors,
            ctx,
            out,
            assigned_via_container_get,
            "Type the services `setUp()` fetches from the container from their `@var`.",
        );
    }
}

fn type_from_var<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    ancestors: &[Node<'ast, 'arena>],
    ctx: &Context<'_>,
    out: &mut Vec<Report>,
    assigned: fn(&Method<'_>, &[u8]) -> bool,
    message: &str,
) {
    let facts = facts(ctx);
    let view = ClassView::of(&facts, node, ancestors).expect("the rules target classes");
    if !is_test_case(&facts, &view) {
        return;
    }
    let Some(set_up) = view.method(b"setUp") else {
        return;
    };
    let mut edits: Vec<Edit> = Vec::new();
    for prop in &view.properties() {
        if prop.item.is_none()
            || !prop.is_private()
            || prop
                .modifiers()
                .iter()
                .any(|m| matches!(m, Modifier::Static(_)))
            || !assigned(set_up, prop.name())
        {
            continue;
        }
        let var = VarTag::of(ctx, prop.property, &view.path);
        if var.annotated.doc.is_none() {
            continue;
        }
        let Ok(Some(tag)) = &var.tag else { continue };
        let Some(written) = object_name(&tag.ty) else {
            continue;
        };
        let class = doc_class(&view.path, written);
        let Some(removed) = facts
            .codebase
            .get_class_like(class.as_bytes())
            .and(var.remove(ctx.source))
        else {
            continue;
        };
        let at = variable_span(prop).start;
        edits.push(Edit::new(Span::new(at, at), format!("\\{class} ")));
        edits.extend(removed);
    }
    let Some(first) = edits.first() else { return };
    out.push(Report::fixed(
        first.span,
        message.to_string(),
        Safety::Unsafe,
        edits,
    ));
}

pub(crate) fn doc_class(path: &[Node<'_, '_>], written: &str) -> String {
    match written.strip_prefix('\\') {
        Some(fq) => fq.to_string(),
        None => imported(path, written).unwrap_or_else(|| match super::namespace_of(path) {
            Some(ns) => format!("{ns}\\{written}"),
            None => written.to_string(),
        }),
    }
}

pub(crate) fn object_name<'s>(ty: &TagType<'s>) -> Option<&'s str> {
    let name = match *ty {
        TagType::Name(n) => n,
        TagType::Other(o) => match o.split_once('<') {
            Some((n, _)) if is_name(n) && o.ends_with('>') => n,
            _ => return None,
        },
        _ => return None,
    };
    let keyword = matches!(
        name.to_ascii_lowercase().as_str(),
        "int"
            | "integer"
            | "string"
            | "bool"
            | "boolean"
            | "float"
            | "double"
            | "real"
            | "array"
            | "iterable"
            | "callable"
            | "callback"
            | "object"
            | "mixed"
            | "void"
            | "null"
            | "true"
            | "false"
            | "resource"
            | "never"
            | "self"
            | "static"
            | "parent"
            | "scalar"
            | "number"
            | "numeric"
            | "list"
            | "array-key"
            | "class-string"
    );
    (!keyword).then_some(name)
}

fn assigned_via_get_repository(set_up: &Method<'_>, name: &[u8]) -> bool {
    let mut stack = vec![Node::Method(set_up)];
    while let Some(node) = stack.pop() {
        if let Node::Assignment(a) = node
            && a.operator.is_assign()
            && let Expression::Access(mago_syntax::cst::Access::Property(fetch)) =
                unparenthesize(a.lhs)
            && matches!(unparenthesize(fetch.object), Expression::Variable(Variable::Direct(v)) if v.name == b"$this")
            && matches!(&fetch.property, ClassLikeMemberSelector::Identifier(id) if id.value == name)
            && matches!(unparenthesize(a.rhs), Expression::Call(Call::Method(call))
                if matches!(&call.method, ClassLikeMemberSelector::Identifier(m)
                    if m.value.eq_ignore_ascii_case(b"getRepository")))
        {
            return true;
        }
        node.visit_children(|c| stack.push(c));
    }
    false
}

fn assigned_via_container_get(set_up: &Method<'_>, name: &[u8]) -> bool {
    let mut stack = vec![Node::Method(set_up)];
    while let Some(node) = stack.pop() {
        if let Node::Assignment(a) = node
            && a.operator.is_assign()
            && let Expression::Access(mago_syntax::cst::Access::Property(fetch)) =
                unparenthesize(a.lhs)
            && is_this(fetch.object)
            && is_selector(&fetch.property, name)
            && is_container_get(a.rhs)
        {
            return true;
        }
        node.visit_children(|c| stack.push(c));
    }
    false
}

fn is_container_get(expr: &Expression<'_>) -> bool {
    let Expression::Call(Call::Method(call)) = unparenthesize(expr) else {
        return false;
    };
    if !is_selector(&call.method, b"get") {
        return false;
    }
    match unparenthesize(call.object) {
        Expression::Call(Call::StaticMethod(c)) => is_selector(&c.method, b"getContainer"),
        Expression::Call(Call::Method(c)) => is_selector(&c.method, b"getContainer"),
        Expression::Access(mago_syntax::cst::Access::Property(p)) => {
            is_selector(&p.property, b"container")
        }
        caller => is_this(caller),
    }
}

fn is_selector(selector: &ClassLikeMemberSelector<'_>, name: &[u8]) -> bool {
    matches!(selector, ClassLikeMemberSelector::Identifier(id) if id.value.eq_ignore_ascii_case(name))
}
