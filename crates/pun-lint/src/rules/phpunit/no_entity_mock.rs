use mago_codex::symbol::SymbolKind;
use mago_syntax::cst::{
    Argument, Call, ClassLikeMemberSelector, Expression, Literal, MethodBody, Node, NodeKind,
    PartialApplication, StringPart, Variable,
};
use mago_syntax::token::TokenKind;
use pun_problem::{Edit, Safety, Span};

use super::class_string;
use crate::rules::support::class_members::walk;
use crate::rules::support::rewrites::remove_statements;
use crate::rules::support::statement_edits::{assign, block_list, expression, variable_name};
use crate::rules::support::statements::previous_token_end;
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoEntityMock;

const ENTITY_DOCBLOCK_MARKERS: [&str; 4] =
    ["@Document", "@ORM\\Document", "@Entity", "@ORM\\Entity"];

#[derive(Clone)]
enum State<'a> {
    Original,
    Removed,
    Replaced {
        var: String,
        method: String,
        arg: &'a Expression<'a>,
    },
}

enum FirstArg<'a> {
    Null,
    Str(String),
    Expr(&'a Expression<'a>),
}

type Args<'a> = Option<&'a Expression<'a>>;

fn find_call<'a>(
    node: Node<'a, 'a>,
    found: &dyn Fn(Option<&[u8]>, Option<&[u8]>) -> bool,
) -> Option<Args<'a>> {
    let name = |m: &'a ClassLikeMemberSelector<'a>| match m {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    };
    let call = match node {
        Node::MethodCall(m) => Some((
            m.object,
            name(&m.method),
            m.argument_list.arguments.first().map(Argument::value),
        )),
        Node::MethodPartialApplication(m) => Some((m.object, name(&m.method), None)),
        _ => None,
    };
    if let Some((object, method, args)) = call
        && found(variable_name(object), method)
    {
        return Some(args);
    }
    let mut hit = None;
    node.visit_children(|child| {
        if hit.is_none() {
            hit = find_call(child, found);
        }
    });
    hit
}

fn find_in<'a>(
    state: &State<'a>,
    expr: &'a Expression<'a>,
    found: &dyn Fn(Option<&[u8]>, Option<&[u8]>) -> bool,
) -> Option<Args<'a>> {
    match state {
        State::Replaced { var, method, arg } => {
            if found(Some(var.as_bytes()), Some(method.as_bytes())) {
                return Some(Some(arg));
            }
            find_call(Node::Expression(arg), found)
        }
        _ => find_call(Node::Expression(expr), found),
    }
}

