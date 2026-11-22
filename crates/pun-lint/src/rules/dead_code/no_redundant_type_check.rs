use crate::problem::{Edit, Safety};
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_syntax::cst::{
    BinaryOperator, Call, Expression, FunctionCall, Identifier, Node, NodeKind, Variable,
};

use crate::rules::dead_code::no_useless_cast::{names_variable, within};
use crate::rules::support::expressions::{args, equal, is_constant, text};
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoRedundantTypeCheck;

impl Rule for NoRedundantTypeCheck {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-redundant-type-check"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Binary])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Binary(binary) = node else { return };
        let (left, right) = (unparenthesize(binary.lhs), unparenthesize(binary.rhs));
        let keep = match binary.operator {
            BinaryOperator::Or(_) => refactor_boolean_or(ctx, ancestors, left, right),
            BinaryOperator::And(_) => refactor_boolean_and(ctx, ancestors, left, right),
            _ => false,
        };
        if !keep {
            return;
        }
        let span = node_span(binary);
        out.push(Report::fixed(
            span,
            "This type check is always true for the variable's type; remove it.",
            Safety::Unsafe,
            vec![Edit::new(span, text(ctx, left).to_string())],
        ));
    }
}

fn refactor_boolean_or(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    left: &Expression<'_>,
    right: &Expression<'_>,
) -> bool {
    let Expression::Binary(identical) = left else {
        return false;
    };
    if !matches!(identical.operator, BinaryOperator::Identical(_)) {
        return false;
    }
    let compared = if is_constant(identical.lhs, "null") {
        identical.rhs
    } else if is_constant(identical.rhs, "null") {
        identical.lhs
    } else {
        return false;
    };
    let compared = unparenthesize(compared);
    if !matches!(compared, Expression::Variable(Variable::Direct(_))) {
        return false;
    }
    let Expression::UnaryPrefix(not) = right else {
        return false;
    };
    if !not.operator.is_not() {
        return false;
    }
    let Some((name, _)) = match_type_check_func_call(ctx, not.operand, compared) else {
        return false;
    };
    let facts = facts(ctx);
    let Some(native) = native_type(&facts, ancestors, compared) else {
        return false;
    };
    let (nulls, rest): (Vec<&TAtomic>, Vec<&TAtomic>) = native
        .iter()
        .partition(|atomic| matches!(atomic, TAtomic::Null));
    !nulls.is_empty() && is_always_matching_type(name, &rest)
}

fn refactor_boolean_and(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    left: &Expression<'_>,
    right: &Expression<'_>,
) -> bool {
    if !matches!(left, Expression::Variable(Variable::Direct(_))) {
        return false;
    }
    let Some((name, argument)) = match_type_check_func_call(ctx, right, left) else {
        return false;
    };
    let facts = facts(ctx);
    if native_type(&facts, ancestors, left).is_none() {
        return false;
    }
    facts
        .native_type(node_span(argument))
        .is_some_and(|checked| {
            is_always_matching_type(name, &checked.types.iter().collect::<Vec<_>>())
        })
}

fn native_type<'f>(
    facts: &'f Facts,
    ancestors: &[Node<'_, '_>],
    variable: &Expression<'_>,
) -> Option<&'f [TAtomic]> {
    let span = node_span(variable);
    let native = facts.native_type(span)?;
    let doubted = native.possibly_undefined()
        || facts.full_type(span).is_some_and(|full| {
            if native.types.len() > 1 {
                full.types.len() != native.types.len()
                    || !full.types.iter().all(|a| native.types.contains(a))
            } else {
                !within(full, native)
            }
        })
        || names_variable(ancestors, span.start);
    (!doubted).then_some(native.types.as_ref())
}

fn match_type_check_func_call<'a>(
    ctx: &Context<'_>,
    expr: &'a Expression<'a>,
    expected: &Expression<'_>,
) -> Option<(&'a [u8], &'a Expression<'a>)> {
    let Expression::Call(Call::Function(call)) = unparenthesize(expr) else {
        return None;
    };
    let arguments = args(call);
    let [argument] = arguments.as_slice() else {
        return None;
    };
    if !equal(ctx, argument.value(), expected) {
        return None;
    }
    Some((function_name(call)?, unparenthesize(argument.value())))
}

fn function_name<'a>(call: &'a FunctionCall<'a>) -> Option<&'a [u8]> {
    match call.function {
        Expression::Identifier(Identifier::Local(id)) => Some(id.value),
        Expression::Identifier(Identifier::FullyQualified(id)) => Some(&id.value[1..]),
        _ => None,
    }
}

fn is_always_matching_type(name: &[u8], types: &[&TAtomic]) -> bool {
    let matches: fn(&TAtomic) -> bool = match name {
        b"is_string" => |a| {
            matches!(
                a,
                TAtomic::Scalar(TScalar::String(_) | TScalar::ClassLikeString(_))
            )
        },
        b"is_int" | b"is_integer" | b"is_long" => {
            |a| matches!(a, TAtomic::Scalar(TScalar::Integer(_)))
        }
        b"is_float" | b"is_double" => |a| matches!(a, TAtomic::Scalar(TScalar::Float(_))),
        b"is_bool" => |a| matches!(a, TAtomic::Scalar(TScalar::Bool(_))),
        b"is_array" => |a| matches!(a, TAtomic::Array(_)),
        b"is_object" => |a| matches!(a, TAtomic::Object(_)),
        _ => return false,
    };
    !types.is_empty() && types.iter().all(|a| matches(a))
}
