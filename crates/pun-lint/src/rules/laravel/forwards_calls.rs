//! rector-laravel 2.7.3's `UseForwardsCallsTraitRector` with its `CallUserFuncAnalyzer`: a class that forwards calls
//! with `call_user_func()` or `call_user_func_array()` uses `ForwardsCalls` and `$this->forwardCallTo()` instead.

use mago_syntax::cst::{
    Argument, ArgumentList, ClassLikeMember, Expression, FunctionCall, Node, NodeKind, Sequence,
};

use super::member_name;
use super::names::resolve_class_name;
use super::property_attribute::array_elements;
use crate::problem::{Edit, Safety, Span};
use crate::rules::pest::function_name;
use crate::rules::support::conditions::indentation;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const FORWARD_CALLS_TRAIT: &str = "Illuminate\\Support\\Traits\\ForwardsCalls";

/// `UseForwardsCallsTraitRector`.
pub struct PreferForwardsCalls;

/// The arguments of a `forwardCallTo()` call: the values of `call_user_func()`'s other arguments, the argument array
/// of `call_user_func_array()`, or none.
enum Args {
    List(Vec<Span>),
    Array(Span),
}

/// A `call_user_func()` or `call_user_func_array()` call that becomes `$this->forwardCallTo()`.
struct Forward {
    span: Span,
    object: Span,
    method: Method,
    args: Args,
}

enum Method {
    Expression(Span),
    Name(String),
}

fn arguments<'a>(list: &'a ArgumentList<'a>) -> Vec<&'a Argument<'a>> {
    list.arguments.iter().collect()
}

/// `isCallUserFuncCall()`.
fn is_call_user_func(call: &FunctionCall<'_>) -> Option<bool> {
    let name = function_name(call)?;
    if name.eq_ignore_ascii_case(b"call_user_func") {
        Some(false)
    } else if name.eq_ignore_ascii_case(b"call_user_func_array") {
        Some(true)
    } else {
        None
    }
}

/// `canDetermineMethodFromCallable()`.
fn can_determine_method(call: &FunctionCall<'_>) -> bool {
    arguments(&call.argument_list).first().is_some_and(|a| {
        array_elements(a.value()).is_some() || first_class_method(a.value()).is_some()
    })
}

/// The object and method name of `$object->method(...)`.
fn first_class_method<'a>(
    expr: &'a Expression<'a>,
) -> Option<(&'a Expression<'a>, Option<&'a [u8]>)> {
    let Expression::PartialApplication(mago_syntax::cst::PartialApplication::Method(m)) = expr
    else {
        return None;
    };
    crate::rules::support::class_dependency::is_first_class(&m.argument_list)
        .then(|| (m.object, member_name(&m.method)))
}

/// `getForwardedMethod()`.
fn forwarded(call: &FunctionCall<'_>, array: bool) -> Option<Forward> {
    let args = arguments(&call.argument_list);
    let first = args.first()?.value();
    let args = if array {
        args.get(1).map_or(Args::List(Vec::new()), |a| {
            Args::Array(node_span(a.value()))
        })
    } else {
        Args::List(args[1..].iter().map(|a| node_span(a.value())).collect())
    };
    let (object, method) = if let Some(elements) = array_elements(first) {
        // Rector also asks PHPStan whether the first item is an object, but of the item node, whose type is never
        // known not to be one.
        let values: Vec<_> = elements
            .iter()
            .map(|e| match e {
                mago_syntax::cst::ArrayElement::KeyValue(e) => Some(e.value),
                mago_syntax::cst::ArrayElement::Value(e) => Some(e.value),
                mago_syntax::cst::ArrayElement::Variadic(e) => Some(e.value),
                mago_syntax::cst::ArrayElement::Missing(_) => None,
            })
            .collect();
        let [Some(object), Some(method)] = values[..] else {
            return None;
        };
        (node_span(object), Method::Expression(node_span(method)))
    } else {
        let (object, name) = first_class_method(first)?;
        (
            node_span(object),
            Method::Name(String::from_utf8_lossy(name?).into_owned()),
        )
    };
    Some(Forward {
        span: node_span(call),
        object,
        method,
        args,
    })
}

