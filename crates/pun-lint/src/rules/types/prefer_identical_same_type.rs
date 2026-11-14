use mago_codex::ttype::TType;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::array::TArray;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::atomic::scalar::class_like_string::TClassLikeString;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{
    Access, BinaryOperator, ClassLikeConstantSelector, Expression, Node, NodeKind,
};
use pun_problem::{Edit, Safety};

use super::facts::{Facts, facts};
use super::native::native_type;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferIdenticalSameType;

impl Rule for PreferIdenticalSameType {
    fn id(&self) -> &'static str {
        "lint/types/prefer-identical-same-type"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Binary])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Binary(binary) = node else {
            return;
        };
        let operator = match binary.operator {
            BinaryOperator::Equal(_) => "===",
            BinaryOperator::NotEqual(_) | BinaryOperator::AngledNotEqual(_) => "!==",
            _ => return,
        };
        let (left, right) = (unparenthesize(binary.lhs), unparenthesize(binary.rhs));
        if matches!(left, Expression::ArrayAccess(_)) || matches!(right, Expression::ArrayAccess(_))
        {
            return;
        }
        let facts = facts(ctx);
        let (Some(l), Some(r)) = (native_type(&facts, left), native_type(&facts, right)) else {
            return;
        };
        let object = |e: &Expression<'_>, t: &TUnion| {
            object_class_fetch(&facts, e).unwrap_or_else(|| has_object(&facts, t))
        };
        if object(left, l) || object(right, r) || l.is_mixed() || r.is_mixed() {
            return;
        }
        let (Some(l), Some(r)) = (normalized(l), normalized(r)) else {
            return;
        };
        if l != r {
            return;
        }
        let text = format!(
            "{} {operator} {}",
            operand(ctx.source, left),
            operand(ctx.source, right)
        );
        out.push(Report::fixed(
            node_span(binary),
            format!("Use `{operator}`: both sides have the same type."),
            Safety::Safe,
            vec![Edit::new(node_span(binary), text)],
        ));
    }
}

fn has_object(facts: &Facts, t: &TUnion) -> bool {
    t.types.iter().any(|a| atomic_has_object(facts, a))
}

fn atomic_has_object(facts: &Facts, atomic: &TAtomic) -> bool {
    match atomic {
        TAtomic::Null | TAtomic::Never | TAtomic::Void | TAtomic::Resource(_) => false,
        TAtomic::Scalar(TScalar::String(s)) => s
            .get_known_literal_value()
            .is_some_and(|v| is_declared_class(facts, v)),
        TAtomic::Scalar(TScalar::ClassLikeString(TClassLikeString::Literal { value })) => {
            is_declared_class(facts, value.as_bytes())
        }
        TAtomic::Scalar(TScalar::ClassLikeString(_)) => true,
        TAtomic::Scalar(_) => false,
        TAtomic::Array(TArray::List(list)) => {
            has_object(facts, &list.element_type)
                || (list.known_elements.iter().flat_map(|m| m.iter()))
                    .any(|(_, (_, t))| has_object(facts, t))
        }
        TAtomic::Array(TArray::Keyed(keyed)) => {
            (keyed.parameters.iter()).any(|(k, v)| has_object(facts, k) || has_object(facts, v))
                || (keyed.known_items.iter().flat_map(|m| m.iter()))
                    .any(|(_, (_, t))| has_object(facts, t))
        }
        _ => true,
    }
}

fn object_class_fetch(facts: &Facts, expr: &Expression<'_>) -> Option<bool> {
    let Expression::Access(Access::ClassConstant(fetch)) = expr else {
        return None;
    };
    let ClassLikeConstantSelector::Identifier(constant) = &fetch.constant else {
        return None;
    };
    if !constant.value.eq_ignore_ascii_case(b"class")
        || matches!(
            unparenthesize(fetch.class),
            Expression::Identifier(_)
                | Expression::Self_(_)
                | Expression::Static(_)
                | Expression::Parent(_)
        )
    {
        return None;
    }
    let t = facts.native_type(node_span(fetch.class))?;
    Some(match t.types.as_ref() {
        [TAtomic::Object(TObject::Named(n))] => (facts.codebase)
            .get_class_like(n.name.as_bytes())
            .is_some_and(|c| c.flags.is_final()),
        [TAtomic::Object(TObject::Enum(_))] => true,
        _ => false,
    })
}

fn is_declared_class(facts: &Facts, name: &[u8]) -> bool {
    facts
        .codebase
        .get_class_like(name)
        .is_some_and(|c| c.original_name.as_bytes() == name)
}

fn normalized(t: &TUnion) -> Option<String> {
    let all =
        |f: fn(&TScalar) -> bool| (t.types.iter()).all(|a| matches!(a, TAtomic::Scalar(s) if f(s)));
    if all(TScalar::is_any_string) {
        return Some("string".to_string());
    }
    if all(TScalar::is_bool) {
        return Some("bool".to_string());
    }
    if all(TScalar::is_int) {
        return Some("int".to_string());
    }
    if all(TScalar::is_float) {
        return Some("float".to_string());
    }
    match t.types.as_ref() {
        [one] => Some(one.get_id().to_string()),
        _ => None,
    }
}

fn operand(source: &str, expr: &Expression<'_>) -> String {
    let span = node_span(expr);
    let text = &source[span.start..span.end];
    let loose = match expr {
        Expression::Binary(b) => !matches!(
            b.operator,
            BinaryOperator::Exponentiation(_)
                | BinaryOperator::Instanceof(_)
                | BinaryOperator::Multiplication(_)
                | BinaryOperator::Division(_)
                | BinaryOperator::Modulo(_)
                | BinaryOperator::Addition(_)
                | BinaryOperator::Subtraction(_)
                | BinaryOperator::StringConcat(_)
                | BinaryOperator::LeftShift(_)
                | BinaryOperator::RightShift(_)
                | BinaryOperator::LessThan(_)
                | BinaryOperator::LessThanOrEqual(_)
                | BinaryOperator::GreaterThan(_)
                | BinaryOperator::GreaterThanOrEqual(_)
        ),
        Expression::Conditional(_)
        | Expression::Assignment(_)
        | Expression::Yield(_)
        | Expression::Throw(_)
        | Expression::Pipe(_)
        | Expression::ArrowFunction(_) => true,
        _ => false,
    };
    if loose {
        format!("({text})")
    } else {
        text.to_string()
    }
}
