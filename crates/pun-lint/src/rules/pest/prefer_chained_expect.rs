use mago_syntax::cst::{
    Access, ArgumentList, ArrayElement, Call, ClassLikeConstantSelector, ClassLikeMemberSelector,
    CompositeString, Expression, Literal, MethodCall, Node, NodeKind, PropertyAccess, StringPart,
    TokenSeparatedSequence,
};
use pun_problem::{Safety, Span};

use super::{
    Item, Link, STATEMENT_LISTS, chain_links, expect_argument, has_comments, identifier,
    is_expect_chain, kept_text, method_call, print_args, print_member, print_nodes, rewrite_list,
    statement_expression, statement_list,
};
use crate::syntax::{node_span, same_expression, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct PreferChainedExpect {
    separate_variables: bool,
}

const SUBJECT_TRANSFORMING_METHODS: [&[u8]; 7] = [
    b"and",
    b"json",
    b"each",
    b"match",
    b"sequence",
    b"unless",
    b"when",
];

#[derive(Clone)]
enum Chain<'a> {
    Old(&'a MethodCall<'a>),
    Call {
        var: Box<Chain<'a>>,
        call: &'a MethodCall<'a>,
        newline: bool,
    },
    And {
        var: Box<Chain<'a>>,
        value: &'a Expression<'a>,
        newline: bool,
    },
    Property {
        var: Box<Chain<'a>>,
        fetch: &'a PropertyAccess<'a>,
    },
}

struct Stmt<'a> {
    index: usize,
    call: &'a MethodCall<'a>,
    chain: Option<Chain<'a>>,
}

impl Rule for PreferChainedExpect {
    fn id(&self) -> &'static str {
        "lint/pest/prefer-chained-expect"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(STATEMENT_LISTS)
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        match options {
            serde_json::Value::Null => Ok(()),
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    match (key.as_str(), value.as_bool()) {
                        ("merge-different-variables", Some(merge)) => {
                            self.separate_variables = !merge;
                        }
                        _ => {
                            return Err(format!(
                                "unknown option `{key}` or a value of the wrong type"
                            )
                            .into());
                        }
                    }
                }
                Ok(())
            }
            _ => Err("options must be a table".into()),
        }
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(list) = statement_list(ctx, node) else {
            return;
        };
        let mut stmts: Vec<Option<Stmt<'_>>> = list
            .stmts
            .iter()
            .enumerate()
            .map(|(index, s)| {
                let call = statement_expression(s).and_then(method_call)?;
                Some(Stmt {
                    index,
                    call,
                    chain: None,
                })
            })
            .collect();
        let mut order: Vec<usize> = (0..list.stmts.len()).collect();
        let starts: Vec<usize> = list.stmts.iter().map(|s| node_span(*s).start).collect();
        let commented = |s: &Stmt<'_>| has_comments(ctx, starts[s.index]);
        let merge = !self.separate_variables;
        let mut changed = false;
        loop {
            let mut changed_in_pass = false;
            for key in 0..stmts.len() {
                let (Some(Some(current)), Some(Some(next))) = (stmts.get(key), stmts.get(key + 1))
                else {
                    continue;
                };
                if !is_expect_chain(current.call) || !is_expect_chain(next.call) {
                    continue;
                }
                let (Some(first), Some(next_arg)) =
                    (expect_argument(current.call), expect_argument(next.call))
                else {
                    continue;
                };
                if commented(current) || commented(next) {
                    continue;
                }
                if same_expression(ctx.source, first, next_arg) {
                    let transforms_subject = names(current)
                        .iter()
                        .any(|n| n.is_some_and(|n| SUBJECT_TRANSFORMING_METHODS.contains(&n)));
                    if transforms_subject && !merge {
                        continue;
                    }
                    if !transforms_subject && !is_side_effect_free(first) && !merge {
                        continue;
                    }
                    if !transforms_subject && is_side_effect_free(first) {
                        let second = stmts.remove(key + 1).unwrap();
                        order.remove(key + 1);
                        let current = stmts[key].as_mut().unwrap();
                        let base = current.chain.take().unwrap_or(Chain::Old(current.call));
                        let mut chain = rebuild(base, &chain_links(second.call));
                        newlines(&mut chain);
                        current.chain = Some(chain);
                        changed_in_pass = true;
                        break;
                    }
                }
                if merge {
                    merge_different(ctx, &mut stmts, &mut order, key, &commented);
                    changed_in_pass = true;
                    break;
                }
            }
            changed |= changed_in_pass;
            if !changed_in_pass {
                break;
            }
        }
        if !changed {
            return;
        }
        let mut items: Vec<Item<'_>> = Vec::new();
        for (entry, index) in stmts.iter().zip(&order) {
            match entry
                .as_ref()
                .and_then(|s| s.chain.as_ref().map(|c| (s.call, c)))
            {
                Some((call, chain)) => {
                    let span = node_span(list.stmts[*index]);
                    let indent = super::indent_at(ctx, span.start);
                    let expr = node_span(call);
                    let text = format!(
                        "{}{}{}",
                        &ctx.source[span.start..expr.start],
                        print(ctx, chain, indent),
                        &ctx.source[expr.end..span.end]
                    );
                    items.push(Item::Changed(*index, text));
                }
                None => items.push(Item::Old(*index)),
            }
        }
        if list.nop {
            items.push(Item::Old(list.stmts.len()));
        }
        if let Some(edits) = rewrite_list(ctx, node, ancestors, &list, &items) {
            out.push(Report::fixed(
                node_span(list.stmts[0]),
                "Chain consecutive `expect()` calls.",
                Safety::Safe,
                edits,
            ));
        }
    }
}

