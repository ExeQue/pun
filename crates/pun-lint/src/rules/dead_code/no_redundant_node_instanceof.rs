//! Rector 2.8.4's `RemoveRefactorDuplicatedNodeInstanceCheckRector`, with the Rector-rule helpers
//! `AddParamTypeToRefactorMethodRector` (types/add_refactor_param_phpdoc.rs) shares.

use mago_syntax::cst::{
    Access, ArrayElement, Class, ClassLikeConstantSelector, ClassLikeMember, Expression, Method,
    MethodBody, Node, NodeKind, Statement, UnaryPrefixOperator,
};

use crate::problem::Safety;
use crate::rules::support::class_members::qualified;
use crate::rules::support::docblocks::Doc;
use crate::rules::support::names::{imported, resolve_class_name};
use crate::rules::support::statements::{block_list, remove, tokens};
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub(crate) const ABSTRACT_RECTOR: &str = "Rector\\Rector\\AbstractRector";
const NODE: &str = "PhpParser\\Node";

/// `$classReflection->is('Rector\Rector\AbstractRector')`: the class, by Pun's codebase, is a Rector rule.
pub(crate) fn is_rector_rule(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    class: &Class<'_>,
) -> bool {
    let name = qualified(ancestors, class.name.value);
    facts(ctx)
        .codebase
        .is_instance_of(name.as_bytes(), ABSTRACT_RECTOR.as_bytes())
}

/// `Class_::getMethod()`, ignoring case.
pub(crate) fn method<'a>(class: &'a Class<'a>, name: &str) -> Option<&'a Method<'a>> {
    class.members.iter().find_map(|m| match m {
        ClassLikeMember::Method(method)
            if method.name.value.eq_ignore_ascii_case(name.as_bytes()) =>
        {
            Some(method)
        }
        _ => None,
    })
}

pub(crate) fn first_statement<'a>(method: &'a Method<'a>) -> Option<&'a Statement<'a>> {
    match &method.body {
        MethodBody::Concrete(body) => body.statements.iter().find(|s| {
            !matches!(
                s,
                Statement::Noop(_) | Statement::OpeningTag(_) | Statement::ClosingTag(_)
            )
        }),
        MethodBody::Abstract(_) => None,
    }
}

/// The class names a `getNodeTypes()` whose first statement returns an array literal of `X::class` lists, or
/// nothing when an item is not one.
pub(crate) fn node_types(ancestors: &[Node<'_, '_>], class: &Class<'_>) -> Option<Vec<String>> {
    let Statement::Return(r#return) = first_statement(method(class, "getNodeTypes")?)? else {
        return None;
    };
    let elements = match unparenthesize(r#return.value?) {
        Expression::Array(a) => &a.elements,
        Expression::LegacyArray(a) => &a.elements,
        _ => return None,
    };
    elements
        .iter()
        .map(|e| match e {
            ArrayElement::Value(v) => class_constant(ancestors, v.value),
            _ => None,
        })
        .collect()
}

/// The class an `X::class` names.
fn class_constant(ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> Option<String> {
    let Expression::Access(Access::ClassConstant(access)) = unparenthesize(expr) else {
        return None;
    };
    let (Expression::Identifier(class), ClassLikeConstantSelector::Identifier(constant)) =
        (access.class, &access.constant)
    else {
        return None;
    };
    constant
        .value
        .eq_ignore_ascii_case(b"class")
        .then(|| resolve_class_name(ancestors, class))
}

/// A PHPDoc class name resolved against the file's imports and namespace.
pub(crate) fn resolve_doc_class(ancestors: &[Node<'_, '_>], written: &str) -> String {
    if let Some(fq) = written.strip_prefix('\\') {
        return fq.to_string();
    }
    imported(ancestors, written).unwrap_or_else(|| {
        let namespace = qualified(ancestors, b"");
        match namespace.trim_end_matches('\\') {
            "" => written.to_string(),
            ns => format!("{ns}\\{written}"),
        }
    })
}

/// The type the method's PHPDoc gives `$node` (`getParamType('$node')`): its words after `@param`.
pub(crate) fn node_param_tag<'s>(
    ctx: &Context<'s>,
    chain: &[Node<'_, '_>],
    method: &Method<'_>,
) -> Option<Option<&'s str>> {
    let doc = Doc::of(ctx, chain, node_span(method).start)?;
    (0..doc.len()).find_map(|i| {
        let words = doc.words(i);
        match words.as_slice() {
            ["@param", "$node", ..] => Some(None),
            ["@param", ty, "$node", ..] => Some(Some(*ty)),
            _ => None,
        }
    })
}

pub struct NoRedundantNodeInstanceof;

impl Rule for NoRedundantNodeInstanceof {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-redundant-node-instanceof"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Class(class) = node else { return };
        if !is_rector_rule(ctx, ancestors, class) {
            return;
        }
        let Some(refactor) = method(class, "refactor") else {
            return;
        };
        let Some(Statement::If(r#if)) = first_statement(refactor) else {
            return;
        };
        // matchBooleanNotInstanceOfNodeClass(): `! ... instanceof X`, X a known php-parser node
        let Expression::UnaryPrefix(not) = unparenthesize(r#if.condition) else {
            return;
        };
        if !matches!(not.operator, UnaryPrefixOperator::Not(_)) {
            return;
        }
        let Expression::Binary(instanceof) = unparenthesize(not.operand) else {
            return;
        };
        if !instanceof.operator.is_instanceof() {
            return;
        }
        let Expression::Identifier(checked) = instanceof.rhs else {
            return;
        };
        let checked = resolve_class_name(ancestors, checked);
        let codebase = &facts(ctx).codebase;
        if !codebase.is_instance_of(checked.as_bytes(), NODE.as_bytes()) {
            return;
        }
        let mut chain = ancestors.to_vec();
        chain.push(node);
        // matchNodeParamType(): the `@param` type of `$node`, when it is one class
        let param_type = node_param_tag(ctx, &chain, refactor)
            .flatten()
            .filter(|t| !t.contains(['|', '&', '<', '?', '(']))
            .map(|t| resolve_doc_class(ancestors, t))
            .filter(|t| codebase.get_class_like(t.as_bytes()).is_some());
        if method(class, "getNodeTypes").is_none() {
            return;
        }
        let only_type = node_types_single(ancestors, class);
        let expected = match (param_type, only_type) {
            (Some(param), _) => param,
            (None, Some(only)) => only,
            (None, None) => return,
        };
        if !expected.eq_ignore_ascii_case(&checked) {
            return;
        }
        let MethodBody::Concrete(body) = &refactor.body else {
            return;
        };
        let tokens = tokens(ctx);
        let list = block_list(tokens, body);
        let edits = remove(ctx.source, tokens, &list, &[0]);
        out.push(Report::fixed(
            node_span(r#if),
            "Remove the instanceof check: `getNodeTypes()` already limits the node to that type.",
            Safety::Safe,
            edits,
        ));
    }
}

/// The one class `getNodeTypes()` returns.
fn node_types_single(ancestors: &[Node<'_, '_>], class: &Class<'_>) -> Option<String> {
    match node_types(ancestors, class)?.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}
