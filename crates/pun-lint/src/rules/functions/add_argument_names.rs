//! Rector 2.8.4's named-args rules: `AddNameToBooleanArgumentRector` and `AddNameToNullArgumentRector`, with
//! `CallLikeArgumentNameAdder`, and `ExplicitAttributeNamedArgsRector`. The parameter names come from the called
//! function's or method's declaration in Pun's codebase (PHP's own functions and classes, and the project's); a call
//! whose declaration Pun cannot resolve is left alone.

use crate::problem::{Edit, Safety, Span};
use mago_codex::identifier::method::MethodIdentifier;
use mago_codex::metadata::function_like::{FunctionLikeKind, FunctionLikeMetadata};
use mago_codex::metadata::parameter::FunctionLikeParameterMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_syntax::cst::{Argument, Expression, Node, NodeKind, PartialArgument};
use mago_word::word;

use crate::rules::complexity::require_named_argument_order::{Target, callee, callee_method};
use crate::rules::support::expressions::is_constant;
use crate::rules::support::names::resolve_class_name;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

/// Internal functions that reflection reports as named-argument capable, but reject named arguments at runtime due to
/// alternate signatures (`CallLikeArgumentNameAdder::NO_NAMED_ARGUMENTS_FUNCTION_NAMES`).
const NO_NAMED_ARGUMENTS_FUNCTION_NAMES: &[&str] = &["session_set_save_handler"];

/// `AddNameToBooleanArgumentRector`.
pub struct AddBooleanArgumentName;

impl Rule for AddBooleanArgumentName {
    fn id(&self) -> &'static str {
        "lint/functions/add-boolean-argument-name"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(CALLS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        add_names_to_args(node, ancestors, ctx, out, "boolean", |e| {
            is_constant(e, "true") || is_constant(e, "false")
        });
    }
}

/// `AddNameToNullArgumentRector`.
pub struct AddNullArgumentName;

impl Rule for AddNullArgumentName {
    fn id(&self) -> &'static str {
        "lint/functions/add-null-argument-name"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(CALLS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        add_names_to_args(node, ancestors, ctx, out, "null", |e| {
            is_constant(e, "null")
        });
    }
}

const CALLS: &[NodeKind] = &[
    NodeKind::FunctionCall,
    NodeKind::MethodCall,
    NodeKind::StaticMethodCall,
    NodeKind::Instantiation,
];

fn parameter_name(parameter: &FunctionLikeParameterMetadata) -> &[u8] {
    let name = parameter.get_name().0.as_bytes();
    name.strip_prefix(b"$").unwrap_or(name)
}

fn name_edit(arg: Span, parameter: &FunctionLikeParameterMetadata) -> Edit {
    let name = String::from_utf8_lossy(parameter_name(parameter));
    Edit::new(Span::new(arg.start, arg.start), format!("{name}: "))
}

/// `CallLikeArgumentNameAdder::resolveParameterReflection()`.
fn resolve_parameter<'m>(
    arg: &Argument<'_>,
    position: usize,
    parameters: &'m [FunctionLikeParameterMetadata],
) -> Option<&'m FunctionLikeParameterMetadata> {
    if let Argument::Named(named) = arg {
        return parameters
            .iter()
            .find(|p| parameter_name(p) == named.name.value);
    }
    if let Some(parameter) = parameters.get(position) {
        return Some(parameter);
    }
    parameters.last().filter(|p| p.flags.is_variadic())
}

