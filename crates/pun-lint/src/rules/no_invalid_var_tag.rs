use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::array::TArray;
use mago_codex::ttype::atomic::array::key::ArrayKey;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::atomic::scalar::int::TInteger;
use mago_codex::ttype::union::TUnion;
use mago_phpdoc_syntax::cst::TagValue;
use mago_phpdoc_syntax::cst::r#type::{ReferenceKind, Type};
use mago_syntax::cst::{
    Argument, AssignmentOperator, Call, Expression, Node, NodeKind, Statement, StaticItem,
};
use pun_problem::{Edit, Fix, Safety, Span};

use super::class_method::{object_class_names, type_of};
use super::lint_b_doc::Doc;
use super::php_parser::{List, enclosing, holder_list, previous_token_end, tokens};
use super::phpdoc_tag::{is_valid, named, with_tag};
use super::rector_batch5::{called_function_name, resolve_class_name};
use super::rector_conditions::comments_between;
use super::types::facts::{Facts, facts};
use super::used_variable::{same_name, try_of, used_in_next};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoInvalidVarTag;

impl Rule for NoInvalidVarTag {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-invalid-var-tag"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::Statement,
            NodeKind::Block,
            NodeKind::Program,
            NodeKind::NamespaceImplicitBody,
            NodeKind::SwitchExpressionCase,
            NodeKind::SwitchDefaultCase,
            NodeKind::IfColonDelimitedBody,
            NodeKind::IfColonDelimitedBodyElseIfClause,
            NodeKind::IfColonDelimitedBodyElseClause,
            NodeKind::WhileColonDelimitedBody,
            NodeKind::ForColonDelimitedBody,
            NodeKind::ForeachColonDelimitedBody,
            NodeKind::DeclareColonDelimitedBody,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let source = ctx.source;
        let tokens = tokens(ctx);
        let mut chain = ancestors.to_vec();
        chain.push(node);
        let (list, at, holder) = match node {
            Node::Statement(statement) => {
                if !matches!(
                    statement,
                    Statement::Foreach(_)
                        | Statement::Static(_)
                        | Statement::Echo(_)
                        | Statement::EchoTag(_)
                        | Statement::Return(_)
                        | Statement::Expression(_)
                        | Statement::If(_)
                        | Statement::While(_)
                        | Statement::Switch(_)
                ) || ancestors.is_empty()
                {
                    return;
                }
                let (list, index) = enclosing(source, tokens, &chain);
                (list, At::Statement(index), chain.len() - 2)
            }
            _ => {
                if !is_list_holder(node) {
                    return;
                }
                let list = holder_list(source, tokens, node);
                if !list.nop {
                    return;
                }
                (list, At::Nop, chain.len() - 1)
            }
        };
        let facts = facts(ctx);
        let Some(defined) = defined_before(&facts, &chain[..=holder], &list, at) else {
            return;
        };
        let index = match at {
            At::Statement(i) => i,
            At::Nop => list.stmts.len(),
        };
        let try_ = match at {
            At::Statement(_) => try_of(&chain),
            At::Nop => {
                let mut chain = chain.clone();
                chain.push(node);
                try_of(&chain)
            }
        };
        let next = (index + 1).min(list.stmts.len());
        if defined
            .iter()
            .any(|name| used_in_next(&list, next, try_, name.as_bytes()))
            || list
                .stmts
                .get(next)
                .is_some_and(|s| is_inline_html(source, s))
        {
            return;
        }
        let (start, comments_from) = match at {
            At::Statement(i) => {
                let start = node_span(list.stmts[i]).start;
                (
                    start,
                    previous_token_end(source, tokens, start).max(list.inner.start),
                )
            }
            At::Nop => (
                list.inner.end,
                list.stmts
                    .last()
                    .map_or(list.inner.start, |s| node_span(*s).end),
            ),
        };
        if comments_between(tokens, comments_from, start).len() != 1 {
            return;
        }
        let doc_ancestors = match at {
            At::Statement(_) => ancestors,
            At::Nop => &chain[..],
        };
        let Some(doc) = Doc::of(ctx, doc_ancestors, start) else {
            return;
        };
        let holder_comments = holder_span(&chain[..=holder]).map_or(0, |span| {
            comments_between(
                tokens,
                previous_token_end(source, tokens, span.start),
                span.start,
            )
            .len()
        });
        let statement = match at {
            At::Statement(i) => Some(list.stmts[i]),
            At::Nop => None,
        };
        let tags: Vec<Option<VarTag>> = (0..doc.len()).map(|i| var_tag(doc.text(i))).collect();
        let mut removed = vec![false; tags.len()];
        for tag in tags.iter().flatten() {
            if tag.object_shape {
                continue;
            }
            let name = tag.variable.as_deref().unwrap_or("");
            if name.is_empty() && statement.is_some_and(|s| allowed_unnamed(ancestors, s, tag)) {
                continue;
            }
            if statement.is_some_and(|s| has_variable(Node::Statement(s), None, name)) {
                continue;
            }
            if holder_comments > 1 {
                continue;
            }
            if used_in_next(&list, next, try_, name.as_bytes()) {
                continue;
            }
            for (i, other) in tags.iter().enumerate() {
                if let Some(other) = other
                    && (name.is_empty() || other.variable.as_deref() == Some(name))
                {
                    removed[i] = true;
                }
            }
        }
        if !removed.contains(&true) {
            return;
        }
        let printed = doc.print(&removed);
        let edit = if statement.is_some() && !printed.is_empty() {
            Edit::new(doc.span, printed)
        } else if !printed.is_empty() {
            Edit::new(doc.span, format!("{printed}\n"))
        } else if statement.is_some() {
            doc.remove(&removed, start)
        } else {
            let line = source[..doc.span.start].rfind('\n').map_or(0, |i| i + 1);
            let from = if source[line..doc.span.start].trim().is_empty() {
                line
            } else {
                doc.span.start
            };
            Edit::new(Span::new(from, doc.span.end), "")
        };
        out.push(Report {
            span: doc.span,
            message: "Remove the `@var` tag of a variable the statement does not have.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![edit],
            }),
        });
    }
}