fn merge_different<'a>(
    ctx: &Context<'_>,
    stmts: &mut Vec<Option<Stmt<'a>>>,
    order: &mut Vec<usize>,
    key: usize,
    commented: &dyn Fn(&Stmt<'_>) -> bool,
) {
    let target = expect_argument(stmts[key + 1].as_ref().unwrap().call).unwrap();
    let target_is_side_effect_free = is_side_effect_free(target);
    let mut methods = Vec::new();
    while let Some(Some(current)) = stmts.get(key + 1) {
        if !methods.is_empty() && !target_is_side_effect_free {
            break;
        }
        if commented(current) || !is_expect_chain(current.call) {
            break;
        }
        if !expect_argument(current.call).is_some_and(|a| same_expression(ctx.source, target, a)) {
            break;
        }
        methods.extend(chain_links(current.call));
        stmts.remove(key + 1);
        order.remove(key + 1);
    }
    let current = stmts[key].as_mut().unwrap();
    let base = current.chain.take().unwrap_or(Chain::Old(current.call));
    let and = Chain::And {
        var: Box::new(base),
        value: target,
        newline: false,
    };
    let mut chain = rebuild(and, &methods);
    newlines(&mut chain);
    current.chain = Some(chain);
}

fn is_side_effect_free(expr: &Expression<'_>) -> bool {
    let parts_free = |parts: &[StringPart<'_>]| {
        parts.iter().all(|part| match part {
            StringPart::Literal(_) => true,
            StringPart::Expression(e) => is_side_effect_free(e),
            StringPart::BracedExpression(b) => is_side_effect_free(b.expression),
        })
    };
    match unparenthesize(expr) {
        Expression::CompositeString(CompositeString::Interpolated(s)) => {
            parts_free(s.parts.as_slice())
        }
        Expression::CompositeString(CompositeString::Document(d)) => parts_free(d.parts.as_slice()),
        Expression::Variable(_)
        | Expression::Literal(_)
        | Expression::MagicConstant(_)
        | Expression::ConstantAccess(_)
        | Expression::Access(Access::ClassConstant(_) | Access::StaticProperty(_)) => true,
        Expression::Access(Access::Property(p)) => is_side_effect_free(p.object),
        Expression::Access(Access::NullSafeProperty(p)) => is_side_effect_free(p.object),
        Expression::ArrayAccess(a) => is_side_effect_free(a.array) && is_side_effect_free(a.index),
        Expression::ArrayAppend(a) => is_side_effect_free(a.array),
        Expression::Array(a) => elements_free(&a.elements),
        Expression::LegacyArray(a) => elements_free(&a.elements),
        _ => false,
    }
}

fn elements_free(elements: &TokenSeparatedSequence<'_, ArrayElement<'_>>) -> bool {
    elements.iter().all(|element| match element {
        ArrayElement::KeyValue(e) => is_side_effect_free(e.value),
        ArrayElement::Value(e) => is_side_effect_free(e.value),
        ArrayElement::Variadic(e) => is_side_effect_free(e.value),
        ArrayElement::Missing(_) => true,
    })
}

