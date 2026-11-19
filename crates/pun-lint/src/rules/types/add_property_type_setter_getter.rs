use mago_syntax::cst::{
    Access, ClassLikeMember, ClassLikeMemberSelector, Expression, Hint, Method, MethodBody, Node,
    NodeKind, Statement, Variable,
};
use pun_problem::{Edit, Safety, Span};

use super::facts::facts;
use super::property_type::{
    ClassView, Prop, TYPED_PROPERTIES, Ty, default_span, default_ty, hint_ty, is_legal,
    is_null_literal, property_declaration, variable_span,
};
use crate::syntax::unparenthesize;
use crate::{Context, Report, Rule};

pub struct AddPropertyTypeSetterGetter;

impl Rule for AddPropertyTypeSetterGetter {
    fn id(&self) -> &'static str {
        "lint/types/add-property-type-setter-getter"
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
        let facts = facts(ctx);
        let Some(view) = ClassView::of(&facts, node, ancestors) else {
            return;
        };
        let methods: Vec<&Method<'_>> = view
            .members
            .iter()
            .filter_map(|m| match m {
                ClassLikeMember::Method(m) => Some(m),
                _ => None,
            })
            .collect();
        let mut edits: Vec<Edit> = Vec::new();
        for prop in &view.properties() {
            if !prop.is_private() || prop.item.is_none() {
                continue;
            }
            let name = prop.name();
            if view
                .method(b"__construct")
                .is_some_and(|c| constructor_assigns(c, name))
            {
                continue;
            }
            let Some(t) = getter_setter_type(&view, &methods, name) else {
                continue;
            };
            let default_null = prop.default().is_some_and(is_null_literal);
            if !default_null
                && let Some(default) = prop.default()
                && !default_compatible(&default_ty(&facts, default), &t)
            {
                continue;
            }
            if !is_legal(ctx, &facts, &view, prop, false) {
                continue;
            }
            let Some(declaration) = property_declaration(&t, &facts.codebase, ctx.php_version.id())
            else {
                continue;
            };
            let at = variable_span(prop);
            edits.push(Edit::new(
                Span::new(at.start, at.start),
                format!("{declaration} "),
            ));
            edits.extend(default_edit(prop, &t, default_null, at.end));
        }
        let Some(first) = edits.first() else { return };
        out.push(Report::fixed(
            first.span,
            "Type the properties from their getter and setter.",
            Safety::Unsafe,
            edits,
        ));
    }
}

fn constructor_assigns(constructor: &Method<'_>, name: &[u8]) -> bool {
    fn walk(node: Node<'_, '_>, name: &[u8]) -> bool {
        if let Node::Assignment(a) = node
            && a.operator.is_assign()
            && !super::property_type::is_by_ref(a)
            && is_this_fetch(a.lhs, name)
        {
            return true;
        }
        let mut found = false;
        node.visit_children(|c| found = found || walk(c, name));
        found
    }
    walk(Node::Method(constructor), name)
}

fn is_this_fetch(expr: &Expression<'_>, name: &[u8]) -> bool {
    matches!(unparenthesize(expr), Expression::Access(Access::Property(p))
        if matches!(unparenthesize(p.object), Expression::Variable(Variable::Direct(v)) if v.name == b"$this")
            && matches!(&p.property, ClassLikeMemberSelector::Identifier(id) if id.value == name))
}

fn statements<'a>(method: &'a Method<'a>) -> Vec<&'a Statement<'a>> {
    match &method.body {
        MethodBody::Concrete(block) => block.statements.iter().collect(),
        MethodBody::Abstract(_) => Vec::new(),
    }
}

fn getter_setter_type(
    view: &ClassView<'_, '_>,
    methods: &[&Method<'_>],
    name: &[u8],
) -> Option<Ty> {
    let getter = getter_type(view, methods, name)?;
    let setter = setter_type(view, methods, name)?;
    if setter == getter {
        return Some(setter);
    }
    let mut members: Vec<Ty> = setter.members().into_iter().cloned().collect();
    members.extend(getter.members().into_iter().cloned());
    Some(Ty::Union(members))
}

fn declared(view: &ClassView<'_, '_>, hint: &Hint<'_>) -> Option<Ty> {
    match hint_ty(hint, &view.path, view) {
        Ty::Mixed => None,
        t if t.members().iter().all(|m| matches!(m, Ty::Array(..))) => Some(Ty::Mixed),
        t => Some(t),
    }
}

fn getter_type(view: &ClassView<'_, '_>, methods: &[&Method<'_>], name: &[u8]) -> Option<Ty> {
    for method in methods {
        let [Statement::Return(r)] = statements(method)[..] else {
            continue;
        };
        let Some(Expression::Access(Access::Property(p))) = r.value.map(unparenthesize) else {
            continue;
        };
        if !matches!(&p.property, ClassLikeMemberSelector::Identifier(id) if id.value == name) {
            continue;
        }
        if let Some(t) = method
            .return_type_hint
            .as_ref()
            .and_then(|h| declared(view, &h.hint))
        {
            return Some(t);
        }
    }
    None
}

fn setter_type(view: &ClassView<'_, '_>, methods: &[&Method<'_>], name: &[u8]) -> Option<Ty> {
    for method in methods {
        let [Statement::Expression(e)] = statements(method)[..] else {
            continue;
        };
        let Expression::Assignment(a) = unparenthesize(e.expression) else {
            continue;
        };
        if !a.operator.is_assign()
            || super::property_type::is_by_ref(a)
            || !is_this_fetch(a.lhs, name)
        {
            continue;
        }
        let hint = method
            .parameter_list
            .parameters
            .first()
            .and_then(|p| p.hint.as_ref())?;
        if let Some(t) = declared(view, hint) {
            return Some(t);
        }
    }
    None
}

fn default_compatible(default: &Ty, t: &Ty) -> bool {
    matches!(
        (default, t),
        (Ty::Float(_), Ty::Float(_)) | (Ty::Int(_), Ty::Int(_)) | (Ty::Str(_), Ty::Str(_))
    ) || default == t
}

fn default_edit(
    prop: &Prop<'_, '_>,
    t: &Ty,
    default_null: bool,
    variable_end: usize,
) -> Option<Edit> {
    let insert = |text: &str| {
        Some(Edit::new(
            Span::new(variable_end, variable_end),
            text.to_string(),
        ))
    };
    if t.members().contains(&&Ty::Null) {
        return match prop.default() {
            Some(_) => None,
            None => insert(" = null"),
        };
    }
    if default_null {
        let span = default_span(prop)?;
        return Some(Edit::new(
            span,
            if matches!(t, Ty::Str(_)) { " = ''" } else { "" },
        ));
    }
    match (t, prop.default()) {
        (Ty::Float(_), None) => insert(" = 0.0"),
        (Ty::Int(_), None) => insert(" = 0"),
        _ => None,
    }
}
