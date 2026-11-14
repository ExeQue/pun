use mago_syntax::cst::*;
use pun_problem::{Edit, Fix, Safety, Span};

use crate::rules::support::expressions::call_to;
use crate::rules::support::statement_edits::{block_list, remove};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferVariadicParam;

impl Rule for PreferVariadicParam {
    fn id(&self) -> &'static str {
        "lint/style/prefer-variadic-param"
    }

    fn min_php_version(&self) -> u32 {
        50600
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Function, NodeKind::Method, NodeKind::Closure])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some((params, body)) = parts(node) else {
            return;
        };
        if !params.parameters.is_empty() || func_get_args_assign(body).is_none() {
            return;
        }
        let list = block_list(ctx.source, body);
        for (index, statement) in list.stmts.iter().enumerate() {
            let Some(assign) = assign_of(statement) else {
                continue;
            };
            let (name, mut edits) = match assign.lhs {
                Expression::Variable(Variable::Direct(v)) => (v.name, remove(ctx, &list, &[index])),
                Expression::Variable(_) => return,
                _ => (
                    &b"$args"[..],
                    vec![Edit::new(node_span(assign.rhs), "$args")],
                ),
            };
            if !has_function_or_closure_inside(body, name) {
                let at = params.right_parenthesis.start.offset as usize;
                edits.push(Edit::new(
                    Span::new(at, at),
                    format!("...{}", String::from_utf8_lossy(name)),
                ));
            }
            out.push(Report {
                span: node_span(assign),
                message: "Use a variadic parameter instead of `func_get_args()`.".to_string(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits,
                }),
            });
            return;
        }
    }
}

fn parts<'a>(node: Node<'a, 'a>) -> Option<(&'a FunctionLikeParameterList<'a>, &'a Block<'a>)> {
    match node {
        Node::Function(f) => Some((&f.parameter_list, &f.body)),
        Node::Method(m) => match &m.body {
            MethodBody::Concrete(block) => Some((&m.parameter_list, block)),
            MethodBody::Abstract(_) => None,
        },
        Node::Closure(c) => Some((&c.parameter_list, &c.body)),
        _ => None,
    }
}

fn assign_of<'a>(statement: &'a Statement<'a>) -> Option<&'a Assignment<'a>> {
    let Statement::Expression(e) = statement else {
        return None;
    };
    match e.expression {
        Expression::Assignment(a)
            if matches!(a.operator, AssignmentOperator::Assign(_))
                && call_to(a.rhs, "func_get_args").is_some() =>
        {
            Some(a)
        }
        _ => None,
    }
}

fn func_get_args_assign<'a>(body: &'a Block<'a>) -> Option<&'a Assignment<'a>> {
    fn find<'a>(node: Node<'a, 'a>, found: &mut Option<&'a Assignment<'a>>) {
        match node {
            Node::Function(_)
            | Node::Closure(_)
            | Node::ArrowFunction(_)
            | Node::Class(_)
            | Node::AnonymousClass(_)
            | Node::Interface(_)
            | Node::Trait(_)
            | Node::Enum(_) => return,
            Node::Statement(s) if found.is_none() => *found = assign_of(s),
            _ => {}
        }
        node.visit_children(|child| {
            if found.is_none() {
                find(child, found);
            }
        });
    }
    let mut found = None;
    find(Node::Block(body), &mut found);
    found
}

fn has_function_or_closure_inside(body: &Block<'_>, name: &[u8]) -> bool {
    crate::rules::support::side_effects::any_node(Node::Block(body), &|n| {
        let Some((params, inner)) = (match n {
            Node::Function(_) | Node::Closure(_) => parts(n),
            _ => None,
        }) else {
            return false;
        };
        params.parameters.is_empty()
            && func_get_args_assign(inner).is_some_and(
                |a| matches!(a.lhs, Expression::Variable(Variable::Direct(v)) if v.name == name),
            )
    })
}
