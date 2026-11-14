use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::callable::TCallable;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{
    AssignmentOperator, BinaryOperator, Closure, Expression, FunctionLikeParameter, Identifier,
    Node, NodeKind,
};
use pun_problem::{Edit, Safety};

use super::add_param_type_array_dim_fetch::is_variable;
use super::facts::{Facts, facts};
use super::namespace_of;
use super::param::{
    Flow, add_hint, array_shape_of, doc_order, is_mixed, item_type, param_declaration_unsorted,
    traverse,
};
use crate::rules::support::arguments::value;
use crate::rules::support::class_members::type_of;
use crate::rules::support::expressions::{args, named};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct AddParamTypeArrayReduce;

impl Rule for AddParamTypeArrayReduce {
    fn id(&self) -> &'static str {
        "lint/types/add-param-type-array-reduce"
    }

    fn once(&self) -> bool {
        true
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        if !named(call, "array_reduce") {
            return;
        }
        let facts = facts(ctx);
        if !is_native(&facts, ancestors, call.function) {
            return;
        }
        let args = args(call);
        let Some(Expression::Closure(closure)) = args.get(1).map(|a| unparenthesize(value(a)))
        else {
            return;
        };
        let Some(returned) = closure_return(&facts, value(args[1])) else {
            return;
        };
        let value_type = array_shape_of(&facts, ancestors, value(args[0]))
            .as_ref()
            .and_then(item_type);
        let mut carry: Option<Vec<TAtomic>> = Some(returned.types.to_vec());
        if returns_untyped_param(closure, value_type.is_some())
            || (args.get(2).is_none() && returns_param_sum(closure))
        {
            carry = None;
        } else if let Some(initial) = args.get(2) {
            carry = type_of(&facts, value(initial)).and_then(|initial| combine(initial, returned));
        } else if value_type.as_ref().is_some_and(is_general_int)
            && returns_carry_math_on_item(closure)
            && let Some(types) = carry.as_mut()
        {
            types.sort_by_key(|a| !matches!(a, TAtomic::Scalar(TScalar::Float(_))));
        }
        let params: Vec<&FunctionLikeParameter<'_>> =
            closure.parameter_list.parameters.iter().collect();
        let mut edits: Vec<Edit> = Vec::new();
        if let (Some(param), Some(t)) = (params.get(1), value_type) {
            let order = doc_order(ctx, ancestors);
            let position = |a: &TAtomic| {
                let name = param_declaration_unsorted(
                    std::slice::from_ref(a),
                    &facts.codebase,
                    ctx.php_version,
                )
                .unwrap_or_default();
                let short = name
                    .rsplit('\\')
                    .next()
                    .unwrap_or(&name)
                    .to_ascii_lowercase();
                order
                    .iter()
                    .position(|o| *o == short)
                    .unwrap_or(order.len())
            };
            let mut types = t.types.to_vec();
            types.sort_by_key(|a| position(a));
            edits.extend(refactor_parameter(&facts, param, &types, ctx.php_version));
        }
        if let (Some(param), Some(t)) = (params.first(), carry) {
            edits.extend(refactor_parameter(&facts, param, &t, ctx.php_version));
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report::fixed(
            node_span(call),
            "Add the item and carry types to the array_reduce() closure's parameters.",
            Safety::Unsafe,
            edits,
        ));
    }
}

fn closure_return<'f>(facts: &'f Facts, closure: &Expression<'_>) -> Option<&'f TUnion> {
    let t = type_of(facts, closure)?;
    t.types.iter().find_map(|a| {
        if let TAtomic::Callable(TCallable::Signature(s)) = a {
            s.return_type.as_deref()
        } else {
            None
        }
    })
}

fn returns_untyped_param(closure: &Closure<'_>, value_known: bool) -> bool {
    let untyped: Vec<&[u8]> = closure
        .parameter_list
        .parameters
        .iter()
        .enumerate()
        .filter(|(i, p)| p.hint.is_none() && !(value_known && *i == 1))
        .map(|(_, p)| p)
        .map(|p| p.variable.name)
        .collect();
    let mut returned: Vec<&[u8]> = Vec::new();
    let mut reassigned: Vec<&[u8]> = Vec::new();
    traverse(Node::Block(&closure.body), &mut |node| {
        match node {
            Node::Return(r) => returned.extend(
                r.value
                    .and_then(|v| untyped.iter().copied().find(|name| is_variable(v, name))),
            ),
            Node::Assignment(a) if matches!(a.operator, AssignmentOperator::Assign(_)) => {
                reassigned.extend(
                    untyped
                        .iter()
                        .copied()
                        .find(|name| is_variable(a.lhs, name)),
                );
            }
            _ => {}
        }
        Flow::Continue
    });
    returned.iter().any(|name| !reassigned.contains(name))
}