fn first_arg_value(found: Option<Args<'_>>) -> Option<FirstArg<'_>> {
    let Some(Some(value)) = found else {
        return Some(FirstArg::Null);
    };
    Some(match unparenthesize(value) {
        Expression::Literal(Literal::String(s)) => {
            FirstArg::Str(String::from_utf8(s.value?.to_vec()).ok()?)
        }
        _ => FirstArg::Expr(value),
    })
}

fn without_mock(name: &str) -> String {
    name.replace("Mock", "")
}

fn ucfirst(s: &str) -> String {
    let mut out = s.to_string();
    if let Some(first) = out.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    out
}

fn resolve_method_name(facts: &Facts, name: &str, class: &str) -> String {
    if let Some(rest) = name.strip_prefix("get") {
        return format!("set{}", ucfirst(rest));
    }
    if let Some(rest) = name.strip_prefix("is") {
        for setter in [format!("set{}", ucfirst(name)), format!("set{rest}")] {
            if facts
                .documented()
                .method_exists(lookup(class).as_bytes(), setter.as_bytes())
            {
                return setter;
            }
        }
    }
    name.to_string()
}

impl Rule for NoEntityMock {
    fn id(&self) -> &'static str {
        "lint/phpunit/no-entity-mock"
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
        let MethodBody::Concrete(block) = &method.body else {
            return;
        };
        let list = block_list(ctx.source, block);
        let facts = facts(ctx);
        let entities = collect_entity_classes(ctx, &facts, ancestors, &list.stmts);
        if entities.is_empty() {
            return;
        }
        let mut states = vec![State::Original; list.stmts.len()];
        for (name, class) in &entities {
            for (at, statement) in list.stmts.iter().enumerate() {
                if matches!(states[at], State::Removed) {
                    continue;
                }
                let Some(expr) = expression(statement) else {
                    continue;
                };
                let is_call = matches!(
                    expr,
                    Expression::Call(Call::Method(_))
                        | Expression::PartialApplication(PartialApplication::Method(_))
                );
                if matches!(states[at], State::Original) && !is_call {
                    continue;
                }
                let state = states[at].clone();
                let on_variable = |var: Option<&[u8]>, _: Option<&[u8]>| {
                    var.is_some_and(|v| v.eq_ignore_ascii_case(name.as_bytes()))
                };
                if find_in(&state, expr, &on_variable).is_none() {
                    continue;
                }
                let named = |wanted: &'static str| {
                    move |_: Option<&[u8]>, n: Option<&[u8]>| {
                        n.is_some_and(|n| n.eq_ignore_ascii_case(wanted.as_bytes()))
                    }
                };
                let Some(FirstArg::Str(method_name)) =
                    first_arg_value(find_in(&state, expr, &named("method")))
                else {
                    return;
                };
                let mut removed = false;
                if method_name.starts_with("set") {
                    states[at] = State::Removed;
                    removed = true;
                }
                let setter = resolve_method_name(&facts, &method_name, class);
                if let Some(FirstArg::Expr(arg)) =
                    first_arg_value(find_in(&state, expr, &named("willReturn")))
                    && !setter.is_empty()
                    && setter != "0"
                    && !removed
                {
                    states[at] = State::Replaced {
                        var: name.clone(),
                        method: setter,
                        arg,
                    };
                }
            }
        }
        let names: Vec<&str> = entities.iter().map(|(n, _)| n.as_str()).collect();
        let mut edits: Vec<Edit> = Vec::new();
        let mut covered: Vec<Span> = Vec::new();
        let removed: Vec<bool> = states.iter().map(|s| matches!(s, State::Removed)).collect();
        for (at, statement) in list.stmts.iter().enumerate() {
            if removed[at] {
                covered.push(node_span(*statement));
            }
        }
        for (at, statement) in list.stmts.iter().enumerate() {
            if !matches!(states[at], State::Original) {
                continue;
            }
            let Some(assignment) = expression(statement).and_then(assign) else {
                continue;
            };
            let rhs = unparenthesize(assignment.rhs);
            let selector = match rhs {
                Expression::Call(Call::Method(call)) => &call.method,
                Expression::PartialApplication(PartialApplication::Method(call)) => &call.method,
                _ => continue,
            };
            let is_create_mock = matches!(selector, ClassLikeMemberSelector::Identifier(id) if id.value.eq_ignore_ascii_case(b"createMock"));
            let Some(var) = variable_name(assignment.lhs) else {
                continue;
            };
            if !is_create_mock || !names.iter().any(|n| var.eq_ignore_ascii_case(n.as_bytes())) {
                continue;
            }
            let Some((_, class)) = entities.iter().find(|(n, _)| n.as_bytes() == var) else {
                return;
            };
            let span = node_span(rhs);
            covered.push(span);
            edits.push(Edit::new(span, format!("new \\{}()", lookup(class))));
        }
        let renames = renames(Node::Method(method), &names);
        let renamed = |span: Span| -> String {
            let mut text = String::new();
            let mut at = span.start;
            for (s, new) in renames
                .iter()
                .filter(|(s, _)| s.start >= span.start && s.end <= span.end)
            {
                text.push_str(&ctx.source[at..s.start]);
                text.push_str(new);
                at = s.end;
            }
            text.push_str(&ctx.source[at..span.end]);
            text
        };
        for (at, statement) in list.stmts.iter().enumerate() {
            if let State::Replaced { var, method, arg } = &states[at] {
                let span = node_span(expression(statement).unwrap());
                covered.push(span);
                edits.push(Edit::new(
                    span,
                    format!(
                        "${}->{method}({})",
                        without_mock(var),
                        renamed(node_span(unparenthesize(arg)))
                    ),
                ));
            }
        }
        for (span, new) in &renames {
            if !covered
                .iter()
                .any(|c| c.start <= span.start && span.end <= c.end)
            {
                edits.push(Edit::new(*span, new.clone()));
            }
        }
        edits.extend(remove_statements(ctx, &list, &removed));
        out.push(Report::fixed(
            node_span(&method.name),
            "Use the entity class directly instead of a mock of it.",
            Safety::Safe,
            edits,
        ));
    }
}

