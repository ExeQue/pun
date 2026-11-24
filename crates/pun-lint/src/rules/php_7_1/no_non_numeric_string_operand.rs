//! Rector 2.8.4's `BinaryOpBetweenNumberAndStringRector`, with `ExprAnalyzer::isNonTypedFromParam`.

use mago_syntax::cst::{
    Binary, BinaryOperator, Expression, Literal, MagicConstant, Node, NodeKind, Variable,
};
use pun_phpstan::types::{Type, is_super_type_of};

use crate::problem::{Edit, Safety};
use crate::rules::support::rector_names::attribute::is_numeric;
use crate::rules::types::compare::mapper::expr_type;
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoNonNumericStringOperand;

impl Rule for NoNonNumericStringOperand {
    fn id(&self) -> &'static str {
        "lint/php-7.1/no-non-numeric-string-operand"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70100)
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
        let Node::Binary(binary) = node else {
            return;
        };
        if matches!(
            binary.operator,
            BinaryOperator::StringConcat(_)
                | BinaryOperator::NullCoalesce(_)
                | BinaryOperator::Instanceof(_)
        ) {
            return;
        }
        let (left, right) = (unparenthesize(binary.lhs), unparenthesize(binary.rhs));
        // array element number type only comes from phpdoc array shape, never a runtime guarantee
        if matches!(left, Expression::ArrayAccess(_)) || matches!(right, Expression::ArrayAccess(_))
        {
            return;
        }
        let facts = facts(ctx);
        if is_non_typed_from_param(&facts, ancestors, left)
            || is_non_typed_from_param(&facts, ancestors, right)
        {
            return;
        }
        let (string, number) = if is_string_or_static_non_numeric_string(&facts, ancestors, left)
            && is_number_type(&facts, ancestors, right)
        {
            (left, right)
        } else if is_string_or_static_non_numeric_string(&facts, ancestors, right)
            && is_number_type(&facts, ancestors, left)
        {
            (right, left)
        } else {
            return;
        };
        let zero = if expr_type(&facts, ancestors, number, true)
            .is_integer()
            .yes()
        {
            "0"
        } else {
            "0.0"
        };
        report(binary, string, zero, out);
    }
}

fn report(binary: &Binary<'_>, string: &Expression<'_>, zero: &str, out: &mut Vec<Report>) {
    out.push(Report::fixed(
        node_span(binary),
        format!("A non-numeric string operand is a warning since PHP 7.1; use `{zero}`."),
        Safety::Unsafe,
        vec![Edit::new(node_span(string), zero)],
    ));
}

fn is_number_type(facts: &Facts, ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    let native = expr_type(facts, ancestors, expr, true);
    native.is_integer().yes() || native.is_float().yes()
}

fn is_string_or_static_non_numeric_string(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
) -> bool {
    // replace only scalar values, not variables/constants/etc.
    match expr {
        Expression::MagicConstant(MagicConstant::Line(_)) => false,
        Expression::Literal(Literal::String(s)) => s
            .value
            .is_some_and(|v| !is_numeric(&String::from_utf8_lossy(v))),
        // `__FILE__`, `__CLASS__` and the others are constant strings that are never numeric.
        Expression::MagicConstant(_) => true,
        Expression::Literal(_) | Expression::CompositeString(_) | Expression::Variable(_) => {
            match expr_type(facts, ancestors, expr, false) {
                Type::ConstString(v) => !is_numeric(&String::from_utf8_lossy(&v)),
                _ => false,
            }
        }
        _ => false,
    }
}

/// `ExprAnalyzer::isNonTypedFromParam`: a variable whose native type does not back its known type.
fn is_non_typed_from_param(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
) -> bool {
    let Expression::Variable(variable) = expr else {
        return false;
    };
    let native = expr_type(facts, ancestors, expr, true);
    let full = expr_type(facts, ancestors, expr, false);
    let is_mixed = |t: &Type| matches!(t, Type::Mixed { .. });
    if matches!(native, Type::Mixed { explicit: false }) || (is_mixed(&native) && !is_mixed(&full))
    {
        return true;
    }
    if matches!(native, Type::ObjectWithoutClass) && !matches!(full, Type::ObjectWithoutClass) {
        return true;
    }
    if !matches!(variable, Variable::Direct(_)) {
        return true;
    }
    if matches!(native, Type::Union(_)) && native != full {
        return true;
    }
    !is_super_type_of(&native, &full).yes()
}
