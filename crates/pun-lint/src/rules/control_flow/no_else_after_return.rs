use mago_syntax::cst::*;
use pun_problem::{Edit, Fix, Safety, Span};

use crate::rules::support::statement_edits::{
    Frame, IfView, List, braced, comments, enclosing, if_view, inserted_statements,
    is_exit_or_throw, line_indent, moved_statements, replace_statement,
};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoElseAfterReturn;

impl Rule for NoElseAfterReturn {
    fn id(&self) -> &'static str {
        "lint/control-flow/no-else-after-return"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Statement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Statement(s @ Statement::If(r#if)) = node else {
            return;
        };
        let source = ctx.source;
        let has_more = match &r#if.body {
            IfBody::Statement(b) => !b.else_if_clauses.is_empty() || b.else_clause.is_some(),
            IfBody::ColonDelimited(b) => !b.else_if_clauses.is_empty() || b.else_clause.is_some(),
        };
        if !has_more {
            return;
        }
        let view = if_view(source, r#if);
        if does_not_last_statement_break_flow(source, &view.stmts) {
            return;
        }
        let (list, index) = enclosing(source, s, ancestors);
        let items = if view.elseifs.is_empty() {
            let (else_stmts, _) = view.else_.as_ref().unwrap();
            let mut items = vec![Item::Kept];
            if !else_stmts.is_empty() {
                items.push(Item::Stmts(else_stmts));
            }
            items
        } else {
            handle_else_ifs(source, &view)
        };
        let clauses = clauses(source, &view);
        let inserted = list.frame != Frame::Bare;
        let edit = if view.elseifs.is_empty() || !inserted {
            replace_statement(source, &list, index, |indent| {
                items
                    .iter()
                    .map(|item| print(ctx, r#if, &clauses, item, indent, inserted))
                    .collect::<Vec<_>>()
                    .join(&format!("\n{indent}"))
            })
        } else {
            let before = list.before(ctx, index);
            let start = list.with_comments(ctx, index).start;
            let indent = line_indent(source, node_span(r#if).start);
            let mut text = String::new();
            for item in &items {
                if matches!(item, Item::ModifiedIf(..)) {
                    text.push_str(&source[before..start]);
                } else {
                    text.push_str(&format!("\n{indent}"));
                    if matches!(item, Item::NewIf(0)) && start < node_span(r#if).start {
                        text.push_str(&format!(
                            "{}\n{indent}",
                            source[start..node_span(r#if).start].trim_end()
                        ));
                    }
                }
                text.push_str(&print(ctx, r#if, &clauses, item, indent, inserted));
            }
            Edit::new(Span::new(before, node_span(r#if).end), text)
        };
        out.push(Report {
            span: node_span(r#if),
            message: "This `else` follows a branch that always leaves; it is not needed."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![edit],
            }),
        });
    }
}

enum Item<'v, 'a> {
    Kept,
    NewIf(usize),
    ModifiedIf(usize, Vec<usize>),
    Clause(usize),
    Stmts(&'v List<'a>),
}

struct Clause<'v, 'a> {
    condition: Option<&'a Expression<'a>>,
    stmts: &'v List<'a>,
    comments: Option<Span>,
}

fn clauses<'v, 'a>(source: &str, view: &'v IfView<'a>) -> Vec<Clause<'v, 'a>> {
    let mut out = vec![Clause {
        condition: Some(view.condition),
        stmts: &view.stmts,
        comments: None,
    }];
    let mut previous = view.stmts.inner.end;
    for (condition, stmts, span) in &view.elseifs {
        out.push(Clause {
            condition: Some(condition),
            stmts,
            comments: comments(source, previous, span.start),
        });
        previous = stmts.inner.end;
    }
    if let Some((stmts, span)) = &view.else_ {
        out.push(Clause {
            condition: None,
            stmts,
            comments: comments(source, previous, span.start),
        });
    }
    out
}

fn handle_else_ifs<'v, 'a>(source: &str, view: &'v IfView<'a>) -> Vec<Item<'v, 'a>> {
    let breaks =
        |clause: usize| !does_not_last_statement_break_flow(source, clause_stmts(view, clause));
    let mut items = vec![Item::NewIf(0)];
    let mut remaining: Vec<usize> = (1..=view.elseifs.len()).collect();
    while !remaining.is_empty() {
        let current = remaining.remove(0);
        if !breaks(current) {
            let (kept, own): (Vec<usize>, Vec<usize>) = remaining
                .iter()
                .partition(|&&e| !breaks(e) && !clause_stmts(view, e).is_empty());
            items.push(Item::ModifiedIf(current, kept));
            items.extend(own.into_iter().map(Item::Clause));
            break;
        }
        if remaining.is_empty() {
            items.push(Item::ModifiedIf(current, Vec::new()));
            break;
        }
        items.push(Item::NewIf(current));
    }
    if let Some((else_stmts, _)) = &view.else_ {
        let keep_else = items.iter().any(
            |item| matches!(item, Item::NewIf(c) | Item::ModifiedIf(c, _) | Item::Clause(c) if !breaks(*c)),
        );
        if keep_else {
            items.push(Item::Clause(usize::MAX));
        } else if !else_stmts.is_empty() {
            items.push(Item::Stmts(else_stmts));
        }
    }
    items
}

fn clause_stmts<'v, 'a>(view: &'v IfView<'a>, clause: usize) -> &'v List<'a> {
    if clause == 0 {
        &view.stmts
    } else {
        &view.elseifs[clause - 1].1
    }
}

fn print(
    ctx: &Context<'_>,
    r#if: &If<'_>,
    clauses: &[Clause<'_, '_>],
    item: &Item<'_, '_>,
    indent: &str,
    inserted: bool,
) -> String {
    let source = ctx.source;
    let head = |clause: &Clause<'_, '_>| {
        clause
            .comments
            .map(|c| format!("{}\n{indent}", &source[c.start..c.end]))
            .unwrap_or_default()
    };
    let condition = |clause: &Clause<'_, '_>| {
        let c = unparenthesize(clause.condition.unwrap());
        let s = node_span(c);
        &source[s.start..s.end]
    };
    match item {
        Item::Kept => {
            let IfBody::Statement(body) = &r#if.body else {
                let clause = &clauses[0];
                return format!(
                    "if ({}) {}",
                    condition(clause),
                    braced(ctx, clause.stmts, indent)
                );
            };
            let span = node_span(r#if);
            source[span.start..node_span(body.statement).end].to_string()
        }
        Item::NewIf(c) | Item::ModifiedIf(c, _) => {
            let clause = &clauses[*c];
            let mut text = format!(
                "{}if ({}) {}",
                head(clause),
                condition(clause),
                braced(ctx, clause.stmts, indent)
            );
            if let Item::ModifiedIf(_, kept) = item {
                for &e in kept {
                    text.push(' ');
                    text.push_str(&print_clause(ctx, &clauses[e], indent));
                }
            }
            text
        }
        Item::Clause(c) => {
            let clause = if *c == usize::MAX {
                clauses.last().unwrap()
            } else {
                &clauses[*c]
            };
            print_clause(ctx, clause, indent)
        }
        Item::Stmts(list) if inserted => inserted_statements(ctx, list, indent),
        Item::Stmts(list) => moved_statements(ctx, list, indent),
    }
}

fn print_clause(ctx: &Context<'_>, clause: &Clause<'_, '_>, indent: &str) -> String {
    let source = ctx.source;
    let head = clause
        .comments
        .map(|c| format!("{}\n{indent}", &source[c.start..c.end]))
        .unwrap_or_default();
    match clause.condition {
        Some(condition) => {
            let s = node_span(unparenthesize(condition));
            format!(
                "{head}elseif ({}) {}",
                &source[s.start..s.end],
                braced(ctx, clause.stmts, indent)
            )
        }
        None => format!("{head}else {}", braced(ctx, clause.stmts, indent)),
    }
}

fn does_not_last_statement_break_flow(source: &str, stmts: &List<'_>) -> bool {
    match stmts.last() {
        Some(Statement::If(inner)) => {
            let view = if_view(source, inner);
            match &view.else_ {
                Some((else_stmts, _)) => {
                    does_not_last_statement_break_flow(source, &view.stmts)
                        || does_not_last_statement_break_flow(source, else_stmts)
                        || view
                            .elseifs
                            .iter()
                            .any(|(_, l, _)| does_not_last_statement_break_flow(source, l))
                }
                None => true,
            }
        }
        Some(Statement::Return(_) | Statement::Continue(_)) => false,
        Some(Statement::Expression(e)) => !is_exit_or_throw(e.expression),
        _ => true,
    }
}