/// `CallLikeArgumentNameAdder::addNamesToArgs()`.
fn add_names_to_args<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    ancestors: &[Node<'ast, 'arena>],
    ctx: &Context<'_>,
    out: &mut Vec<Report>,
    kind: &str,
    should_name: impl Fn(&Expression<'_>) -> bool,
) {
    let (list, target) = match node {
        Node::FunctionCall(call) => (&call.argument_list, Target::Function(call.function)),
        Node::MethodCall(call) => (
            &call.argument_list,
            Target::Method(call.object, &call.method),
        ),
        Node::StaticMethodCall(call) => (
            &call.argument_list,
            Target::Static(call.class, &call.method),
        ),
        Node::Instantiation(new) => match &new.argument_list {
            Some(list) => (list, Target::New(new.class)),
            None => return,
        },
        _ => return,
    };
    let args: Vec<&Argument<'_>> = list.arguments.iter().collect();
    // shouldSkip(): no arguments, or an unpacked one
    if args.is_empty()
        || args
            .iter()
            .any(|a| matches!(a, Argument::Positional(p) if p.ellipsis.is_some()))
    {
        return;
    }
    let facts = facts(ctx);
    // Rector resolves no method on a nullable object, nor on a nullsafe call (not a target here)
    if let Target::Method(object, _) = target
        && facts
            .native_type(node_span(object))
            .is_some_and(|t| t.types.iter().any(TAtomic::is_null))
    {
        return;
    }
    let Some(callee) = callee(&facts, target, ancestors) else {
        return;
    };
    // `@no-named-arguments` on the function, the method or its class; only the documented codebase reads docblocks
    let documented = facts.documented();
    let forbids = match callee_method(&facts, target, ancestors) {
        Some((class, method)) => {
            let id = MethodIdentifier::new(word(class.as_bytes()), word(method));
            let declaring = documented.get_declaring_method_identifier(&id);
            documented
                .get_method_by_id(&declaring)
                .is_some_and(|m| m.flags.forbids_named_arguments())
                || [word(class.as_bytes()), declaring.get_class_name()]
                    .iter()
                    .any(|c| {
                        documented
                            .get_class_like(c.as_bytes())
                            .is_some_and(|c| c.flags.forbids_named_arguments())
                    })
        }
        None => documented
            .get_function(callee.name.as_bytes())
            .is_some_and(|f| f.flags.forbids_named_arguments()),
    };
    if forbids || callee.flags.forbids_named_arguments() {
        return;
    }
    if callee.kind == FunctionLikeKind::Function
        && NO_NAMED_ARGUMENTS_FUNCTION_NAMES
            .iter()
            .any(|n| callee.original_name.to_string().eq_ignore_ascii_case(n))
    {
        return;
    }
    let parameters = &callee.parameters;
    // resolveFirstPositionToName()
    let Some(position) = args.iter().enumerate().position(|(i, arg)| {
        matches!(arg, Argument::Positional(p) if should_name(p.value))
            && can_name_from(&args, parameters, i)
    }) else {
        return;
    };
    let mut edits = Vec::new();
    for (i, arg) in args.iter().enumerate().skip(position) {
        if matches!(arg, Argument::Named(_)) {
            continue;
        }
        let Some(parameter) = resolve_parameter(arg, i, parameters) else {
            return;
        };
        edits.push(name_edit(node_span(*arg), parameter));
    }
    if edits.is_empty() {
        return;
    }
    out.push(Report::fixed(
        node_span(&node),
        format!("Name the {kind} argument and the arguments after it."),
        // a subclass may name the parameter differently, and a renamed parameter breaks the call
        Safety::Unsafe,
        edits,
    ));
}

/// `CallLikeArgumentNameAdder::canNameArgsFromPosition()`.
fn can_name_from(
    args: &[&Argument<'_>],
    parameters: &[FunctionLikeParameterMetadata],
    position: usize,
) -> bool {
    args.iter().enumerate().skip(position).all(|(i, arg)| {
        matches!(arg, Argument::Named(_))
            || resolve_parameter(arg, i, parameters).is_some_and(|p| !p.flags.is_variadic())
    })
}

/// `ExplicitAttributeNamedArgsRector`.
pub struct AddAttributeArgumentNames;

impl Rule for AddAttributeArgumentNames {
    fn id(&self) -> &'static str {
        "lint/functions/add-attribute-argument-names"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Attribute])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Attribute(attribute) = node else {
            return;
        };
        let Some(list) = &attribute.argument_list else {
            return;
        };
        if list.arguments.is_empty() {
            return;
        }
        let facts = facts(ctx);
        let class = resolve_class_name(ancestors, &attribute.name);
        let Some(constructor) = facts
            .codebase
            .get_declaring_method(class.as_bytes(), b"__construct")
        else {
            return;
        };
        let Some(edits) = names_to_apply(list.arguments.iter(), constructor) else {
            return;
        };
        out.push(Report::fixed(
            node_span(attribute),
            "Name the attribute's arguments after its constructor's parameters.",
            Safety::Safe,
            edits,
        ));
    }
}

/// `ExplicitAttributeNamedArgsRector::resolveArgNamesToApply()`: nothing when an argument maps to a variadic parameter
/// or to no parameter.
fn names_to_apply<'a>(
    args: impl Iterator<Item = &'a PartialArgument<'a>>,
    constructor: &FunctionLikeMetadata,
) -> Option<Vec<Edit>> {
    let mut edits = Vec::new();
    for (position, arg) in args.enumerate() {
        if matches!(arg, PartialArgument::Named(_)) {
            continue;
        }
        let parameter = constructor.parameters.get(position)?;
        if parameter.flags.is_variadic() {
            return None;
        }
        edits.push(name_edit(node_span(arg), parameter));
    }
    (!edits.is_empty()).then_some(edits)
}
