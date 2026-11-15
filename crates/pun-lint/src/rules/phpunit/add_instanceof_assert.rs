use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{
    Call, ClassLikeMemberSelector, Expression, ForeachBody, MethodBody, Node, NodeKind, Statement,
    Variable,
};
use pun_problem::{Edit, Safety, Span};

use super::is_in_test_class;
use crate::rules::support::statement_edits::{
    List, assign, block_list, body_list, expression, first_comment, holder_list, line_indent,
};
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const MESSAGE: &str = "Assert the instance of a nullable object before using it.";

pub struct AddInstanceofAssert;

impl Rule for AddInstanceofAssert {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-instanceof-assert"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method, NodeKind::Foreach])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        run(node, ancestors, ctx, out, true, &|stmt, collection, _| {
            matched_receivers(ctx, stmt, collection)
        });
    }
}

pub struct AddInstanceofAssertForArgument;

impl Rule for AddInstanceofAssertForArgument {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-instanceof-assert-for-argument"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method, NodeKind::Foreach])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        run(
            node,
            ancestors,
            ctx,
            out,
            false,
            &|stmt, collection, dynamic| {
                matched_argument(ctx, stmt, collection, dynamic)
                    .into_iter()
                    .collect()
            },
        );
    }
}

type Collected = (Vec<u8>, String);

fn run(
    node: Node<'_, '_>,
    ancestors: &[Node<'_, '_>],
    ctx: &Context<'_>,
    out: &mut Vec<Report>,
    static_calls: bool,
    matched: &dyn Fn(&Statement<'_>, &[Collected], &[String]) -> Vec<usize>,
) {
    let list = match node {
        Node::Method(method) => {
            let MethodBody::Concrete(block) = &method.body else {
                return;
            };
            if !is_in_test_class(ctx, ancestors, Some(method)) {
                return;
            }
            block_list(ctx.source, block)
        }
        _ => {
            let Node::Foreach(foreach) = node else { return };
            if !is_in_test_class(ctx, ancestors, None) {
                return;
            }
            match &foreach.body {
                ForeachBody::Statement(statement) => body_list(ctx.source, statement),
                ForeachBody::ColonDelimited(body) => {
                    let mut scope = ancestors.to_vec();
                    scope.push(node);
                    holder_list(ctx.source, Node::ForeachColonDelimitedBody(body), &scope).unwrap()
                }
            }
        }
    };
    if list.len() < 2 {
        return;
    }
    let mut collection = collect(ctx, &list);
    let mut dynamic: Vec<Vec<String>> = Vec::new();
    let mut widened: Vec<String> = Vec::new();
    for statement in &list.stmts {
        dynamic.push(widened.clone());
        dynamic_assign_classes(ctx, statement, &mut widened);
    }
    let mut edits = Vec::new();
    for (i, statement) in list.stmts.iter().enumerate() {
        if let Some(name) = asserted_variable(statement, static_calls) {
            if let Some(index) = match_by_name(&collection, name) {
                collection.remove(index);
            }
            continue;
        }
        let mut found = matched(statement, &collection, &dynamic[i]);
        if found.is_empty() {
            continue;
        }
        let asserts: Vec<&Collected> = found.iter().map(|&index| &collection[index]).collect();
        edits.push(insert_before(ctx, &list, i, &asserts));
        found.sort_unstable();
        for index in found.into_iter().rev() {
            collection.remove(index);
        }
    }
    let Some(first) = edits.first().map(|e: &Edit| e.span) else {
        return;
    };
    out.push(Report::fixed(first, MESSAGE, Safety::Safe, edits));
}

fn asserted_variable<'a>(statement: &'a Statement<'a>, static_calls: bool) -> Option<&'a [u8]> {
    let Statement::Expression(e) = statement else {
        return None;
    };
    let (method, arguments) = match e.expression {
        Expression::Call(Call::Method(call)) => (&call.method, &call.argument_list),
        Expression::Call(Call::StaticMethod(call)) if static_calls => {
            (&call.method, &call.argument_list)
        }
        _ => return None,
    };
    let ClassLikeMemberSelector::Identifier(name) = method else {
        return None;
    };
    if !name.value.eq_ignore_ascii_case(b"assertInstanceOf") {
        return None;
    }
    match arguments
        .arguments
        .get(1)
        .map(mago_syntax::cst::Argument::value)
    {
        Some(Expression::Variable(Variable::Direct(v))) => Some(&v.name[1..]),
        _ => None,
    }
}

fn dynamic_assign_classes(ctx: &Context<'_>, statement: &Statement<'_>, out: &mut Vec<String>) {
    let Some(assignment) = expression(statement).and_then(assign) else {
        return;
    };
    match unparenthesize(assignment.lhs) {
        Expression::Variable(Variable::Indirect(v))
            if !matches!(
                unparenthesize(v.expression),
                Expression::Literal(mago_syntax::cst::Literal::String(_))
            ) => {}
        Expression::Variable(Variable::Nested(_)) => {}
        _ => return,
    }
    let facts = facts(ctx);
    for atomic in facts
        .full_type(node_span(assignment.rhs))
        .iter()
        .flat_map(|t| t.types.iter())
    {
        object_class_names(atomic, out);
    }
}

fn collect(ctx: &Context<'_>, list: &List<'_>) -> Vec<Collected> {
    if list.nop
        || list
            .stmts
            .iter()
            .any(|s| !matches!(s, Statement::Expression(_)))
    {
        return Vec::new();
    }
    let facts = facts(ctx);
    let mut out = Vec::new();
    for statement in &list.stmts {
        let Some(assignment) = expression(statement).and_then(assign) else {
            continue;
        };
        if !matches!(
            unparenthesize(assignment.rhs),
            Expression::Call(Call::Method(_))
        ) {
            continue;
        }
        let Expression::Variable(Variable::Direct(variable)) = unparenthesize(assignment.lhs)
        else {
            continue;
        };
        let rest: Vec<&TAtomic> = (facts.full_type(node_span(assignment)).iter())
            .flat_map(|t| t.types.iter())
            .filter(|a| !matches!(a, TAtomic::Null))
            .collect();
        let class = match rest[..] {
            [TAtomic::Object(TObject::Named(named))]
                if named.intersection_types.is_none() && !named.is_this && !named.is_static =>
            {
                named.name.to_string()
            }
            [TAtomic::Object(TObject::Enum(e))] => e.name.to_string(),
            _ => continue,
        };
        out.push((variable.name[1..].to_vec(), class));
    }
    out
}

fn is_nullable(ctx: &Context<'_>, expr: &Expression<'_>) -> bool {
    let facts = facts(ctx);
    facts
        .full_type(node_span(expr))
        .is_some_and(|t| t.types.len() == 2 && t.types.iter().any(|a| matches!(a, TAtomic::Null)))
}

fn match_by_name(collection: &[Collected], name: &[u8]) -> Option<usize> {
    collection.iter().position(|(n, _)| n == name)
}

fn traverse<'a>(statement: &'a Statement<'a>, visit: &mut dyn FnMut(Node<'a, 'a>) -> bool) {
    let mut stack = vec![Node::Statement(statement)];
    while let Some(node) = stack.pop() {
        if visit(node) {
            return;
        }
        let mut children = Vec::new();
        node.visit_children(|child| children.push(child));
        stack.extend(children.into_iter().rev());
    }
}

