//! Rector 2.8.4's `IfToNullCoalescingAssignRector`.

use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{
    Access, AssignmentOperator, BinaryOperator, ClassLikeMemberSelector, Construct, Expression,
    Node, NodeKind, Statement, UnaryPrefixOperator, Variable,
};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::{unpacked, value};
use crate::rules::support::expressions::{call_to, equal, is_constant, text};
use crate::rules::support::statement_edits::{comments, if_view};
use crate::rules::support::statements::line_indent;
use crate::rules::types::class_scope;
use crate::rules::types::facts::{Facts, facts};
use crate::rules::types::names::resolve_class_name;
use crate::syntax::{node_span, same_code, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferCoalesceAssignOverIf;

impl Rule for PreferCoalesceAssignOverIf {
    fn id(&self) -> &'static str {
        "lint/php-7.4/prefer-coalesce-assign-over-if"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70400)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::If])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::If(r#if) = node else {
            return;
        };
        let view = if_view(ctx.source, r#if);
        if view.else_.is_some() || !view.elseifs.is_empty() || view.stmts.stmts.len() != 1 {
            return;
        }
        let only = view.stmts.stmts[0];
        let Statement::Expression(statement) = only else {
            return;
        };
        let Expression::Assignment(assign) = statement.expression else {
            return;
        };
        if !matches!(assign.operator, AssignmentOperator::Assign(_)) {
            return;
        }
        let Some(tested) = match_null_guarded_expr(r#if.condition) else {
            return;
        };
        if !equal(ctx, assign.lhs, tested) {
            return;
        }
        // a typed non-nullable property can never be null on the left of ??=
        if is_non_nullable_property(&facts(ctx), ancestors, tested) {
            return;
        }
        // the assigned value must not reference the target, e.g. $x = $x + 1
        if references(
            ctx.source,
            Node::Expression(assign.rhs),
            node_span(assign.lhs),
        ) {
            return;
        }
        let span = node_span(r#if);
        let indent = line_indent(ctx.source, span.start);
        // keep comments from both the if and the single assign inside it
        let kept = comments(
            ctx.source,
            node_span(r#if.condition).end,
            node_span(only).start,
        )
        .map(|c| format!("{}\n{indent}", &ctx.source[c.start..c.end]))
        .unwrap_or_default();
        let code = format!(
            "{kept}{} ??= {};",
            text(ctx, assign.lhs),
            text(ctx, assign.rhs)
        );
        out.push(Report::fixed(
            span,
            "Assign with `??=` instead of an `if` that assigns only when the value is null.",
            Safety::Safe,
            vec![Edit::new(span, code)],
        ));
    }
}

fn match_null_guarded_expr<'a>(expr: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    let expr = unparenthesize(expr);
    // ! isset($value)
    if let Expression::UnaryPrefix(not) = expr
        && matches!(not.operator, UnaryPrefixOperator::Not(_))
        && let Expression::Construct(Construct::Isset(isset)) = unparenthesize(not.operand)
    {
        let [only] = isset.values.as_slice() else {
            return None;
        };
        return Some(only);
    }
    // is_null($value)
    if let Some(call) = call_to(expr, "is_null") {
        let args = call.argument_list.arguments.as_slice();
        let [only] = args else {
            return None;
        };
        if unpacked(only) {
            return None;
        }
        return Some(value(only));
    }
    // null === $value or $value === null
    if let Expression::Binary(binary) = expr
        && matches!(binary.operator, BinaryOperator::Identical(_))
    {
        if is_constant(binary.lhs, "null") {
            return Some(binary.rhs);
        }
        if is_constant(binary.rhs, "null") {
            return Some(binary.lhs);
        }
    }
    None
}

/// `isNonNullableProperty`: a property fetch whose declared (readable) type is neither mixed nor nullable.
fn is_non_nullable_property(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
) -> bool {
    let class_of = |object: &Expression<'_>| {
        let t = facts.full_type(node_span(unparenthesize(object)))?;
        let [TAtomic::Object(TObject::Named(owner))] = t.types.as_ref() else {
            return None;
        };
        Some(String::from_utf8_lossy(owner.name.as_bytes()).into_owned())
    };
    let (class, name) = match expr {
        Expression::Access(Access::Property(fetch)) => {
            let ClassLikeMemberSelector::Identifier(name) = &fetch.property else {
                return false;
            };
            let Some(class) = class_of(fetch.object) else {
                return false;
            };
            (class, [b"$", name.value].concat())
        }
        Expression::Access(Access::StaticProperty(fetch)) => {
            let Variable::Direct(v) = &fetch.property else {
                return false;
            };
            let owner = match unparenthesize(fetch.class) {
                Expression::Static(_) | Expression::Self_(_) => {
                    class_scope(ancestors).and_then(|scope| scope.name)
                }
                Expression::Identifier(id) => Some(resolve_class_name(ancestors, id)),
                object => class_of(object),
            };
            let Some(owner) = owner else {
                return false;
            };
            (owner, v.name.to_vec())
        }
        _ => return false,
    };
    let Some(property) = facts
        .documented()
        .get_declaring_property(class.as_bytes(), &name)
    else {
        return false;
    };
    let Some(t) = property.type_metadata.as_ref().map(|t| &t.type_union) else {
        return false;
    };
    !t.is_mixed() && !t.has_null()
}

fn references(source: &str, node: Node<'_, '_>, target: crate::problem::Span) -> bool {
    if same_code(source, node_span(&node), target) {
        return true;
    }
    let mut found = false;
    node.visit_children(|child| found = found || references(source, child, target));
    found
}
