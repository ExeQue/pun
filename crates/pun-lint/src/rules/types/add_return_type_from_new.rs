use crate::problem::Span;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{
    AnonymousClass, Expression, Node, NodeKind, Return, Statement, UnaryPostfix, UnaryPrefix,
    Variable,
};

use super::facts::{Facts, facts};
use super::native::native_type;
use super::silent_void::Place;
use super::{
    SCALAR_TYPES, add_return_type, ancestor_names, any_scoped, class_scope,
    has_only_return_with_expr, returns_scoped, untyped,
};
use crate::rules::types::names::resolve_class_name;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct AddReturnTypeFromNew;

const OBJECT_TYPE: u32 = 70200;
const PHP_80: u32 = 80000;

#[derive(PartialEq)]
enum NewType {
    Class(String),
    SelfType,
    Static,
    Anonymous(Vec<String>),
}

impl Rule for AddReturnTypeFromNew {
    fn id(&self) -> &'static str {
        "lint/types/add-return-type-from-new"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(SCALAR_TYPES)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Function, NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(f) = untyped(node, ancestors, ctx) else {
            return;
        };
        let returns = returns_scoped(f.body);
        let scope = class_scope(ancestors);
        let facts = facts(ctx);
        let place = Place {
            facts: &facts,
            ancestors,
            scope: scope.as_ref(),
        };
        if !has_only_return_with_expr(&place, f.body, &returns) {
            return;
        }
        let r#type = match always_returned_variable_new(&facts, f.body, &returns, ancestors) {
            Some(r#type) => Some(r#type),
            None => direct_return_new(
                &facts,
                ancestors,
                &returns,
                ctx.php_version.id(),
                matches!(node, Node::Method(_)),
            ),
        };
        if let Some(r#type) = r#type {
            out.push(add_return_type(f.span, f.at, &r#type));
        }
    }
}

fn new_type(facts: &Facts, ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> Option<NewType> {
    match expr {
        Expression::AnonymousClass(class) => Some(anonymous(ancestors, class)),
        Expression::Instantiation(new) => match new.class {
            Expression::Self_(_) => Some(NewType::SelfType),
            Expression::Static(_) => Some(NewType::Static),
            Expression::Identifier(id) => {
                let class = resolve_class_name(ancestors, id);
                facts
                    .codebase
                    .get_class_like(class.as_bytes())
                    .is_some()
                    .then_some(NewType::Class(class))
            }
            _ => None,
        },
        _ => None,
    }
}

fn anonymous(ancestors: &[Node<'_, '_>], class: &AnonymousClass<'_>) -> NewType {
    NewType::Anonymous(anonymous_parents(ancestors, class))
}

pub(super) fn anonymous_parents(
    ancestors: &[Node<'_, '_>],
    class: &AnonymousClass<'_>,
) -> Vec<String> {
    class
        .extends
        .iter()
        .flat_map(|e| e.types.iter())
        .chain(class.implements.iter().flat_map(|i| i.types.iter()))
        .map(|id| resolve_class_name(ancestors, id))
        .collect()
}

fn direct_return_new(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    returns: &[&Return<'_>],
    php_version: u32,
    is_method: bool,
) -> Option<String> {
    let mut types: Vec<NewType> = Vec::new();
    for r in returns {
        let mut t = new_type(facts, ancestors, r.value?)?;
        if let NewType::Anonymous(parents) = &t
            && returns.len() > 1
            && let Some(first) = parents.first()
        {
            t = NewType::Class(first.clone());
        }
        if !types.contains(&t) {
            types.push(t);
        }
    }
    if let [one] = types.as_slice() {
        return match one {
            NewType::Class(class) => (!(is_method
                && is_response_inside_controller(facts, ancestors, class)))
            .then(|| format!("\\{class}")),
            NewType::SelfType => Some("self".to_string()),
            NewType::Static => Some(
                if php_version >= PHP_80 {
                    "static"
                } else {
                    "self"
                }
                .to_string(),
            ),
            NewType::Anonymous(parents) => match parents.as_slice() {
                [parent] => Some(format!("\\{parent}")),
                _ => (php_version >= OBJECT_TYPE).then(|| "object".to_string()),
            },
        };
    }
    if php_version < PHP_80 {
        return None;
    }
    if types.iter().any(|t| matches!(t, NewType::Anonymous(_))) {
        return Some("object".to_string());
    }
    let classes: Option<Vec<String>> = types
        .iter()
        .map(|t| match t {
            NewType::Class(class) => Some(format!("\\{class}")),
            NewType::SelfType => Some("self".to_string()),
            NewType::Static => Some("static".to_string()),
            NewType::Anonymous(_) => None,
        })
        .collect();
    Some(classes?.join("|"))
}

fn is_response_inside_controller(facts: &Facts, ancestors: &[Node<'_, '_>], class: &str) -> bool {
    facts.codebase.is_instance_of(
        class.to_ascii_lowercase().as_bytes(),
        b"symfony\\component\\httpfoundation\\response",
    ) && class_scope(ancestors).is_some_and(|scope| {
        ancestor_names(facts, &scope).iter().any(|a| {
            a.eq_ignore_ascii_case("Symfony\\Bundle\\FrameworkBundle\\Controller\\Controller")
                || a.eq_ignore_ascii_case(
                    "Symfony\\Bundle\\FrameworkBundle\\Controller\\AbstractController",
                )
        })
    })
}

fn always_returned_variable_new(
    facts: &Facts,
    body: &mago_syntax::cst::Block<'_>,
    returns: &[&Return<'_>],
    ancestors: &[Node<'_, '_>],
) -> Option<String> {
    let created = created_variables_to_types(body, ancestors);
    let mut classes: Vec<String> = Vec::new();
    for r in returns {
        let value = r.value?;
        let Expression::Variable(Variable::Direct(v)) = value else {
            return None;
        };
        let class = match native_type(facts, value)?.types.as_ref() {
            [TAtomic::Object(TObject::Any)] => "object".to_string(),
            [TAtomic::Object(TObject::Named(named))]
                if !named.is_this && named.name.as_bytes().starts_with(b"{anonymous-class:") =>
            {
                "object".to_string()
            }
            [TAtomic::Object(TObject::Named(named))] if !named.is_this && !named.is_static => {
                let (_, class) = created.iter().rev().find(|(name, _)| *name == v.name)?;
                if !class.as_bytes().eq_ignore_ascii_case(named.name.as_bytes())
                    || is_written_elsewhere(body, v.name)
                {
                    return None;
                }
                format!("\\{class}")
            }
            _ => return None,
        };
        if !classes.contains(&class) {
            classes.push(class);
        }
    }
    match classes.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

fn created_variables_to_types<'a>(
    body: &'a mago_syntax::cst::Block<'a>,
    ancestors: &[Node<'_, '_>],
) -> Vec<(&'a [u8], String)> {
    let mut created: Vec<(&[u8], String)> = Vec::new();
    for statement in &body.statements {
        let Statement::Expression(e) = statement else {
            continue;
        };
        let Expression::Assignment(assign) = e.expression else {
            continue;
        };
        if !assign.operator.is_assign() {
            continue;
        }
        let Expression::Variable(Variable::Direct(v)) = assign.lhs else {
            continue;
        };
        let class = match assign.rhs {
            Expression::Instantiation(new) => match new.class {
                Expression::Identifier(id) => resolve_class_name(ancestors, id),
                Expression::Self_(_) => "self".to_string(),
                Expression::Static(_) => "static".to_string(),
                _ => continue,
            },
            Expression::AnonymousClass(_) => continue,
            _ => {
                created.retain(|(name, _)| *name != v.name);
                continue;
            }
        };
        created.retain(|(name, _)| *name != v.name);
        created.push((v.name, class));
    }
    created
}

fn written_variable(target: &Expression<'_>) -> Option<Span> {
    match target {
        Expression::Variable(Variable::Direct(v)) => Some(node_span(v)),
        Expression::ArrayAccess(a) => written_variable(a.array),
        Expression::ArrayAppend(a) => written_variable(a.array),
        _ => None,
    }
}

fn is_written_elsewhere(body: &mago_syntax::cst::Block<'_>, name: &[u8]) -> bool {
    let allowed: Vec<Span> = body
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Statement::Expression(e) => match e.expression {
                Expression::Assignment(a) if a.operator.is_assign() => {
                    written_variable(a.lhs).filter(|_| matches!(a.lhs, Expression::Variable(_)))
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    let mut exact: Vec<Span> = Vec::new();
    let mut within: Vec<Span> = Vec::new();
    let mut uses: Vec<Span> = Vec::new();
    any_scoped(Node::Block(body), &mut |n| {
        match n {
            Node::Assignment(a) => match a.lhs {
                Expression::Array(_) | Expression::List(_) => within.push(node_span(a.lhs)),
                lhs => exact.extend(written_variable(lhs)),
            },
            Node::Foreach(f) => within.push(node_span(&f.target)),
            Node::UnaryPrefix(UnaryPrefix { operator, operand })
                if operator.is_increment_or_decrement() =>
            {
                exact.extend(written_variable(operand));
            }
            Node::UnaryPostfix(UnaryPostfix { operand, .. }) => {
                exact.extend(written_variable(operand));
            }
            Node::Unset(u) => exact.extend(u.values.iter().filter_map(|v| written_variable(v))),
            Node::Global(g) => within.push(node_span(g)),
            Node::Static(s) => within.push(node_span(s)),
            Node::DirectVariable(v) if v.name == name => uses.push(node_span(v)),
            _ => {}
        }
        false
    });
    uses.iter().any(|u| {
        !allowed.contains(u)
            && (exact.contains(u) || within.iter().any(|r| r.start <= u.start && u.end <= r.end))
    })
}