fn renames(root: Node<'_, '_>, names: &[&str]) -> Vec<(Span, String)> {
    let mut skip: Vec<Span> = Vec::new();
    let mut found: Vec<(Span, String)> = Vec::new();
    walk(root, &mut |node, _| {
        match node {
            Node::StaticPropertyAccess(access) => skip.push(node_span(&access.property)),
            Node::PropertyAbstractItem(item) => skip.push(node_span(&item.variable)),
            Node::PropertyConcreteItem(item) => skip.push(node_span(&item.variable)),
            Node::StringPart(StringPart::Expression(expr)) => {
                let name = match expr {
                    Expression::Variable(Variable::Direct(v)) => Some(&v.name[1..]),
                    Expression::Variable(Variable::Indirect(v)) => match v.expression {
                        Expression::Identifier(id) => Some(id.value()),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(name) = name
                    && let Some(n) = names.iter().find(|n| n.as_bytes() == name)
                {
                    let span = node_span(*expr);
                    skip.push(span);
                    found.push((span, format!("{{${}}}", without_mock(n))));
                }
            }
            Node::DirectVariable(v) => {
                let span = node_span(v);
                if !skip.contains(&span)
                    && let Some(n) = names.iter().find(|n| n.as_bytes() == &v.name[1..])
                {
                    found.push((span, format!("${}", without_mock(n))));
                }
            }
            _ => {}
        }
        true
    });
    found.sort_by_key(|(s, _)| s.start);
    found
}

fn collect_entity_classes(
    ctx: &Context<'_>,
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    statements: &[&mago_syntax::cst::Statement<'_>],
) -> Vec<(String, String)> {
    let mut found: Vec<(String, String)> = Vec::new();
    for statement in statements {
        let Some(assignment) = expression(statement).and_then(assign) else {
            continue;
        };
        let Some(name) = variable_name(assignment.lhs) else {
            continue;
        };
        let Expression::Call(Call::Method(call)) = unparenthesize(assignment.rhs) else {
            continue;
        };
        let is_create_mock = matches!(&call.method, ClassLikeMemberSelector::Identifier(id) if id.value.eq_ignore_ascii_case(b"createMock"));
        let Some(first) = call.argument_list.arguments.first() else {
            continue;
        };
        if !is_create_mock {
            continue;
        }
        let Some(class) = class_string(ctx, ancestors, first.value()) else {
            continue;
        };
        let Some(metadata) = facts.codebase.get_class_like(lookup(&class).as_bytes()) else {
            continue;
        };
        if metadata.kind == SymbolKind::Class && metadata.flags.is_abstract() {
            continue;
        }
        let name = String::from_utf8_lossy(name).into_owned();
        match found.iter_mut().find(|(n, _)| *n == name) {
            Some(entry) => entry.1 = class,
            None => found.push((name, class)),
        }
    }
    found.retain(|(_, class)| is_entity_class(facts, class));
    found
}

fn lookup(class: &str) -> &str {
    class.strip_prefix('\\').unwrap_or(class)
}

fn is_entity_class(facts: &Facts, class: &str) -> bool {
    let Some((metadata, source)) = facts
        .codebase
        .get_class_like(lookup(class).as_bytes())
        .and_then(|m| Some((m, facts.source_of(m.span.file_id)?)))
    else {
        return false;
    };
    let start = metadata.span.start.offset as usize;
    let all = crate::rules::support::statements::lex(&source);
    let from = previous_token_end(&source, &all, start);
    all.iter()
        .rfind(|t| t.start >= from && t.end <= start && t.kind == TokenKind::DocBlockComment)
        .is_some_and(|doc| {
            let doc = &source[doc.start..doc.end];
            ENTITY_DOCBLOCK_MARKERS.iter().any(|m| doc.contains(m))
        })
}
