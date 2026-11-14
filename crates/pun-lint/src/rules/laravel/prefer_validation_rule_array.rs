use mago_allocator::{Arena, LocalArena};
use mago_syntax::cst::{
    ArrayElement, ClassLikeMember, CompositeString, DocumentIndentation, DocumentKind,
    DocumentString, Expression, Literal, LiteralStringKind, MethodBody, Node, NodeKind, Statement,
    StringPart,
};
use mago_syntax_core::utils::parse_literal_string_in;
use pun_problem::{Edit, Safety};

use super::object_type::is_object_type;
use super::{is_a, is_name, member_name};
use crate::rules::complexity::prefer_absolute_include::print_string;
use crate::rules::support::class_members::qualified;
use crate::rules::support::expressions::text;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferValidationRuleArray;

impl Rule for PreferValidationRuleArray {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-validation-rule-array"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::MethodCall,
            NodeKind::StaticMethodCall,
            NodeKind::Class,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let rules = match node {
            Node::MethodCall(call) => {
                let Some(name) = member_name(&call.method) else {
                    return;
                };
                let args = call.argument_list.arguments.as_slice();
                if is_name(name, "validate") {
                    if !is_object_type(ctx, ancestors, call.object, "Illuminate\\Http\\Request") {
                        return;
                    }
                    let Some(first) = args.first() else { return };
                    first.value()
                } else if is_name(name, "make") {
                    let [_, rules] = args else { return };
                    if !is_object_type(
                        ctx,
                        ancestors,
                        call.object,
                        "Illuminate\\Validation\\Factory",
                    ) {
                        return;
                    }
                    rules.value()
                } else {
                    return;
                }
            }
            Node::StaticMethodCall(call) => {
                if !member_name(&call.method).is_some_and(|n| is_name(n, "make")) {
                    return;
                }
                let [_, rules] = call.argument_list.arguments.as_slice() else {
                    return;
                };
                if !is_object_type(
                    ctx,
                    ancestors,
                    call.class,
                    "Illuminate\\Support\\Facades\\Validator",
                ) {
                    return;
                }
                rules.value()
            }
            _ => {
                let Node::Class(class) = node else { return };
                let name = qualified(ancestors, class.name.value);
                if !is_a(ctx, &name, &["Illuminate\\Foundation\\Http\\FormRequest"]) {
                    return;
                }
                let mut edits = Vec::new();
                for member in &class.members {
                    let ClassLikeMember::Method(method) = member else {
                        continue;
                    };
                    if !is_name(method.name.value, "rules") {
                        continue;
                    }
                    let MethodBody::Concrete(body) = &method.body else {
                        continue;
                    };
                    edits.extend(first_return_edits(ctx, Node::Block(body)));
                }
                report(node, edits, out);
                return;
            }
        };
        report(node, rule_edits(ctx, unparenthesize(rules)), out);
    }
}

fn report(node: Node<'_, '_>, edits: Vec<Edit>, out: &mut Vec<Report>) {
    if edits.is_empty() {
        return;
    }
    out.push(Report::fixed(
        node_span(&node),
        "Write each validation rule as an item of an array instead of joining them with `|`.",
        Safety::Safe,
        edits,
    ));
}

fn first_return_edits(ctx: &Context<'_>, node: Node<'_, '_>) -> Vec<Edit> {
    let (mut stack, mut children) = (vec![node], Vec::new());
    while let Some(node) = stack.pop() {
        if let Node::Statement(Statement::Return(r)) = node
            && let Some(value) = r.value
        {
            let edits = rule_edits(ctx, unparenthesize(value));
            if !edits.is_empty() {
                return edits;
            }
        }
        node.visit_children(|child| children.push(child));
        stack.extend(children.drain(..).rev());
    }
    Vec::new()
}

