use crate::problem::Safety;
use mago_codex::ttype::atomic::TAtomic;
use mago_syntax::cst::{Expression, FunctionLikeParameterList, Hint, Node, NodeKind};

use super::facts::facts;
use super::param::{
    ArrayShape, add_hint, array_shape_of, constant_values, constant_values_union, is_mixed,
    item_declaration, item_type, param_declaration_unsorted,
};
use crate::rules::laravel::get_arg;
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::named;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const ARRAY_ANY: u32 = 80400;

pub struct AddParamTypeArrayAnyAll;

pub struct PreferNarrowParamType;

impl Rule for AddParamTypeArrayAnyAll {
    fn id(&self) -> &'static str {
        "lint/types/add-param-type-array-any-all"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(ARRAY_ANY)
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
        visit(
            node,
            ancestors,
            ctx,
            out,
            &["array_any", "array_all"],
            false,
        );
    }
}

impl Rule for PreferNarrowParamType {
    fn id(&self) -> &'static str {
        "lint/types/prefer-narrow-param-type"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(ARRAY_ANY)
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
        visit(
            node,
            ancestors,
            ctx,
            out,
            &["array_any", "array_all", "array_find", "array_find_key"],
            true,
        );
    }
}

fn visit(
    node: Node<'_, '_>,
    ancestors: &[Node<'_, '_>],
    ctx: &Context<'_>,
    out: &mut Vec<Report>,
    names: &[&str],
    narrow: bool,
) {
    let Node::FunctionCall(call) = node else {
        return;
    };
    if !names.iter().any(|n| named(call, n)) {
        return;
    }
    let (Some(array), Some(callback)) = (
        get_arg(&call.argument_list, "array", 0),
        get_arg(&call.argument_list, "callback", 1),
    ) else {
        return;
    };
    let Some(params) = closure_params(value(callback)) else {
        return;
    };
    let Some(param) = params.parameters.first() else {
        return;
    };
    let hint_span = match (&param.hint, narrow) {
        (None, false) => None,
        (Some(hint @ Hint::Nullable(_)), true) => Some(node_span(hint)),
        _ => return,
    };
    let facts = facts(ctx);
    let Some(shape) = array_shape_of(&facts, ancestors, value(array)) else {
        return;
    };
    let Some(item) = item_type(&shape) else {
        return;
    };
    if is_mixed(&item) || (narrow && item.types.iter().any(|t| matches!(t, TAtomic::Null))) {
        return;
    }
    let declaration = match shape {
        ArrayShape::Constant(values) => {
            let Some(types) = constant_values(&facts, ancestors, value(array), &values) else {
                return;
            };
            param_declaration_unsorted(
                &constant_values_union(&types),
                &facts.codebase,
                ctx.php_version.id(),
            )
        }
        ArrayShape::General(_) => {
            item_declaration(&facts, ancestors, value(array), &item, ctx.php_version.id())
        }
    };
    let Some(declaration) = declaration else {
        return;
    };
    let edit = match hint_span {
        Some(span) => crate::problem::Edit::new(span, declaration),
        None => add_hint(param, &declaration),
    };
    out.push(Report::fixed(
        node_span(call),
        "Add the array's item type to the callback's parameter.",
        Safety::Unsafe,
        vec![edit],
    ));
}

pub(crate) fn closure_params<'a>(
    expr: &'a Expression<'a>,
) -> Option<&'a FunctionLikeParameterList<'a>> {
    match unparenthesize(expr) {
        Expression::Closure(c) => Some(&c.parameter_list),
        Expression::ArrowFunction(a) => Some(&a.parameter_list),
        _ => None,
    }
}