#[derive(Clone, Copy)]
enum At {
    Statement(usize),
    Nop,
}

struct VarTag {
    variable: Option<String>,
    object_shape: bool,
    generic: Option<String>,
}

fn var_tag(text: &str) -> Option<VarTag> {
    with_tag(text, |tag| {
        if !named(tag, "var") || !is_valid(tag) {
            return None;
        }
        let TagValue::Var(value) = tag.value else {
            return None;
        };
        let description = value.description.map(|d| {
            let span = d.span;
            let content = format!("/** {text} */");
            content[span.start.offset as usize..span.end.offset as usize].to_string()
        });
        let object_shape = matches!(value.r#type, Type::Object(o) if o.properties.is_none())
            && description
                .as_deref()
                .is_some_and(|d| d.starts_with('{') && d.contains('}'));
        let generic = match value.r#type {
            Type::Reference(r) if r.parameters.is_some() => Some(match r.kind {
                ReferenceKind::Identifier(id) => String::from_utf8_lossy(id.value).into_owned(),
                ReferenceKind::Self_(k) | ReferenceKind::Static(k) | ReferenceKind::Parent(k) => {
                    String::from_utf8_lossy(k.value).into_owned()
                }
            }),
            _ => None,
        };
        Some(VarTag {
            variable: value
                .variable
                .map(|v| String::from_utf8_lossy(&v.value[1..]).into_owned()),
            object_shape,
            generic,
        })
    })
    .flatten()
}

fn is_inline_html(source: &str, statement: &Statement<'_>) -> bool {
    let Statement::Inline(inline) = statement else {
        return false;
    };
    let start = inline.span.start.offset as usize;
    let value = &source[start..inline.span.end.offset as usize];
    !source[..start].ends_with("?>")
        || !value
            .strip_prefix("\r\n")
            .or_else(|| value.strip_prefix('\n'))
            .is_some_and(str::is_empty)
}

fn allowed_unnamed(ancestors: &[Node<'_, '_>], statement: &Statement<'_>, tag: &VarTag) -> bool {
    match statement {
        Statement::Return(r) => match r.value.map(unparenthesize) {
            Some(Expression::Call(_)) => true,
            Some(Expression::Instantiation(new)) => {
                let class = match unparenthesize(new.class) {
                    Expression::Self_(_) => Some("self".to_string()),
                    Expression::Static(_) => Some("static".to_string()),
                    Expression::Parent(_) => Some("parent".to_string()),
                    Expression::Identifier(id) => Some(resolve_class_name(ancestors, id)),
                    _ => None,
                };
                matches!((&tag.generic, class), (Some(g), Some(c)) if g.eq_ignore_ascii_case(&c))
            }
            _ => false,
        },
        Statement::Expression(e) => matches!(
            unparenthesize(e.expression),
            Expression::Assignment(a)
                if matches!(a.operator, AssignmentOperator::Assign(_))
                    && matches!(unparenthesize(a.lhs), Expression::Variable(_))
        ),
        _ => false,
    }
}

fn has_variable(node: Node<'_, '_>, parent: Option<Node<'_, '_>>, name: &str) -> bool {
    if let Node::DirectVariable(v) = node {
        return !matches!(
            parent,
            Some(Node::PropertyAbstractItem(_) | Node::PropertyConcreteItem(_))
        ) && same_name(name.as_bytes(), &v.name[1..]);
    }
    let mut found = false;
    node.visit_children(|child| found = found || has_variable(child, Some(node), name));
    found
}

