//! Rector 2.8.4's `AssignArrayToStringRector`.

use mago_syntax::cst::{
    ClassLikeMember, ClassLikeMemberSelector, Expression, Literal, Node, NodeKind, Property,
    PropertyItem, Sequence, Statement, Variable,
};
use pun_phpstan::types::Type;

use crate::problem::{Edit, Safety, Span};
use crate::rules::types::compare::mapper::expr_type;
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{is_this, node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoAppendToString;

const MESSAGE: &str =
    "Appending with `[]` to a string is an error since PHP 7.1; start it as an empty array.";

impl Rule for NoAppendToString {
    fn id(&self) -> &'static str {
        "lint/php-7.1/no-append-to-string"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70100)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::Program,
            NodeKind::Namespace,
            NodeKind::Class,
            NodeKind::AnonymousClass,
            NodeKind::Method,
            NodeKind::Function,
            NodeKind::Closure,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let stmts: Vec<Node<'ast, 'arena>> = match node {
            Node::Class(class) => return refactor_class(&class.members, out),
            Node::AnonymousClass(class) => return refactor_class(&class.members, out),
            Node::Program(program) => {
                // handled in Namespace_
                if program
                    .statements
                    .iter()
                    .any(|s| matches!(s, Statement::Namespace(_)))
                {
                    return;
                }
                program.statements.iter().map(Node::Statement).collect()
            }
            Node::Namespace(namespace) => {
                namespace.statements().iter().map(Node::Statement).collect()
            }
            Node::Method(method) => match &method.body {
                mago_syntax::cst::MethodBody::Concrete(block) => vec![Node::Block(block)],
                mago_syntax::cst::MethodBody::Abstract(_) => return,
            },
            Node::Function(function) => vec![Node::Block(&function.body)],
            Node::Closure(closure) => vec![Node::Block(&closure.body)],
            _ => return,
        };
        let facts = facts(ctx);
        let mut assigns = Vec::new();
        for stmt in &stmts {
            empty_string_assigns(*stmt, &mut assigns);
        }
        for (variable, rhs) in assigns {
            if refactor_assign(&facts, ancestors, &stmts, variable) {
                out.push(Report::fixed(
                    node_span(rhs),
                    MESSAGE,
                    Safety::Unsafe,
                    vec![Edit::new(node_span(rhs), "[]")],
                ));
            }
        }
    }
}

fn is_empty_string(expr: &Expression<'_>) -> bool {
    matches!(expr, Expression::Literal(Literal::String(s)) if s.value == Some(b""))
}

/// The `$variable = ''` assigns of the scope, outside the classes, functions and closures in it.
fn empty_string_assigns<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    out: &mut Vec<(&'ast Expression<'arena>, &'ast Expression<'arena>)>,
) {
    match node {
        Node::Class(_) | Node::AnonymousClass(_) | Node::Function(_) | Node::Closure(_) => return,
        Node::Assignment(assign)
            if assign.operator.is_assign()
                && matches!(assign.lhs, Expression::Variable(Variable::Direct(_)))
                && is_empty_string(assign.rhs) =>
        {
            out.push((assign.lhs, assign.rhs));
        }
        _ => {}
    }
    for child in node.children() {
        empty_string_assigns(child, out);
    }
}

/// Each assign of the scope, in Rector's traversal order; `f` returns `true` to stop.
fn each_assign<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    f: &mut dyn FnMut(&'ast mago_syntax::cst::Assignment<'arena>) -> bool,
) -> bool {
    if let Node::Assignment(assign) = node
        && assign.operator.is_assign()
        && f(assign)
    {
        return true;
    }
    node.children()
        .into_iter()
        .any(|child| each_assign(child, f))
}

fn named(expr: &Expression<'_>, name: &[u8]) -> bool {
    matches!(unparenthesize(expr), Expression::Variable(Variable::Direct(v)) if v.name == name)
}

fn refactor_assign(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    stmts: &[Node<'_, '_>],
    variable: &Expression<'_>,
) -> bool {
    let Expression::Variable(Variable::Direct(direct)) = variable else {
        return false;
    };
    let name = direct.name;
    let at = node_span(variable).start;
    let native = expr_type(facts, ancestors, variable, true);
    if native.is_array().yes() || matches!(native, Type::Union(_)) {
        return false;
    }
    let after = |e: &Expression<'_>| named(e, name) && node_span(e).start > at;
    // `isReAssignedAsString`.
    let mut reassigned_as_string = false;
    for stmt in stmts {
        each_assign(*stmt, &mut |assign| {
            reassigned_as_string = after(assign.lhs)
                && !is_empty_string(assign.rhs)
                && expr_type(facts, ancestors, assign.rhs, true)
                    .is_string()
                    .yes();
            reassigned_as_string
        });
        if reassigned_as_string {
            return false;
        }
    }
    // `findSameNamedVariableAssigns`: an append (`$variable[] = ...`) the variable is not re-assigned an array
    // anywhere after.
    let mut append = false;
    let mut reassigned_as_array = false;
    for stmt in stmts {
        each_assign(*stmt, &mut |assign| {
            if after(assign.lhs)
                && expr_type(facts, ancestors, assign.rhs, true)
                    .is_array()
                    .yes()
            {
                reassigned_as_array = true;
                return true;
            }
            if let Expression::ArrayAppend(a) = assign.lhs
                && named(a.array, name)
            {
                append = true;
            }
            false
        });
        if reassigned_as_array {
            return false;
        }
    }
    append
}

/// `refactorClass`: a property whose default is `''` and which a method or property hook appends to.
fn refactor_class(members: &Sequence<'_, ClassLikeMember<'_>>, out: &mut Vec<Report>) {
    for member in members {
        let ClassLikeMember::Property(Property::Plain(property)) = member else {
            continue;
        };
        let Some(PropertyItem::Concrete(item)) = property.items.first() else {
            continue;
        };
        if !is_empty_string(item.value) {
            continue;
        }
        let name = &item.variable.name[1..];
        let appended = members.iter().any(|m| {
            let node = match m {
                ClassLikeMember::Method(method) => Node::Method(method),
                ClassLikeMember::Property(p @ Property::Hooked(_)) => Node::Property(p),
                _ => return false,
            };
            each_assign(node, &mut |assign| {
                matches!(assign.lhs, Expression::ArrayAppend(a) if is_local_property(a.array, name))
            })
        });
        if appended {
            let span: Span = node_span(item.value);
            out.push(Report::fixed(
                span,
                MESSAGE,
                Safety::Unsafe,
                vec![Edit::new(span, "[]")],
            ));
        }
    }
}

/// `PropertyFetchAnalyzer::isLocalPropertyFetchName`: `$this->name`, `self::$name` or `static::$name`.
fn is_local_property(expr: &Expression<'_>, name: &[u8]) -> bool {
    match unparenthesize(expr) {
        Expression::Access(mago_syntax::cst::Access::Property(fetch)) => {
            is_this(fetch.object)
                && matches!(&fetch.property, ClassLikeMemberSelector::Identifier(id) if id.value == name)
        }
        Expression::Access(mago_syntax::cst::Access::NullSafeProperty(fetch)) => {
            is_this(fetch.object)
                && matches!(&fetch.property, ClassLikeMemberSelector::Identifier(id) if id.value == name)
        }
        Expression::Access(mago_syntax::cst::Access::StaticProperty(fetch)) => {
            matches!(fetch.class, Expression::Self_(_) | Expression::Static(_))
                && matches!(&fetch.property, Variable::Direct(v) if &v.name[1..] == name)
        }
        _ => false,
    }
}