fn matched_receivers(
    ctx: &Context<'_>,
    statement: &Statement<'_>,
    collection: &[Collected],
) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    let mut names: Vec<&[u8]> = Vec::new();
    traverse(statement, &mut |node| {
        let Node::MethodCall(call) = node else {
            return false;
        };
        let Expression::Variable(Variable::Direct(direct)) = call.object else {
            return false;
        };
        if !is_nullable(ctx, call.object) {
            return false;
        }
        let name = &direct.name[1..];
        if names.contains(&name) {
            return false;
        }
        if let Some(index) = match_by_name(collection, name) {
            names.push(name);
            out.push(index);
        }
        false
    });
    out
}

fn matched_argument(
    ctx: &Context<'_>,
    statement: &Statement<'_>,
    collection: &[Collected],
    dynamic: &[String],
) -> Option<usize> {
    let mut matched = None;
    traverse(statement, &mut |node| {
        let Node::MethodCall(call) = node else {
            return false;
        };
        if is_assert_method_call(&call.method) {
            return false;
        }
        let parameter_types = parameter_types(ctx, call, dynamic);
        for (position, arg) in call.argument_list.arguments.iter().enumerate() {
            let value = arg.value();
            let Expression::Variable(variable) = value else {
                continue;
            };
            let Variable::Direct(direct) = variable else {
                return false;
            };
            if !is_nullable(ctx, value) {
                return false;
            }
            let Some(parameter_type) = parameter_types.as_ref().and_then(|t| t.get(position))
            else {
                return false;
            };
            matched = match_by_name(collection, &direct.name[1..]);
            return parameter_type.as_ref().is_some_and(is_object_type);
        }
        false
    });
    matched
}