fn rebuild<'a>(base: Chain<'a>, methods: &[Link<'a>]) -> Chain<'a> {
    methods.iter().fold(base, |var, link| match link {
        Link::Method(call) => Chain::Call {
            var: Box::new(var),
            call,
            newline: false,
        },
        Link::Property(fetch) => Chain::Property {
            var: Box::new(var),
            fetch,
        },
    })
}

fn names<'a>(statement: &Stmt<'a>) -> Vec<Option<&'a [u8]>> {
    fn walk<'a>(chain: &Chain<'a>, out: &mut Vec<Option<&'a [u8]>>) {
        match chain {
            Chain::Old(call) => out.extend(chain_links(call).iter().map(|l| link_name(l))),
            Chain::Call { var, call, .. } => {
                walk(var, out);
                out.push(identifier(&call.method));
            }
            Chain::And { var, .. } => {
                walk(var, out);
                out.push(Some(b"and"));
            }
            Chain::Property { var, fetch } => {
                walk(var, out);
                out.push(identifier(&fetch.property));
            }
        }
    }
    let mut out = Vec::new();
    match &statement.chain {
        Some(chain) => walk(chain, &mut out),
        None => walk(&Chain::Old(statement.call), &mut out),
    }
    out
}

fn link_name<'a>(link: &Link<'a>) -> Option<&'a [u8]> {
    match link {
        Link::Method(call) => identifier(&call.method),
        Link::Property(fetch) => identifier(&fetch.property),
    }
}

fn newlines(current: &mut Chain<'_>) {
    let (var, flag) = match current {
        Chain::Call { var, newline, .. } | Chain::And { var, newline, .. } => (var, newline),
        Chain::Property { .. } | Chain::Old(_) => return,
    };
    match &mut **var {
        Chain::Old(call) => {
            if !is_old_and(call) {
                *flag = true;
            }
        }
        Chain::Property { var, .. } => newlines(var),
        Chain::And { newline, var, .. } => {
            *newline = true;
            newlines(var);
        }
        Chain::Call {
            call,
            newline,
            var: below,
        } => {
            if is_old_and(call) {
                *newline = true;
                newlines(below);
            } else {
                *flag = true;
                newlines(var);
            }
        }
    }
}

fn print_value(ctx: &Context<'_>, value: &Expression<'_>, indent: usize) -> String {
    let kept = |e: &Expression<'_>| kept_text(ctx, node_span(unparenthesize(e)), indent);
    let lhs = |e: &Expression<'_>, parens: fn(&Expression<'_>) -> bool| {
        let e = unparenthesize(e);
        if parens(e) {
            format!("({})", kept(e))
        } else {
            kept(e)
        }
    };
    let args = |list: &ArgumentList<'_>| print_args(ctx, list, indent);
    let constant = |c: &ClassLikeConstantSelector<'_>| match c {
        ClassLikeConstantSelector::Expression(e) => format!("{{{}}}", kept(e.expression)),
        _ => kept_text(ctx, node_span(c), indent),
    };
    match unparenthesize(value) {
        Expression::Call(Call::Function(c)) => format!(
            "{}({})",
            lhs(c.function, call_lhs_requires_parens),
            args(&c.argument_list)
        ),
        Expression::Call(Call::Method(c)) => format!(
            "{}->{}({})",
            lhs(c.object, dereference_lhs_requires_parens),
            print_member(ctx, &c.method, indent),
            args(&c.argument_list)
        ),
        Expression::Call(Call::NullSafeMethod(c)) => format!(
            "{}?->{}({})",
            lhs(c.object, dereference_lhs_requires_parens),
            print_member(ctx, &c.method, indent),
            args(&c.argument_list)
        ),
        Expression::Call(Call::StaticMethod(c)) => format!(
            "{}::{}({})",
            lhs(c.class, static_dereference_lhs_requires_parens),
            match &c.method {
                ClassLikeMemberSelector::Variable(v) => kept_text(ctx, node_span(v), indent),
                method => print_member(ctx, method, indent),
            },
            args(&c.argument_list)
        ),
        Expression::Access(Access::Property(a)) => format!(
            "{}->{}",
            lhs(a.object, dereference_lhs_requires_parens),
            print_member(ctx, &a.property, indent)
        ),
        Expression::Access(Access::NullSafeProperty(a)) => format!(
            "{}?->{}",
            lhs(a.object, dereference_lhs_requires_parens),
            print_member(ctx, &a.property, indent)
        ),
        Expression::Access(Access::StaticProperty(a)) => format!(
            "{}::{}",
            lhs(a.class, static_dereference_lhs_requires_parens),
            kept_text(ctx, node_span(&a.property), indent)
        ),
        Expression::Access(Access::ClassConstant(a)) => format!(
            "{}::{}",
            lhs(a.class, static_dereference_lhs_requires_parens),
            constant(&a.constant)
        ),
        Expression::ArrayAccess(a) => format!(
            "{}[{}]",
            lhs(a.array, dereference_lhs_requires_parens),
            kept(a.index)
        ),
        Expression::Instantiation(n) => format!(
            "new {}({})",
            lhs(n.class, new_operand_requires_parens),
            n.argument_list.as_ref().map_or_default(args)
        ),
        Expression::Array(a) => format!(
            "[{}]",
            print_nodes(ctx, &element_spans(&a.elements), true, indent)
        ),
        Expression::LegacyArray(a) => format!(
            "array({})",
            print_nodes(ctx, &element_spans(&a.elements), true, indent)
        ),
        other => kept(other),
    }
}

