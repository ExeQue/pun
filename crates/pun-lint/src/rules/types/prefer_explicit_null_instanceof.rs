use crate::problem::{Edit, Safety, Span};
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{BinaryOperator, Construct, Expression, Node, NodeKind};

use super::facts::facts;
use super::prefer_instanceof::needs_parentheses;
use super::rector_type::{RectorType, get_type};
use crate::rules::support::expressions::{is_constant, text};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferExplicitNullInstanceof;

impl Rule for PreferExplicitNullInstanceof {
    fn id(&self) -> &'static str {
        "lint/types/prefer-explicit-null-instanceof"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::If,
            NodeKind::IfStatementBodyElseIfClause,
            NodeKind::IfColonDelimitedBodyElseIfClause,
            NodeKind::Conditional,
            NodeKind::Binary,
            NodeKind::EmptyConstruct,
            NodeKind::While,
            NodeKind::DoWhile,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let facts = facts(ctx);
        let native = |expr: &Expression<'_>| {
            let t = facts.native_type(node_span(expr))?;
            nullable_object(t)
        };
        let mut changes: Vec<(Span, &Expression<'_>, String, bool)> = Vec::new();
        match node {
            Node::If(_)
            | Node::IfStatementBodyElseIfClause(_)
            | Node::IfColonDelimitedBodyElseIfClause(_)
            | Node::Conditional(_) => {
                let condition = match node {
                    Node::If(i) => i.condition,
                    Node::IfStatementBodyElseIfClause(c) => c.condition,
                    Node::IfColonDelimitedBodyElseIfClause(c) => c.condition,
                    Node::Conditional(t) if t.then.is_some() => t.condition,
                    _ => return,
                };
                let condition = unparenthesize(condition);
                let (value, negated) = match condition {
                    Expression::UnaryPrefix(not) if not.operator.is_not() => {
                        (unparenthesize(not.operand), true)
                    }
                    other => (other, false),
                };
                if bails(value) {
                    return;
                }
                if native(value).is_none() {
                    return;
                }
                let RectorType::Scope(Some(t)) = get_type(&facts, value) else {
                    return;
                };
                if let Some(class) = nullable_object(t) {
                    changes.push((node_span(condition), value, class, negated));
                }
            }
            Node::Binary(binary) => match binary.operator {
                BinaryOperator::Identical(_) | BinaryOperator::NotIdentical(_) => {
                    let value = if is_constant(binary.lhs, "null") {
                        binary.rhs
                    } else if is_constant(binary.rhs, "null") {
                        binary.lhs
                    } else {
                        return;
                    };
                    let value = unparenthesize(value);
                    if let Some(class) = native(value) {
                        let negated = matches!(binary.operator, BinaryOperator::Identical(_));
                        changes.push((node_span(binary), value, class, negated));
                    }
                }
                BinaryOperator::And(_) | BinaryOperator::Or(_) => {
                    let sides = [unparenthesize(binary.lhs), unparenthesize(binary.rhs)];
                    if sides.iter().any(|s| matches!(s, Expression::Assignment(_))) {
                        return;
                    }
                    for side in sides {
                        let value = match side {
                            Expression::UnaryPrefix(not)
                                if not.operator.is_not()
                                    && matches!(binary.operator, BinaryOperator::Or(_)) =>
                            {
                                unparenthesize(not.operand)
                            }
                            other => other,
                        };
                        let never = matches!(value, Expression::UnaryPrefix(_)) || bails(value);
                        if never {
                            continue;
                        }
                        if let Some(class) = native(value) {
                            changes.push((node_span(value), value, class, false));
                        }
                    }
                }
                _ => {}
            },
            Node::EmptyConstruct(empty) => {
                let value = unparenthesize(empty.value);
                if matches!(value, Expression::ArrayAccess(_)) {
                    return;
                }
                let Some(class) = native(value) else { return };
                let mut span = node_span(empty);
                let mut negated = true;
                for parent in ancestors.iter().rev() {
                    match parent {
                        Node::Parenthesized(_) | Node::Expression(_) | Node::Construct(_) => {}
                        Node::UnaryPrefix(not) if not.operator.is_not() => {
                            span = node_span(*not);
                            negated = false;
                            break;
                        }
                        _ => break,
                    }
                }
                changes.push((span, value, class, negated));
            }
            _ => {
                let condition = if let Node::While(w) = node {
                    w.condition
                } else {
                    let Node::DoWhile(d) = node else { return };
                    d.condition
                };
                let value = unparenthesize(condition);
                if matches!(value, Expression::Assignment(_) | Expression::Binary(_)) {
                    return;
                }
                if let Some(class) = native(value) {
                    changes.push((node_span(value), value, class, false));
                }
            }
        }
        let source = ctx.source;
        changes.retain(|(span, value, ..)| {
            let inner = node_span(*value);
            ![
                &source[span.start..inner.start],
                &source[inner.end..span.end],
            ]
            .iter()
            .any(|code| code.contains(['/', '#']))
        });
        let Some(first) = changes.first() else { return };
        let span = first.0;
        let edits = changes
            .iter()
            .map(|(span, value, class, negated)| {
                let value = if needs_parentheses(value) {
                    format!("({})", text(ctx, value))
                } else {
                    text(ctx, value).to_string()
                };
                let not = if *negated { "!" } else { "" };
                Edit::new(*span, format!("{not}{value} instanceof \\{class}"))
            })
            .collect();
        out.push(Report::fixed(
            span,
            "Check a nullable object with `instanceof`.",
            Safety::Unsafe,
            edits,
        ));
    }
}

fn bails(value: &Expression<'_>) -> bool {
    matches!(
        value,
        Expression::Binary(_) | Expression::Construct(Construct::Empty(_) | Construct::Isset(_))
    )
}

fn nullable_object(t: &TUnion) -> Option<String> {
    let ([TAtomic::Null, object] | [object, TAtomic::Null]) = t.types.as_ref() else {
        return None;
    };
    let name = match object {
        TAtomic::Object(TObject::Named(named))
            if named.intersection_types.is_none() && !named.is_this && !named.is_static =>
        {
            named.name
        }
        TAtomic::Object(TObject::Enum(e)) => e.name,
        _ => return None,
    };
    Some(name.to_string())
}