fn is_assert_method_call(method: &ClassLikeMemberSelector<'_>) -> bool {
    const PREFIXES: [&[u8]; 6] = [
        b"expectNotToPerformAssertions",
        b"assert",
        b"expectException",
        b"setExpectedException",
        b"expectOutput",
        b"should",
    ];
    matches!(method, ClassLikeMemberSelector::Identifier(id) if PREFIXES.iter().any(|p| id.value.starts_with(p)))
}

fn parameter_types(
    ctx: &Context<'_>,
    call: &mago_syntax::cst::MethodCall<'_>,
    dynamic: &[String],
) -> Option<Vec<Option<TUnion>>> {
    if call.argument_list.arguments.is_empty() {
        return None;
    }
    let ClassLikeMemberSelector::Identifier(method) = &call.method else {
        return None;
    };
    let facts = facts(ctx);
    let mut classes: Vec<String> = Vec::new();
    for atomic in facts.full_type(node_span(call.object))?.types.iter() {
        object_class_names(atomic, &mut classes);
    }
    if matches!(call.object, Expression::Variable(Variable::Direct(v)) if v.name != b"$this") {
        for class in dynamic {
            if !classes.contains(class) {
                classes.push(class.clone());
            }
        }
    }
    let [class] = &classes[..] else {
        return None;
    };
    let codebase = facts.documented();
    codebase.get_class_like(class.as_bytes())?;
    let metadata = codebase.get_declaring_method(class.as_bytes(), method.value)?;
    Some(
        metadata
            .parameters
            .iter()
            .map(|p| p.type_metadata.as_ref().map(|t| t.type_union.clone()))
            .collect(),
    )
}

fn object_class_names(atomic: &TAtomic, out: &mut Vec<String>) {
    let mut add = |name: String| {
        if !out.contains(&name) {
            out.push(name);
        }
    };
    match atomic {
        TAtomic::Object(TObject::Named(named)) => {
            add(named.name.to_string());
            for member in named.intersection_types.iter().flat_map(|i| i.iter()) {
                if let TAtomic::Object(TObject::Named(n)) = member {
                    add(n.name.to_string());
                }
            }
        }
        TAtomic::Object(TObject::Enum(e)) => add(e.name.to_string()),
        TAtomic::GenericParameter(parameter) => {
            for a in parameter.constraint.types.iter() {
                object_class_names(a, out);
            }
        }
        _ => {}
    }
}

fn is_object_type(t: &TUnion) -> bool {
    match t.types.as_ref() {
        [TAtomic::Object(TObject::Named(named))] => {
            named.intersection_types.is_none() && !named.is_this && !named.is_static
        }
        [TAtomic::Object(TObject::Enum(_))] => true,
        [TAtomic::GenericParameter(parameter)] => is_object_type(&parameter.constraint),
        _ => false,
    }
}

fn insert_before(
    ctx: &crate::Context<'_>,
    list: &List<'_>,
    i: usize,
    asserts: &[&Collected],
) -> Edit {
    let source = ctx.source;
    let indent_of = |pos: usize| {
        let width: usize = line_indent(source, pos)
            .bytes()
            .map(|b| if b == b'\t' { 4 } else { 1 })
            .sum();
        format!("\n{}", " ".repeat(width))
    };
    let printed = asserts.iter().map(|(name, class)| {
        format!(
            "$this->assertInstanceOf(\\{class}::class, ${});",
            String::from_utf8_lossy(name)
        )
    });
    if i == 0 {
        let span = node_span(list.stmts[0]);
        let at = first_comment(source, list.before(ctx, 0), span.start).unwrap_or(span.start);
        let nl = indent_of(span.start);
        let text: String = printed.map(|a| format!("{a}{nl}")).collect();
        return Edit::new(Span::new(at, at), text);
    }
    let previous = node_span(list.stmts[i - 1]);
    let nl = indent_of(previous.start);
    let text: String = printed.map(|a| format!("{nl}{a}")).collect();
    Edit::new(Span::new(previous.end, previous.end), text)
}
