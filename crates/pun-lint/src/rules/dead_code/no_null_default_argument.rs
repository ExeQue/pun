use crate::problem::{Edit, Fix, Safety, Span};
use mago_codex::metadata::CodebaseMetadata;
use mago_codex::metadata::function_like::FunctionLikeMetadata;
use mago_syntax::cst::{Argument, ArgumentList, Node, NodeKind};

use crate::rules::complexity::require_named_argument_order::{Target, callee};
use crate::rules::support::arguments::{unpacked, value};
use crate::rules::support::expressions::is_constant;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

const TARGETS: &[NodeKind] = &[
    NodeKind::MethodCall,
    NodeKind::StaticMethodCall,
    NodeKind::Instantiation,
    NodeKind::FunctionCall,
];

#[derive(Default)]
pub struct NoNullDefaultArgument {
    /// `null` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for NoNullDefaultArgument {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-redundant-default-argument"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !super::no_redundant_default_argument::checks(options, "null")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(TARGETS)
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
        let Some((list, target)) = call(node) else {
            return;
        };
        let args = list.arguments.as_slice();
        // Only trailing `null`s can go, so a call that does not end in one needs no callee.
        if args.last().is_none_or(|a| !is_constant(value(a), "null"))
            || args
                .iter()
                .any(|a| matches!(a, Argument::Named(_)) || unpacked(a))
        {
            return;
        }
        let facts = facts(ctx);
        let Some(nulls) =
            callee(&facts, target, ancestors).and_then(|c| null_positions(&facts.codebase, c))
        else {
            return;
        };
        let mut removed = Vec::new();
        for (position, arg) in args.iter().enumerate().rev() {
            if !is_constant(value(arg), "null") || !nulls.contains(&position) {
                break;
            }
            removed.push(position);
        }
        report(node, list, removed, out);
    }
}

#[derive(Default)]
pub struct NoNullDefaultNamedArgument {
    /// `null-named` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for NoNullDefaultNamedArgument {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-redundant-default-argument"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !super::no_redundant_default_argument::checks(options, "null-named")?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(TARGETS)
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
        let Some((list, target)) = call(node) else {
            return;
        };
        let args = list.arguments.as_slice();
        // Only a named `null` can go, so a call without one needs no callee.
        if args.iter().any(unpacked)
            || !(args.iter())
                .any(|a| matches!(a, Argument::Named(n) if is_constant(n.value, "null")))
        {
            return;
        }
        let facts = facts(ctx);
        let Some(callee) = callee(&facts, target, ancestors) else {
            return;
        };
        let Some(nulls) = null_positions(&facts.codebase, callee) else {
            return;
        };
        let removed = args
            .iter()
            .enumerate()
            .filter(|(_, arg)| {
                let Argument::Named(named) = arg else {
                    return false;
                };
                is_constant(named.value, "null")
                    && callee
                        .parameters
                        .iter()
                        .position(|p| p.get_name().0.as_bytes().get(1..) == Some(named.name.value))
                        .is_some_and(|p| nulls.contains(&p))
            })
            .map(|(i, _)| i)
            .collect();
        report(node, list, removed, out);
    }
}

fn call<'a>(node: Node<'a, 'a>) -> Option<(&'a ArgumentList<'a>, Target<'a>)> {
    match node {
        Node::MethodCall(c) => Some((&c.argument_list, Target::Method(c.object, &c.method))),
        Node::StaticMethodCall(c) => Some((&c.argument_list, Target::Static(c.class, &c.method))),
        Node::FunctionCall(c) => Some((&c.argument_list, Target::Function(c.function))),
        Node::Instantiation(new) => Some((new.argument_list.as_ref()?, Target::New(new.class))),
        _ => None,
    }
}

fn null_positions(
    codebase: &CodebaseMetadata,
    callee: &FunctionLikeMetadata,
) -> Option<Vec<usize>> {
    let ds_map_get = codebase
        .get_method(b"Ds\\Map", b"get")
        .is_some_and(|m| std::ptr::eq(m, callee));
    if ds_map_get
        || callee.flags.uses_func_get_args()
        || callee.parameters.iter().any(|p| p.flags.is_variadic())
    {
        return None;
    }
    Some(
        callee
            .parameters
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                p.default_type
                    .as_ref()
                    .is_some_and(|d| d.type_union.is_null())
            })
            .map(|(i, _)| i)
            .collect(),
    )
}

fn report(
    node: Node<'_, '_>,
    list: &ArgumentList<'_>,
    mut removed: Vec<usize>,
    out: &mut Vec<Report>,
) {
    if removed.is_empty() {
        return;
    }
    removed.sort_unstable();
    let spans: Vec<Span> = list.arguments.iter().map(node_span).collect();
    let mut edits = Vec::new();
    let mut i = 0;
    while i < removed.len() {
        let first = removed[i];
        let mut last = first;
        while i + 1 < removed.len() && removed[i + 1] == last + 1 {
            i += 1;
            last += 1;
        }
        i += 1;
        let span = if first > 0 {
            Span::new(spans[first - 1].end, spans[last].end)
        } else if let Some(next) = spans.get(last + 1) {
            Span::new(spans[0].start, next.start)
        } else {
            Span::new(spans[0].start, spans[last].end)
        };
        edits.push(Edit::new(span, ""));
    }
    out.push(Report {
        span: node_span(&node),
        message: "Remove the `null` argument: the parameter's default is already `null`."
            .to_string(),
        help: None,
        fix: Some(Fix {
            safety: Safety::Safe,
            edits,
        }),
    });
}
