use mago_syntax::cst::*;
use pun_problem::{Edit, Fix, Safety, Span};

use super::rector_stmts::{HOLDERS, List, block_list, holder_list, if_view, is_exit_or_throw};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoUnreachableCode;

impl Rule for NoUnreachableCode {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-unreachable-code"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(HOLDERS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let raw = crate::syntax::statements(node);
        if raw.len() < 2 || !raw[..raw.len() - 1].iter().any(|s| may_terminate(s)) {
            return;
        }
        let Some(list) = holder_list(ctx.source, node, ancestors) else {
            return;
        };
        if list.len() < 2 {
            return;
        }
        let top_level = matches!(node, Node::Program(_) | Node::NamespaceImplicitBody(_))
            || (matches!(node, Node::Block(_))
                && matches!(ancestors.last(), Some(Node::NamespaceBody(_))));
        for key in 1..list.len() {
            let previous = list.stmts[key - 1];
            let current = list.stmts.get(key).copied();
            if !is_always_terminated(ctx.source, top_level, previous, current) {
                continue;
            }
            let start = node_span(previous).end;
            let end = list
                .nop_span(ctx.source)
                .map_or_else(|| node_span(*list.stmts.last().unwrap()).end, |s| s.end);
            let rest = &ctx.source[end..];
            let gap = rest.len() - rest.trim_start().len();
            let end = if rest[gap..].starts_with("?>") {
                end + gap + 2
            } else {
                end
            };
            out.push(Report {
                span: Span::new(list.with_comments(ctx, key).start, end),
                message: "This code can never run.".to_string(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits: vec![Edit::new(Span::new(start, end), "")],
                }),
            });
            return;
        }
    }
}

fn may_terminate(s: &Statement<'_>) -> bool {
    match s {
        Statement::Return(_)
        | Statement::Break(_)
        | Statement::Continue(_)
        | Statement::Goto(_)
        | Statement::Try(_)
        | Statement::If(_)
        | Statement::Switch(_)
        | Statement::While(_)
        | Statement::DoWhile(_)
        | Statement::For(_) => true,
        Statement::Expression(e) => is_exit_or_throw(e.expression),
        _ => false,
    }
}

