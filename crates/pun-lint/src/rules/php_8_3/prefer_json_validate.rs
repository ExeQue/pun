use mago_syntax::cst::{Argument, BinaryOperator, Call, Expression, FunctionCall, Node, NodeKind};
use pun_problem::{Edit, Fix, Safety, Span};

use crate::rules::support::arguments::{remove_argument, unpacked, value as arg_value};
use crate::rules::support::expressions::is_constant;
use crate::rules::support::names::function_call_matches;
use crate::rules::support::values::{Value, value};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

/// Rector's `JsonValidateRector::JSON_MAX_DEPTH`.
const JSON_MAX_DEPTH: i64 = 0x7FFF_FFFF;
/// PHP's `JSON_INVALID_UTF8_IGNORE`.
const JSON_INVALID_UTF8_IGNORE: i64 = 1_048_576;

pub struct PreferJsonValidate;

impl Rule for PreferJsonValidate {
    fn id(&self) -> &'static str {
        "lint/php-8.3/prefer-json-validate"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80300)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Binary])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Binary(and) = node else { return };
        if !matches!(and.operator, BinaryOperator::And(_)) {
            return;
        }
        let Some(call) = match_json_validate_arg(ancestors, and.lhs, and.rhs) else {
            return;
        };
        let args = call.argument_list.arguments.as_slice();
        if args.is_empty() || !validate_args(args) {
            return;
        }
        let span = node_span(&node);
        let call_span = node_span(call);
        let mut edits = vec![Edit::new(node_span(call.function), "json_validate")];
        for around in [
            Span::new(span.start, call_span.start),
            Span::new(call_span.end, span.end),
        ] {
            if around.start < around.end {
                edits.push(Edit::new(around, ""));
            }
        }
        // Remove the associative argument (position 1 or named): json_validate() has no such parameter.
        let associative = args.iter().enumerate().position(|(index, arg)| match arg {
            Argument::Named(named) => named.name.value == b"associative",
            Argument::Positional(_) => index == 1,
        });
        if let Some(index) = associative {
            edits.push(match args.len() {
                1 => Edit::new(node_span(&args[0]), ""),
                _ => remove_argument(&call.argument_list, index),
            });
        }
        out.push(Report {
            span,
            message:
                "Use `json_validate()` instead of `json_decode()` and a `json_last_error()` check."
                    .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}

/// `json_decode(...) !== null && json_last_error() === JSON_ERROR_NONE`, either side of each comparison: the
/// `json_decode()` call.
fn match_json_validate_arg<'a>(
    ancestors: &[Node<'_, '_>],
    left: &'a Expression<'a>,
    right: &'a Expression<'a>,
) -> Option<&'a FunctionCall<'a>> {
    let Expression::Binary(left) = unparenthesize(left) else {
        return None;
    };
    if !matches!(left.operator, BinaryOperator::NotIdentical(_)) {
        return None;
    }
    let decode = two_node_match(
        left.lhs,
        left.rhs,
        |e| call(ancestors, e, "json_decode").is_some(),
        |e| is_constant(e, "null"),
    )?;
    let Expression::Binary(right) = unparenthesize(right) else {
        return None;
    };
    if !matches!(right.operator, BinaryOperator::Identical(_)) {
        return None;
    }
    two_node_match(
        right.lhs,
        right.rhs,
        |e| call(ancestors, e, "json_last_error").is_some(),
        |e| is_constant(e, "JSON_ERROR_NONE"),
    )?;
    call(ancestors, decode, "json_decode")
}

/// `BinaryOpManipulator::matchFirstAndSecondConditionNode`: the operand that meets `first` while the other meets
/// `second`, trying left first.
fn two_node_match<'a>(
    lhs: &'a Expression<'a>,
    rhs: &'a Expression<'a>,
    first: impl Fn(&'a Expression<'a>) -> bool,
    second: impl Fn(&'a Expression<'a>) -> bool,
) -> Option<&'a Expression<'a>> {
    if first(lhs) && second(rhs) {
        return Some(lhs);
    }
    (first(rhs) && second(lhs)).then_some(rhs)
}

fn call<'a>(
    ancestors: &[Node<'_, '_>],
    expr: &'a Expression<'a>,
    name: &str,
) -> Option<&'a FunctionCall<'a>> {
    match unparenthesize(expr) {
        Expression::Call(Call::Function(call)) if function_call_matches(ancestors, call, name) => {
            Some(call)
        }
        _ => None,
    }
}

fn validate_args(args: &[Argument<'_>]) -> bool {
    // Rector's ValueResolver resolves a constant other than true, false and null to its name, never its value.
    if let Some(flags) = get_arg(args, b"flags", 3)
        && value(flags) != Some(Value::Int(JSON_INVALID_UTF8_IGNORE))
    {
        return false;
    }
    if let Some(depth) = get_arg(args, b"depth", 2) {
        // An unresolved depth is null to Rector, and `null <= 0`.
        let Some(Value::Int(depth)) = value(depth) else {
            return false;
        };
        if depth <= 0 || depth > JSON_MAX_DEPTH {
            return false;
        }
    }
    true
}

/// PhpParser's `CallLike::getArg($name, $position)`: the argument named `name`, or the unnamed one at `position`,
/// skipping unpacked ones.
fn get_arg<'a>(
    args: &'a [Argument<'a>],
    name: &[u8],
    position: usize,
) -> Option<&'a Expression<'a>> {
    args.iter()
        .enumerate()
        .find(|(index, arg)| {
            !unpacked(arg)
                && match arg {
                    Argument::Named(named) => named.name.value == name,
                    Argument::Positional(_) => *index == position,
                }
        })
        .map(|(_, arg)| arg_value(arg))
}