fn returns_param_sum(closure: &Closure<'_>) -> bool {
    let untyped: Vec<&[u8]> = closure
        .parameter_list
        .parameters
        .iter()
        .filter(|p| p.hint.is_none())
        .map(|p| p.variable.name)
        .collect();
    let is_param = |e: &Expression<'_>| {
        untyped
            .iter()
            .any(|name| is_variable(unparenthesize(e), name))
    };
    let mut found = false;
    traverse(Node::Block(&closure.body), &mut |node| {
        if let Node::Return(r) = node
            && let Some(Expression::Binary(b)) = r.value.map(unparenthesize)
            && matches!(b.operator, BinaryOperator::Addition(_))
            && is_param(b.lhs)
            && is_param(b.rhs)
        {
            found = true;
        }
        Flow::Continue
    });
    found
}

fn is_general_int(t: &TUnion) -> bool {
    matches!(t.types.as_ref(), [TAtomic::Scalar(TScalar::Integer(i))] if i.is_unspecified())
}

fn returns_carry_math_on_item(closure: &Closure<'_>) -> bool {
    let params: Vec<&FunctionLikeParameter<'_>> =
        closure.parameter_list.parameters.iter().collect();
    let [carry, item, ..] = params.as_slice() else {
        return false;
    };
    if carry.hint.is_some() || item.hint.is_some() {
        return false;
    }
    let mut found = false;
    traverse(Node::Block(&closure.body), &mut |node| {
        if let Node::Return(r) = node
            && let Some(Expression::Binary(b)) = r.value.map(unparenthesize)
            && matches!(
                b.operator,
                BinaryOperator::Subtraction(_) | BinaryOperator::Multiplication(_)
            )
            && is_variable(unparenthesize(b.lhs), carry.variable.name)
            && is_variable(unparenthesize(b.rhs), item.variable.name)
        {
            found = true;
        }
        Flow::Continue
    });
    found
}

fn is_native(facts: &Facts, ancestors: &[Node<'_, '_>], function: &Expression<'_>) -> bool {
    if matches!(
        function,
        Expression::Identifier(Identifier::FullyQualified(_))
    ) {
        return true;
    }
    namespace_of(ancestors).is_none_or(|ns| {
        !facts
            .codebase
            .function_exists(format!("{ns}\\array_reduce").as_bytes())
    })
}

fn refactor_parameter(
    facts: &Facts,
    param: &FunctionLikeParameter<'_>,
    types: &[TAtomic],
    php_version: u32,
) -> Option<Edit> {
    if param.hint.is_some() || is_mixed(&TUnion::from_vec(types.to_vec())) {
        return None;
    }
    let declaration = param_declaration_unsorted(types, &facts.codebase, php_version)?;
    Some(add_hint(param, &declaration))
}

fn combine(initial: &TUnion, returned: &TUnion) -> Option<Vec<TAtomic>> {
    if initial == returned {
        return Some(initial.types.to_vec());
    }
    let mut types: Vec<TAtomic> = Vec::new();
    for t in [initial, returned] {
        if t.types.len() > 1
            && t.types.iter().any(|a| matches!(a, TAtomic::Object(TObject::Named(n)) if n.intersection_types.is_some()))
        {
            return None;
        }
        types.extend(t.types.iter().cloned());
    }
    types.sort_by_key(sort_key);
    Some(types)
}

fn sort_key(a: &TAtomic) -> (u8, i128, String) {
    match a {
        TAtomic::Null => (4, 0, String::new()),
        TAtomic::Scalar(TScalar::Bool(b)) if !b.is_general() => (3, 0, String::new()),
        TAtomic::Scalar(s) if s.is_literal_value() => (
            1,
            s.get_literal_int_value().map_or(0, i128::from),
            String::new(),
        ),
        other => (2, 0, mago_codex::ttype::TType::get_id(other).to_string()),
    }
}