fn element_spans(elements: &TokenSeparatedSequence<'_, ArrayElement<'_>>) -> Vec<Span> {
    elements.iter().map(node_span).collect()
}

fn is_name_node(e: &Expression<'_>) -> bool {
    matches!(
        e,
        Expression::Identifier(_)
            | Expression::Static(_)
            | Expression::Self_(_)
            | Expression::Parent(_)
    )
}

fn call_lhs_requires_parens(e: &Expression<'_>) -> bool {
    !(is_name_node(e)
        || matches!(
            e,
            Expression::Variable(_)
                | Expression::ArrayAccess(_)
                | Expression::Call(_)
                | Expression::Array(_)
                | Expression::LegacyArray(_)
        ))
}

fn dereference_lhs_requires_parens(e: &Expression<'_>) -> bool {
    static_dereference_lhs_requires_parens(e) && !matches!(e, Expression::ConstantAccess(_))
}

fn static_dereference_lhs_requires_parens(e: &Expression<'_>) -> bool {
    !(is_name_node(e)
        || matches!(
            e,
            Expression::Variable(_)
                | Expression::ArrayAccess(_)
                | Expression::Access(_)
                | Expression::Call(_)
                | Expression::Array(_)
                | Expression::LegacyArray(_)
                | Expression::Literal(Literal::String(_))
        ))
}

fn new_operand_requires_parens(e: &Expression<'_>) -> bool {
    match unparenthesize(e) {
        e if is_name_node(e) => false,
        Expression::Variable(_) => false,
        Expression::ArrayAccess(a) => new_operand_requires_parens(a.array),
        Expression::Access(Access::Property(a)) => new_operand_requires_parens(a.object),
        Expression::Access(Access::NullSafeProperty(a)) => new_operand_requires_parens(a.object),
        Expression::Access(Access::StaticProperty(a)) => new_operand_requires_parens(a.class),
        _ => true,
    }
}

fn is_old_and(call: &MethodCall<'_>) -> bool {
    super::is_name(&call.method, "and")
}

fn print(ctx: &Context<'_>, chain: &Chain<'_>, indent: usize) -> String {
    let call_like = |c: &Chain<'_>| !matches!(c, Chain::Property { .. });
    let link = |var: &Chain<'_>, newline: bool, name: String, args: String| {
        let sep = if newline && call_like(var) {
            format!("\n{}", " ".repeat(indent + 4))
        } else {
            String::new()
        };
        format!("{}{sep}->{name}({args})", print(ctx, var, indent))
    };
    match chain {
        Chain::Old(call) => kept_text(ctx, node_span(*call), indent),
        Chain::Call { var, call, newline } => link(
            var,
            *newline,
            print_member(ctx, &call.method, indent),
            print_args(ctx, &call.argument_list, indent),
        ),
        Chain::And {
            var,
            value,
            newline,
        } => link(
            var,
            *newline,
            "and".to_string(),
            print_value(ctx, value, indent),
        ),
        Chain::Property { var, fetch } => format!(
            "{}->{}",
            print(ctx, var, indent),
            print_member(ctx, &fetch.property, indent)
        ),
    }
}