/// The source of `span`, with each outermost forwarded call inside it rewritten.
fn rewritten(source: &str, span: Span, calls: &[Forward]) -> String {
    let mut out = String::new();
    let mut at = span.start;
    for call in calls {
        if call.span.start < at || call.span.end > span.end || call.span == span {
            continue;
        }
        out.push_str(&source[at..call.span.start]);
        out.push_str(&forward_call(source, call, calls));
        at = call.span.end;
    }
    out.push_str(&source[at..span.end]);
    out
}

fn forward_call(source: &str, call: &Forward, calls: &[Forward]) -> String {
    let part = |span: Span| rewritten(source, span, calls);
    let method = match &call.method {
        Method::Expression(span) => part(*span),
        Method::Name(name) => format!("'{name}'"),
    };
    let args = match &call.args {
        Args::List(values) => format!(
            "[{}]",
            values
                .iter()
                .map(|s| part(*s))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Args::Array(span) => part(*span),
    };
    format!(
        "$this->forwardCallTo({}, {method}, {args})",
        part(call.object)
    )
}

/// `hasCallUserFunctionCalls()` and `refactorFunctionCalls()` over one node's subtree.
fn collect(node: Node<'_, '_>, found: &mut bool, calls: &mut Vec<Forward>) {
    if let Node::FunctionCall(call) = node
        && let Some(array) = is_call_user_func(call)
    {
        *found |= can_determine_method(call);
        calls.extend(forwarded(call, array));
    }
    node.visit_children(|c| collect(c, found, calls));
}

impl Rule for PreferForwardsCalls {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-forwards-calls"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        // Rector rewrites the calls of a class nested in another class with the outer one.
        if ancestors
            .iter()
            .any(|a| matches!(a, Node::Class(_) | Node::AnonymousClass(_)))
        {
            return;
        }
        let (members, left_brace, at): (&Sequence<'_, ClassLikeMember<'_>>, usize, Span) =
            match node {
                Node::Class(c) => (
                    &c.members,
                    c.left_brace.end.offset as usize,
                    node_span(&c.name),
                ),
                Node::AnonymousClass(c) => (
                    &c.members,
                    c.left_brace.end.offset as usize,
                    Span::new(
                        c.new.span.start.offset as usize,
                        c.left_brace.start.offset as usize,
                    ),
                ),
                _ => return,
            };
        let (mut found, mut calls) = (false, Vec::new());
        for member in members.iter() {
            collect(Node::ClassLikeMember(member), &mut found, &mut calls);
        }
        if !found {
            return;
        }
        let source = ctx.source;
        calls.sort_by_key(|c| (c.span.start, std::cmp::Reverse(c.span.end)));
        let mut edits = Vec::new();
        // `addCallForwardingTrait()`
        let uses: Vec<_> = members
            .iter()
            .filter_map(|m| match m {
                ClassLikeMember::TraitUse(u) => Some(u),
                _ => None,
            })
            .collect();
        if let Some(first) = uses.first() {
            let has = uses.iter().flat_map(|u| u.trait_names.iter()).any(|t| {
                resolve_class_name(ancestors, t).eq_ignore_ascii_case(FORWARD_CALLS_TRAIT)
            });
            if !has {
                let end = first
                    .trait_names
                    .iter()
                    .last()
                    .map_or(0, |t| node_span(t).end);
                edits.push(Edit::new(
                    Span::new(end, end),
                    format!(", \\{FORWARD_CALLS_TRAIT}"),
                ));
            }
        } else {
            let inside = &source[left_brace..];
            let start = left_brace + inside.len() - inside.trim_start().len();
            let indent = " ".repeat(indentation(source, start));
            edits.push(Edit::new(
                Span::new(start, start),
                format!("use \\{FORWARD_CALLS_TRAIT};\n{indent}"),
            ));
        }
        let mut end = 0;
        for call in &calls {
            if call.span.start < end {
                continue;
            }
            edits.push(Edit::new(call.span, forward_call(source, call, &calls)));
            end = call.span.end;
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report::fixed(
            at,
            "Forward the calls with the `ForwardsCalls` trait.",
            Safety::Unsafe,
            edits,
        ));
    }
}
