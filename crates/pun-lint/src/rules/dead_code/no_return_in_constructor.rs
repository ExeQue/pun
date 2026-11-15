use mago_syntax::cst::{MethodBody, Node, NodeKind, Statement};
use pun_problem::{Edit, Fix, Safety, Span};

use crate::rules::support::statements::{Frame, holder_list, is_dynamic_expr, line_indent};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoReturnInConstructor;

impl Rule for NoReturnInConstructor {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-return-in-constructor"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Method(method) = node else { return };
        let MethodBody::Concrete(body) = &method.body else {
            return;
        };
        if !method.name.value.eq_ignore_ascii_case(b"__construct") {
            return;
        }
        for statement in &body.statements {
            walk(
                ctx.source,
                Node::Statement(statement),
                Node::Block(body),
                true,
                out,
            );
        }
    }
}

fn walk(source: &str, node: Node<'_, '_>, parent: Node<'_, '_>, top: bool, out: &mut Vec<Report>) {
    match node {
        Node::Class(_) | Node::AnonymousClass(_) | Node::Function(_) | Node::Closure(_) => return,
        Node::Statement(Statement::Return(r#return)) => {
            if let Some(value) = r#return.value {
                let span = node_span(r#return);
                let dynamic = is_dynamic_expr(value);
                if dynamic && top {
                    return;
                }
                let edit = if dynamic {
                    let value = node_span(unparenthesize(value));
                    expanded(source, span, &source[value.start..value.end], parent)
                } else {
                    Edit::new(
                        Span::new(node_span(&r#return.r#return).end, node_span(value).end),
                        "",
                    )
                };
                out.push(Report {
                    span,
                    message: "A constructor returns no value; remove it.".to_string(),
                    help: None,
                    fix: Some(Fix {
                        safety: Safety::Safe,
                        edits: vec![edit],
                    }),
                });
            }
            return;
        }
        _ => {}
    }
    node.visit_children(|child| walk(source, child, node, false, out));
}

fn expanded(source: &str, span: Span, value: &str, parent: Node<'_, '_>) -> Edit {
    let tokens = crate::rules::support::statements::lex(source);
    let list = holder_list(source, &tokens, parent);
    let indent = line_indent(source, span.start);
    if list.frame == Frame::Bare {
        return Edit::new(
            span,
            format!("{{\n{indent}    {value};\n{indent}    return;\n{indent}}}"),
        );
    }
    let index = list
        .stmts
        .iter()
        .position(|s| node_span(*s) == span)
        .expect("the return is in the list");
    if index > 0 {
        let previous = node_span(list.stmts[index - 1]).end;
        return Edit::new(
            Span::new(previous, span.end),
            format!("\n{indent}{value};\n{indent}return;"),
        );
    }
    if let Some(next) = list.stmts.get(1) {
        let from = tokens
            .iter()
            .find(|t| t.start >= list.inner.start && t.end <= span.start && t.kind.is_comment())
            .map_or(span.start, |t| t.start);
        return Edit::new(
            Span::new(from, node_span(*next).start),
            format!("{value};\n{indent}return;\n{indent}"),
        );
    }
    let outer = line_indent(source, list.inner.start - 1);
    let inner = format!("{outer}    ");
    let text = format!("\n{inner}{value};\n{inner}return;");
    match list.frame {
        Frame::Braces { close, .. } => Edit::new(
            Span::new(list.inner.start, close),
            format!("{text}\n{outer}"),
        ),
        _ => Edit::new(Span::new(list.inner.start, span.end), text),
    }
}
