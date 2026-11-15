use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{
    AssignmentOperator, BinaryOperator, ClassLikeMemberSelector, Expression, IfBody, Literal,
    MethodBody, Modifier, Node, NodeKind, UnaryPrefixOperator, Variable,
};
use pun_problem::{Safety, Span};

use super::class_scope;
use super::facts::{Facts, facts};
use super::has_parent_class_method;
use super::param::{add_hint, is_mixed, param_declaration};
use crate::rules::support::class_members::qualified;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const TYPED_PROPERTIES: u32 = 70400;

const UNION_TYPES: u32 = 80000;

pub struct AddParamTypeProperty;

impl Rule for AddParamTypeProperty {
    fn id(&self) -> &'static str {
        "lint/types/add-param-type-property"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(TYPED_PROPERTIES)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Method(method) = node else { return };
        let MethodBody::Concrete(body) = &method.body else {
            return;
        };
        let facts = facts(ctx);
        let Some(class) = own_class(&facts, ancestors) else {
            return;
        };
        let mut edits = Vec::new();
        for param in &method.parameter_list.parameters {
            if param.hint.is_some() || param.ellipsis.is_some() {
                continue;
            }
            if !is_legal(Node::Block(body), param.variable.name) {
                continue;
            }
            let Some(types) =
                property_types(&facts, &class, Node::Block(body), param.variable.name)
            else {
                continue;
            };
            let Some(declaration) = declaration(&facts, &types, ctx.php_version.id()) else {
                continue;
            };
            let overrides = !method
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Private(_)))
                && class_scope(ancestors).is_some_and(|scope| {
                    has_parent_class_method(&facts, &scope, method.name.value)
                });
            if overrides {
                return;
            }
            edits.push(add_hint(param, &declaration));
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Add the type of the property a parameter is assigned to.",
            Safety::Unsafe,
            edits,
        ));
    }
}

fn own_class(facts: &Facts, ancestors: &[Node<'_, '_>]) -> Option<String> {
    let at = ancestors.iter().rposition(|a| {
        matches!(
            a,
            Node::Class(_) | Node::AnonymousClass(_) | Node::Trait(_) | Node::Enum(_)
        )
    })?;
    match ancestors[at] {
        Node::AnonymousClass(c) => Some(facts.anonymous_class_name(mago_span::HasSpan::span(c))),
        Node::Class(c) => Some(qualified(&ancestors[..at], c.name.value)),
        _ => None,
    }
}

enum PropertyType {
    Native(Option<TUnion>),
    Falsy,
    Null,
}

fn property_types(
    facts: &Facts,
    class: &str,
    body: Node<'_, '_>,
    name: &[u8],
) -> Option<Vec<PropertyType>> {
    let mut falsy: Vec<(Vec<u8>, Span)> = Vec::new();
    let mut null: Vec<(Vec<u8>, Span)> = Vec::new();
    walk(body, &mut |node| {
        let Node::If(i) = node else { return };
        let IfBody::Statement(s) = &i.body else {
            return;
        };
        match unparenthesize(i.condition) {
            Expression::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Not(_)) => {
                if let Some(property) = local_property(u.operand) {
                    falsy.push((property, node_span(s.statement)));
                }
            }
            Expression::Binary(b) if matches!(b.operator, BinaryOperator::Identical(_)) => {
                let checked = match (unparenthesize(b.lhs), unparenthesize(b.rhs)) {
                    (other, Expression::Literal(Literal::Null(_)))
                    | (Expression::Literal(Literal::Null(_)), other) => local_property(other),
                    _ => None,
                };
                if let Some(property) = checked {
                    null.push((property, node_span(s.statement)));
                }
            }
            _ => {}
        }
    });
    let mut types = Vec::new();
    walk(body, &mut |node| {
        let Node::Assignment(assign) = node else {
            return;
        };
        if !matches!(assign.operator, AssignmentOperator::Assign(_))
            || !matches!(unparenthesize(assign.rhs), Expression::Variable(Variable::Direct(v)) if v.name == name)
        {
            return;
        }
        let Some(property) = local_property(assign.lhs) else {
            return;
        };
        let declared = facts
            .codebase
            .get_declaring_property(class.as_bytes(), &property)
            .and_then(|p| p.type_declaration_metadata.as_ref());
        let at = node_span(assign);
        types.push(match declared {
            Some(t) => PropertyType::Native(Some(t.type_union.clone())),
            None if falsy
                .iter()
                .any(|(p, s)| *p == property && s.start <= at.start && at.end <= s.end) =>
            {
                PropertyType::Falsy
            }
            None if null
                .iter()
                .any(|(p, s)| *p == property && s.start <= at.start && at.end <= s.end) =>
            {
                PropertyType::Null
            }
            None => PropertyType::Native(None),
        });
    });
    (!types.is_empty()).then_some(types)
}