fn defined_before(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    list: &List<'_, '_>,
    at: At,
) -> Option<Vec<String>> {
    let last = match at {
        At::Statement(i) => i + 1,
        At::Nop => list.stmts.len(),
    };
    let mut defined = Vec::new();
    for (k, statement) in list.stmts[..last].iter().enumerate() {
        if let Statement::Expression(e) = statement
            && let Expression::Call(Call::Function(call)) = e.expression
            && called_function_name(ancestors, call.function)
                .is_some_and(|n| n.eq_ignore_ascii_case("extract"))
            && !call.argument_list.arguments.is_empty()
        {
            let value = match call.argument_list.arguments.iter().next()? {
                Argument::Positional(a) => a.value,
                Argument::Named(a) => a.value,
            };
            defined.extend(constant_array_keys(facts, value)?);
            if matches!(at, At::Statement(i) if i == k) {
                return None;
            }
            continue;
        }
        if let Statement::Static(s) = statement {
            for item in &s.items {
                let variable = match item {
                    StaticItem::Abstract(i) => &i.variable,
                    StaticItem::Concrete(i) => &i.variable,
                };
                defined.push(String::from_utf8_lossy(&variable.name[1..]).into_owned());
            }
        }
    }
    Some(defined)
}

fn constant_array_keys(facts: &Facts, expr: &Expression<'_>) -> Option<Vec<String>> {
    let t = type_of(facts, expr)?;
    let [atomic] = &t.types[..] else { return None };
    constant_array(atomic)
}

fn constant_array(atomic: &TAtomic) -> Option<Vec<String>> {
    let items: Vec<(String, &TUnion)> = match atomic {
        TAtomic::Array(TArray::Keyed(k)) if k.parameters.is_none() => k
            .known_items
            .as_ref()?
            .iter()
            .map(|(key, (_, value))| {
                let key = match key {
                    ArrayKey::Integer(i) => i.to_string(),
                    ArrayKey::String(s) => String::from_utf8_lossy(s.as_bytes()).into_owned(),
                    _ => return None,
                };
                Some((key, value))
            })
            .collect::<Option<_>>()?,
        TAtomic::Array(TArray::List(l)) if l.element_type.is_never() => l
            .known_elements
            .as_ref()?
            .iter()
            .map(|(i, (_, value))| (i.to_string(), value))
            .collect(),
        _ => return None,
    };
    let mut keys = Vec::new();
    for (key, value) in items {
        let [value_atomic] = &value.types[..] else {
            return None;
        };
        let constant = match value_atomic {
            TAtomic::Null => true,
            TAtomic::Scalar(TScalar::Bool(b)) => b.value.is_some(),
            TAtomic::Scalar(TScalar::Integer(TInteger::Literal(_))) => true,
            TAtomic::Scalar(TScalar::Float(f)) => f.get_literal_value().is_some(),
            TAtomic::Scalar(s @ TScalar::String(_)) => s.get_known_literal_string_value().is_some(),
            TAtomic::Array(_) => constant_array(value_atomic).is_some(),
            _ => false,
        };
        if constant {
            keys.push(key);
        } else if object_class_names(value).len() != 1 {
            return None;
        }
    }
    Some(keys)
}

fn holder_span(ancestors: &[Node<'_, '_>]) -> Option<pun_problem::Span> {
    let mut i = ancestors.len() - 1;
    loop {
        let node = ancestors[i];
        match node {
            Node::Program(_) => return None,
            Node::Statement(Statement::Block(_)) if i > 0 && is_list_holder(ancestors[i - 1]) => {
                return Some(node_span(&node));
            }
            Node::Block(_)
            | Node::Statement(_)
            | Node::MethodBody(_)
            | Node::ForeachBody(_)
            | Node::WhileBody(_)
            | Node::ForBody(_)
            | Node::DeclareBody(_)
            | Node::IfBody(_)
            | Node::IfStatementBody(_)
            | Node::IfColonDelimitedBody(_)
            | Node::WhileColonDelimitedBody(_)
            | Node::ForColonDelimitedBody(_)
            | Node::ForeachColonDelimitedBody(_)
            | Node::DeclareColonDelimitedBody(_)
            | Node::NamespaceImplicitBody(_)
            | Node::NamespaceBody(_)
            | Node::SwitchBody(_)
                if i > 0 =>
            {
                i -= 1;
            }
            _ => return Some(node_span(&node)),
        }
    }
}

fn is_list_holder(node: Node<'_, '_>) -> bool {
    matches!(
        node,
        Node::Block(_)
            | Node::Program(_)
            | Node::NamespaceImplicitBody(_)
            | Node::SwitchExpressionCase(_)
            | Node::SwitchDefaultCase(_)
            | Node::IfColonDelimitedBody(_)
            | Node::IfColonDelimitedBodyElseIfClause(_)
            | Node::IfColonDelimitedBodyElseClause(_)
            | Node::WhileColonDelimitedBody(_)
            | Node::ForColonDelimitedBody(_)
            | Node::ForeachColonDelimitedBody(_)
            | Node::DeclareColonDelimitedBody(_)
    )
}