fn rule_edits(ctx: &Context<'_>, rules: &Expression<'_>) -> Vec<Edit> {
    let elements = match rules {
        Expression::Array(a) => &a.elements,
        Expression::LegacyArray(a) => &a.elements,
        _ => return Vec::new(),
    };
    let mut edits = Vec::new();
    for element in elements {
        let ArrayElement::KeyValue(item) = element else {
            continue;
        };
        let value = item.value;
        let items = match value {
            Expression::Literal(Literal::String(s)) => s.value.and_then(string_rules),
            Expression::CompositeString(CompositeString::Interpolated(s)) => {
                let parts = (s.parts.iter())
                    .map(|p| match p {
                        StringPart::Literal(l) => l.value.map(|v| Part::Literal(v.to_vec())),
                        StringPart::Expression(e) => Some(Part::Expression(text(ctx, *e))),
                        StringPart::BracedExpression(b) => {
                            Some(Part::Expression(text(ctx, b.expression)))
                        }
                    })
                    .collect::<Option<Vec<_>>>();
                parts.and_then(|p| interpolated_rules(&p))
            }
            Expression::CompositeString(CompositeString::Document(d)) => {
                match (document_value(ctx, d), document_parts(ctx, d)) {
                    (Some(value), _) => string_rules(&value),
                    (None, parts) => parts.and_then(|p| interpolated_rules(&p)),
                }
            }
            _ => continue,
        };
        if let Some(items) = items {
            edits.push(Edit::new(
                node_span(value),
                format!("[{}]", items.join(", ")),
            ));
        }
    }
    edits
}

enum Part<'s> {
    Literal(Vec<u8>),
    Expression(&'s str),
}

fn string_rules(value: &[u8]) -> Option<Vec<String>> {
    (value.split(|b| *b == b'|'))
        .map(|rule| print_string(LiteralStringKind::SingleQuoted, rule))
        .collect()
}

fn interpolated_rules(parts: &[Part<'_>]) -> Option<Vec<String>> {
    let mut rules: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for part in parts {
        match part {
            Part::Literal(value) => {
                for (index, segment) in value.split(|b| *b == b'|').enumerate() {
                    if index != 0 && !current.is_empty() {
                        rules.push(std::mem::take(&mut current));
                    }
                    if !segment.is_empty() {
                        let quoted = print_string(LiteralStringKind::DoubleQuoted, segment)?;
                        current.push(quoted[1..quoted.len() - 1].to_string());
                    }
                }
            }
            Part::Expression(code) => current.push(format!("{{{code}}}")),
        }
    }
    if !current.is_empty() {
        rules.push(current);
    }
    Some(
        rules
            .iter()
            .map(|r| format!("\"{}\"", r.concat()))
            .collect(),
    )
}

pub(crate) fn document_value(ctx: &Context<'_>, doc: &DocumentString<'_>) -> Option<Vec<u8>> {
    let mut value = Vec::new();
    for part in document_parts(ctx, doc)? {
        match part {
            Part::Literal(v) => value.extend(v),
            Part::Expression(_) => return None,
        }
    }
    Some(value)
}

fn document_parts<'s>(ctx: &Context<'s>, doc: &DocumentString<'_>) -> Option<Vec<Part<'s>>> {
    let indent = match doc.indentation {
        DocumentIndentation::Whitespace(n) | DocumentIndentation::Tab(n) => n,
        DocumentIndentation::None | DocumentIndentation::Mixed(..) => 0,
    };
    let mut out = Vec::new();
    for (i, part) in doc.parts.iter().enumerate() {
        let literal = match part {
            StringPart::Literal(l) => l,
            StringPart::Expression(e) => {
                out.push(Part::Expression(text(ctx, *e)));
                continue;
            }
            StringPart::BracedExpression(b) => {
                out.push(Part::Expression(text(ctx, b.expression)));
                continue;
            }
        };
        let raw = strip_indentation(literal.raw, indent, i == 0);
        let value = match doc.kind {
            DocumentKind::Nowdoc => raw,
            DocumentKind::Heredoc => {
                let mut kept = Vec::with_capacity(raw.len());
                let mut backslashes = 0;
                for &b in &raw {
                    if b == b'"' && backslashes % 2 == 1 {
                        kept.push(b'\\');
                    }
                    backslashes = if b == b'\\' { backslashes + 1 } else { 0 };
                    kept.push(b);
                }
                let arena = LocalArena::new();
                let kept = arena.alloc_slice_copy(&kept);
                parse_literal_string_in(&arena, kept, Some(b'"'), false)?.to_vec()
            }
        };
        if !value.is_empty() {
            out.push(Part::Literal(value));
        }
    }
    Some(out)
}

fn strip_indentation(raw: &[u8], indent: usize, at_line_start: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut line_start = at_line_start;
    let mut i = 0;
    while i < raw.len() {
        if line_start {
            let blanks = raw[i..]
                .iter()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            i += blanks.min(indent);
            line_start = false;
            continue;
        }
        out.push(raw[i]);
        line_start = raw[i] == b'\n';
        i += 1;
    }
    out
}
