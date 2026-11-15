use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{Expression, Node, NodeKind};

use super::declare::{expression_order, return_declaration};
use super::facts::{Facts, facts};
use super::native::native_type;
use super::silent_void::{Place, has_silent_void};
use super::{add_return_type, class_scope, returns_scoped};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct AddClosureReturn;

pub struct AddArrowFunctionReturn;

const SCALAR_TYPES: u32 = 70000;
const ARROW_FUNCTION: u32 = 70400;

impl Rule for AddClosureReturn {
    fn id(&self) -> &'static str {
        "lint/types/add-closure-return-type"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(SCALAR_TYPES)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Closure])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        closure_return(node, ancestors, ctx, out, false);
    }
}

pub(crate) fn closure_return<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    ancestors: &[Node<'ast, 'arena>],
    ctx: &Context<'_>,
    out: &mut Vec<Report>,
    full: bool,
) {
    let Node::Closure(closure) = node else {
        return;
    };
    if closure.return_type_hint.is_some() {
        return;
    }
    let facts = facts(ctx);
    let scope = class_scope(ancestors);
    let place = Place {
        facts: &facts,
        ancestors,
        scope: scope.as_ref(),
    };
    let returns = returns_scoped(&closure.body);
    if returns.is_empty() {
        return;
    }
    if returns.iter().all(|r| {
        r.value
            .is_some_and(|v| is_new_parent_in_trait(v, ancestors))
    }) {
        let at = match &closure.use_clause {
            Some(uses) => node_span(uses).end,
            None => node_span(&closure.parameter_list).end,
        };
        out.push(add_return_type(node_span(&closure.function), at, "parent"));
        return;
    }
    let mut types: Vec<&TAtomic> = Vec::new();
    let mut has_void = false;
    for r in &returns {
        match r.value {
            None => has_void = true,
            Some(value) => match expression_order(value, |expr| type_of(&facts, expr, full)) {
                Some(t) => types.extend(t),
                None => return,
            },
        }
    }
    if !has_void && has_silent_void(&place, closure.body.statements.iter()) {
        has_void = true;
    }
    if has_void
        || types
            .iter()
            .any(|t| matches!(t, TAtomic::Never | TAtomic::Mixed(_)))
    {
        return;
    }
    let Some(r#type) = return_declaration(types, &facts.codebase, ctx.php_version.id(), &[], true)
    else {
        return;
    };
    let at = match &closure.use_clause {
        Some(uses) => node_span(uses).end,
        None => node_span(&closure.parameter_list).end,
    };
    out.push(add_return_type(node_span(&closure.function), at, &r#type));
}

fn is_new_parent_in_trait(expr: &Expression<'_>, ancestors: &[Node<'_, '_>]) -> bool {
    matches!(unparenthesize(expr), Expression::Instantiation(i) if matches!(i.class, Expression::Parent(_)))
        && matches!(
            ancestors.iter().rev().find(|a| matches!(
                a,
                Node::Class(_)
                    | Node::AnonymousClass(_)
                    | Node::Interface(_)
                    | Node::Trait(_)
                    | Node::Enum(_)
            )),
            Some(Node::Trait(_))
        )
}

fn type_of<'f>(facts: &'f Facts, expr: &Expression<'_>, full: bool) -> Option<&'f TUnion> {
    if full {
        facts
            .full_type(node_span(expr))
            .filter(|t| !t.types.is_empty())
    } else {
        native_type(facts, expr)
    }
}

impl Rule for AddArrowFunctionReturn {
    fn id(&self) -> &'static str {
        "lint/types/add-arrow-function-return-type"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(ARROW_FUNCTION)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ArrowFunction])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::ArrowFunction(arrow) = node else {
            return;
        };
        if arrow.return_type_hint.is_some() {
            return;
        }
        if is_new_parent_in_trait(arrow.expression, ancestors) {
            out.push(add_return_type(
                node_span(&arrow.r#fn),
                node_span(&arrow.parameter_list).end,
                "parent",
            ));
            return;
        }
        let facts = facts(ctx);
        let Some(types) = expression_order(arrow.expression, |expr| native_type(&facts, expr))
        else {
            return;
        };
        if types
            .iter()
            .any(|t| matches!(t, TAtomic::Void | TAtomic::Never | TAtomic::Mixed(_)))
        {
            return;
        }
        if let Some(r#type) =
            return_declaration(types, &facts.codebase, ctx.php_version.id(), &[], true)
        {
            out.push(add_return_type(
                node_span(&arrow.r#fn),
                node_span(&arrow.parameter_list).end,
                &r#type,
            ));
        }
    }
}