fn local_property(expr: &Expression<'_>) -> Option<Vec<u8>> {
    match unparenthesize(expr) {
        Expression::Access(mago_syntax::cst::Access::Property(fetch)) if matches!(unparenthesize(fetch.object), Expression::Variable(Variable::Direct(v)) if v.name == b"$this") => {
            match &fetch.property {
                ClassLikeMemberSelector::Identifier(id) => Some([b"$", id.value].concat()),
                _ => None,
            }
        }
        Expression::Access(mago_syntax::cst::Access::StaticProperty(fetch))
            if matches!(fetch.class, Expression::Self_(_) | Expression::Static(_)) =>
        {
            match &fetch.property {
                Variable::Direct(v) => Some(v.name.to_vec()),
                _ => None,
            }
        }
        _ => None,
    }
}

fn declaration(facts: &Facts, types: &[PropertyType], php_version: u32) -> Option<String> {
    if types.iter().all(|t| matches!(t, PropertyType::Falsy)) {
        return (php_version >= UNION_TYPES)
            .then(|| "array|bool|int|float|string|null".to_string());
    }
    let mut atomics: Vec<TAtomic> = Vec::new();
    for t in types {
        if matches!(t, PropertyType::Null) {
            if !atomics.contains(&TAtomic::Null) {
                atomics.push(TAtomic::Null);
            }
            continue;
        }
        let PropertyType::Native(Some(t)) = t else {
            return None;
        };
        for a in t.types.iter() {
            if !atomics.contains(a) {
                atomics.push(a.clone());
            }
        }
    }
    let union = TUnion::from_vec(atomics);
    if is_mixed(&union) {
        return None;
    }
    param_declaration(&union, &facts.codebase, php_version)
}

fn walk<'ast, 'arena>(node: Node<'ast, 'arena>, f: &mut dyn FnMut(Node<'ast, 'arena>)) {
    node.visit_children(|child| match child {
        Node::Class(_)
        | Node::AnonymousClass(_)
        | Node::Interface(_)
        | Node::Trait(_)
        | Node::Enum(_)
        | Node::Function(_) => {}
        _ => {
            f(child);
            walk(child, f);
        }
    });
}

pub(crate) fn is_legal(body: Node<'_, '_>, name: &[u8]) -> bool {
    fn has_variable(node: Node<'_, '_>, name: &[u8]) -> bool {
        crate::rules::support::side_effects::any_node(
            node,
            &|n| matches!(n, Node::Variable(Variable::Direct(v)) if v.name == name),
        )
    }
    !crate::rules::support::side_effects::any_node(body, &|node| match node {
        Node::Assignment(a) => {
            matches!(a.operator, AssignmentOperator::Assign(_))
                && !matches!(a.rhs, Expression::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Reference(_)))
                && matches!(unparenthesize(a.lhs), Expression::Variable(Variable::Direct(v)) if v.name == name)
        }
        Node::If(i) => has_variable(Node::Expression(i.condition), name),
        Node::Conditional(_) => has_variable(node, name),
        _ => false,
    })
}
