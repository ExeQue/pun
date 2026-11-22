use crate::problem::{Edit, Fix, Safety, Span};
use mago_syntax::cst::{
    AssignmentOperator, Binary, BinaryOperator, Expression, ForBody, ForeachBody, ForeachTarget,
    IfBody, Node, NodeKind, Sequence, Statement, UnaryPrefixOperator,
};

use crate::rules::support::options::part_on;
use crate::rules::support::statements::{
    body_list, enclosing, keeps_parentheses, line_indent, moved_text, open_list, precedence,
    remove, tokens,
};
use crate::rules::support::variable_usage::{try_of, used_in_next};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct NoEmptyBlock {
    /// `if-foreach-for` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for NoEmptyBlock {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-empty-block"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !checks(options, "if-foreach-for")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::If, NodeKind::For, NodeKind::Foreach])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if self.off {
            return;
        }
        let Some(body) = body_of(node) else { return };
        let looks_empty = match body {
            Body::One(Statement::Block(b)) => b.statements.iter().all(dropped),
            Body::One(s) => dropped(s),
            Body::Open(s, ..) => s.iter().all(dropped),
        };
        if !looks_empty {
            return;
        }
        let source = ctx.source;
        let tokens = tokens(ctx);
        let list = match body {
            Body::One(s) => body_list(tokens, s),
            Body::Open(s, from, to) => open_list(tokens, s, from, to),
        };
        if !list.is_empty() {
            return;
        }
        let (list, key) = enclosing(source, tokens, ancestors);
        let try_ = try_of(ancestors);
        let fix = match node {
            Node::If(r#if) => {
                let (else_if, else_part) = match &r#if.body {
                    IfBody::Statement(b) => (
                        !b.else_if_clauses.is_empty(),
                        b.else_clause.as_ref().map(|e| {
                            let gone = Span::new(node_span(b.statement).end, node_span(e).end);
                            (body_list(tokens, e.statement), gone)
                        }),
                    ),
                    IfBody::ColonDelimited(b) => (
                        !b.else_if_clauses.is_empty(),
                        b.else_clause.as_ref().map(|e| {
                            let endif = b.endif.span.start.offset as usize;
                            let gone = Span::new(node_span(&e.r#else).start, endif);
                            let list = open_list(
                                tokens,
                                &e.statements,
                                e.colon.end.offset as usize,
                                endif,
                            );
                            (list, gone)
                        }),
                    ),
                };
                if else_if {
                    return;
                }
                match else_part {
                    Some((else_list, gone)) if else_list.is_empty() => vec![
                        Edit::new(
                            node_span(r#if.condition),
                            inverted_condition(source, r#if.condition),
                        ),
                        Edit::new(gone, ""),
                    ],
                    Some((else_list, _)) => {
                        let span = node_span(r#if);
                        let indent = line_indent(source, span.start);
                        let inner = format!("{indent}    ");
                        let body = moved_text(source, tokens, &else_list, &inner);
                        let condition = inverted_condition(source, r#if.condition);
                        vec![Edit::new(
                            span,
                            format!("if ({condition}) {{\n{inner}{body}\n{indent}}}"),
                        )]
                    }
                    None if has_side_effect(r#if.condition) => return,
                    None => remove(source, tokens, &list, &[key]),
                }
            }
            Node::For(r#for) => {
                let header = r#for
                    .initializations
                    .iter()
                    .chain(r#for.conditions.iter())
                    .chain(r#for.increments.iter());
                for expr in header {
                    if variables(Node::Expression(expr))
                        .iter()
                        .any(|name| used_in_next(&list, key + 1, try_, name))
                    {
                        return;
                    }
                }
                remove(source, tokens, &list, &[key])
            }
            _ => {
                let Node::Foreach(foreach) = node else { return };
                if has_side_effect(foreach.expression) {
                    return;
                }
                let value = match &foreach.target {
                    ForeachTarget::Value(t) => t.value,
                    ForeachTarget::KeyValue(t) => t.value,
                };
                for expr in [foreach.expression, value, value] {
                    if variables(Node::Expression(expr))
                        .iter()
                        .any(|name| used_in_next(&list, key + 1, try_, name))
                    {
                        return;
                    }
                }
                remove(source, tokens, &list, &[key])
            }
        };
        out.push(Report {
            span: node_span(&node),
            message: "This statement has an empty body and does nothing; remove it.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: fix,
            }),
        });
    }
}

fn dropped(s: &Statement<'_>) -> bool {
    matches!(
        s,
        Statement::Noop(_) | Statement::OpeningTag(_) | Statement::ClosingTag(_)
    )
}

#[derive(Clone, Copy)]
enum Body<'ast, 'arena> {
    One(&'ast Statement<'arena>),
    Open(&'ast Sequence<'arena, Statement<'arena>>, usize, usize),
}

fn body_of<'ast, 'arena>(node: Node<'ast, 'arena>) -> Option<Body<'ast, 'arena>> {
    let after = |s: mago_span::Span| s.end.offset as usize;
    let start = |s: mago_span::Span| s.start.offset as usize;
    Some(match node {
        Node::If(r#if) => match &r#if.body {
            IfBody::Statement(b) => Body::One(b.statement),
            IfBody::ColonDelimited(b) => {
                let to = b
                    .else_if_clauses
                    .first()
                    .map(|c| start(mago_span::HasSpan::span(c)))
                    .or_else(|| b.else_clause.as_ref().map(|c| start(c.r#else.span)))
                    .unwrap_or(start(b.endif.span));
                Body::Open(&b.statements, after(b.colon), to)
            }
        },
        Node::For(r#for) => match &r#for.body {
            ForBody::Statement(s) => Body::One(s),
            ForBody::ColonDelimited(b) => {
                Body::Open(&b.statements, after(b.colon), start(b.end_for.span))
            }
        },
        _ => {
            let Node::Foreach(f) = node else { return None };
            match &f.body {
                ForeachBody::Statement(s) => Body::One(s),
                ForeachBody::ColonDelimited(b) => {
                    Body::Open(&b.statements, after(b.colon), start(b.end_foreach.span))
                }
            }
        }
    })
}

fn has_side_effect(expr: &Expression<'_>) -> bool {
    fn walk(node: Node<'_, '_>) -> bool {
        match node {
            Node::Call(_)
            | Node::PartialApplication(_)
            | Node::Instantiation(_)
            | Node::AnonymousClass(_) => {
                return true;
            }
            Node::Assignment(a)
                if matches!(a.operator, AssignmentOperator::Assign(_))
                    && !matches!(
                        a.rhs,
                        Expression::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Reference(_))
                    ) =>
            {
                return true;
            }
            _ => {}
        }
        let mut found = false;
        node.visit_children(|child| found = found || walk(child));
        found
    }
    walk(Node::Expression(expr))
}

fn variables(node: Node<'_, '_>) -> Vec<Vec<u8>> {
    fn walk(node: Node<'_, '_>, parent: Option<Node<'_, '_>>, out: &mut Vec<Vec<u8>>) {
        match node {
            Node::DirectVariable(v) => {
                if !matches!(
                    parent,
                    Some(Node::PropertyAbstractItem(_) | Node::PropertyConcreteItem(_))
                ) {
                    out.push(v.name[1..].to_vec());
                }
                return;
            }
            Node::IndirectVariable(_) | Node::NestedVariable(_) => out.push(Vec::new()),
            _ => {}
        }
        node.visit_children(|child| walk(child, Some(node), out));
    }
    let mut out = Vec::new();
    walk(node, None, &mut out);
    out
}

pub(crate) fn inverted_condition(source: &str, condition: &Expression<'_>) -> String {
    let text = |e: &Expression| {
        let s = node_span(e);
        &source[s.start..s.end]
    };
    let condition = unparenthesize(condition);
    match condition {
        Expression::Binary(binary) => {
            if let Some(op) = inverted_operator(binary) {
                let operand = |e: &Expression| -> String {
                    let inner = unparenthesize(e);
                    let own = match inner {
                        Expression::Binary(b) => precedence(&b.operator),
                        _ if keeps_parentheses(inner, None) => 0,
                        _ => u8::MAX,
                    };
                    if own <= precedence(&binary.operator) {
                        format!("({})", text(inner))
                    } else {
                        text(inner).to_string()
                    }
                };
                return format!("{} {op} {}", operand(binary.lhs), operand(binary.rhs));
            }
            if matches!(binary.operator, BinaryOperator::Instanceof(_)) {
                return format!("!{}", text(condition));
            }
            format!("!({})", text(condition))
        }
        Expression::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Not(_)) => {
            text(unparenthesize(u.operand)).to_string()
        }
        Expression::Assignment(_) => format!("!{}", text(condition)),
        _ if keeps_parentheses(condition, None) => format!("!({})", text(condition)),
        _ => format!("!{}", text(condition)),
    }
}

fn inverted_operator(binary: &Binary<'_>) -> Option<&'static str> {
    let is_or = |e: &Expression| {
        matches!(
            unparenthesize(e),
            Expression::Binary(Binary {
                operator: BinaryOperator::Or(_),
                ..
            })
        )
    };
    if is_or(binary.lhs) || is_or(binary.rhs) {
        return None;
    }
    Some(match binary.operator {
        BinaryOperator::Identical(_) => "!==",
        BinaryOperator::NotIdentical(_) => "===",
        BinaryOperator::Equal(_) => "!=",
        BinaryOperator::NotEqual(_) | BinaryOperator::AngledNotEqual(_) => "==",
        BinaryOperator::GreaterThan(_) => "<=",
        BinaryOperator::LessThan(_) => ">=",
        BinaryOperator::GreaterThanOrEqual(_) => "<",
        BinaryOperator::LessThanOrEqual(_) => ">",
        _ => return None,
    })
}

const CHECKS: [&str; 2] = ["if-foreach-for", "loop"];

/// Pun's `checks`: the ones that run, one for each of Rector's 2 rules (`if-foreach-for` is
/// `RemoveDeadIfForeachForRector`, `loop` is `RemoveDeadLoopRector`).
pub(super) fn checks(options: &serde_json::Value, check: &str) -> Result<bool, OptionsError> {
    part_on(options, "checks", &CHECKS, check)
}