fn is_always_terminated(
    source: &str,
    top_level: bool,
    node: &Statement<'_>,
    current: Option<&Statement<'_>>,
) -> bool {
    let Some(current) = current else {
        return false;
    };
    if matches!(current, Statement::Inline(_)) {
        return false;
    }
    if top_level
        && matches!(
            current,
            Statement::Class(_)
                | Statement::Interface(_)
                | Statement::Trait(_)
                | Statement::Enum(_)
                | Statement::Function(_)
        )
    {
        return false;
    }
    match node {
        Statement::While(_) | Statement::DoWhile(_) | Statement::For(_) => {
            is_terminated_infinite_loop(node)
        }
        Statement::Try(r#try) => is_terminated_in_last_stmts_try_catch(source, r#try),
        Statement::If(r#if) => is_terminated_in_last_stmts_if(source, r#if),
        Statement::Switch(switch) => is_terminated_in_last_stmts_switch(source, switch),
        _ => is_terminated_node(node, current),
    }
}

fn is_terminated_node(previous: &Statement<'_>, current: &Statement<'_>) -> bool {
    match previous {
        Statement::Return(_) | Statement::Break(_) | Statement::Continue(_) => true,
        Statement::Expression(e) => is_exit_or_throw(e.expression),
        Statement::Goto(_) => !matches!(current, Statement::Label(_)),
        _ => false,
    }
}

fn cases<'a>(switch: &'a Switch<'a>) -> &'a [SwitchCase<'a>] {
    match &switch.body {
        SwitchBody::BraceDelimited(b) => b.cases.as_slice(),
        SwitchBody::ColonDelimited(b) => b.cases.as_slice(),
    }
}

fn case_statements<'a>(case: &'a SwitchCase<'a>) -> &'a Sequence<'a, Statement<'a>> {
    match case {
        SwitchCase::Expression(c) => &c.statements,
        SwitchCase::Default(c) => &c.statements,
    }
}

fn is_terminated_in_last_stmts_switch(source: &str, switch: &Switch<'_>) -> bool {
    let cases = cases(switch);
    if cases.is_empty() {
        return false;
    }
    if cases.iter().any(has_escaping_jump) {
        return false;
    }
    let mut has_default = false;
    for (key, case) in cases.iter().enumerate() {
        if matches!(case, SwitchCase::Default(_)) {
            has_default = true;
        }
        let node = match case {
            SwitchCase::Expression(c) => Node::SwitchExpressionCase(c),
            SwitchCase::Default(c) => Node::SwitchDefaultCase(c),
        };
        let list = holder_list(source, node, &[]).unwrap();
        if list.is_empty() && key + 1 < cases.len() {
            continue;
        }
        if !is_terminated_in_last_stmts(&list) {
            return false;
        }
    }
    has_default
}

fn has_escaping_jump(case: &SwitchCase<'_>) -> bool {
    fn find(node: Node<'_, '_>) -> bool {
        match node {
            Node::Function(_)
            | Node::Method(_)
            | Node::Closure(_)
            | Node::ArrowFunction(_)
            | Node::Class(_)
            | Node::Interface(_)
            | Node::Trait(_)
            | Node::Enum(_)
            | Node::AnonymousClass(_) => false,
            Node::Break(_) | Node::Continue(_) | Node::Goto(_) => true,
            _ => {
                let mut found = false;
                node.visit_children(|child| found = found || find(child));
                found
            }
        }
    }
    case_statements(case)
        .iter()
        .any(|s| find(Node::Statement(s)))
}

fn is_terminated_in_last_stmts_try_catch(source: &str, r#try: &Try<'_>) -> bool {
    if let Some(finally) = &r#try.finally_clause
        && is_terminated_in_last_stmts(&block_list(source, &finally.block))
    {
        return true;
    }
    if !r#try
        .catch_clauses
        .iter()
        .all(|c| is_terminated_in_last_stmts(&block_list(source, &c.block)))
    {
        return false;
    }
    is_terminated_in_last_stmts(&block_list(source, &r#try.block))
}

fn is_terminated_in_last_stmts_if(source: &str, r#if: &If<'_>) -> bool {
    let view = if_view(source, r#if);
    if view.elseifs.is_empty() && view.else_.is_none() {
        return false;
    }
    if !view
        .elseifs
        .iter()
        .all(|(_, list, _)| is_terminated_in_last_stmts(list))
    {
        return false;
    }
    if !is_terminated_in_last_stmts(&view.stmts) {
        return false;
    }
    view.else_
        .as_ref()
        .is_some_and(|(list, _)| is_terminated_in_last_stmts(list))
}

fn is_terminated_in_last_stmts(list: &List<'_>) -> bool {
    match list.stmts.last().copied() {
        Some(Statement::Expression(e)) => is_exit_or_throw(e.expression),
        Some(Statement::Return(_)) => true,
        Some(l @ (Statement::While(_) | Statement::DoWhile(_) | Statement::For(_))) => {
            is_terminated_infinite_loop(l)
        }
        _ => false,
    }
}

fn is_terminated_infinite_loop(r#loop: &Statement<'_>) -> bool {
    let infinite = match r#loop {
        Statement::While(w) => is_always_true(w.condition),
        Statement::DoWhile(d) => is_always_true(d.condition),
        Statement::For(f) => f.conditions.iter().last().is_none_or(|c| is_always_true(c)),
        _ => false,
    };
    infinite && !has_break_or_goto(Node::Statement(r#loop))
}

fn is_always_true(expr: &Expression<'_>) -> bool {
    match crate::syntax::unparenthesize(expr) {
        Expression::Literal(Literal::True(_)) => true,
        Expression::Literal(Literal::Integer(i)) => i.value.is_some_and(|v| v != 0),
        Expression::ConstantAccess(c) => {
            let name = c.name.value();
            name.strip_prefix(b"\\")
                .unwrap_or(name)
                .eq_ignore_ascii_case(b"true")
        }
        _ => false,
    }
}

fn has_break_or_goto(node: Node<'_, '_>) -> bool {
    let mut found = false;
    node.visit_children(|child| {
        found = found
            || match child {
                Node::Function(_)
                | Node::Method(_)
                | Node::Closure(_)
                | Node::ArrowFunction(_)
                | Node::Class(_)
                | Node::Interface(_)
                | Node::Trait(_)
                | Node::Enum(_)
                | Node::AnonymousClass(_) => false,
                Node::Break(_) | Node::Goto(_) => true,
                _ => has_break_or_goto(child),
            }
    });
    found
}
