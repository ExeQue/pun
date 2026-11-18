//! Rector 2.8.4's `FilesystemIteratorSkipDotsRector`.

use mago_codex::metadata::CodebaseMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::reference::{TReference, TReferenceMemberSelector};
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_syntax::cst::{
    Access, BinaryOperator, ClassConstantAccess, ClassLikeConstantSelector, Expression, Node,
    NodeKind,
};
use pun_problem::{Edit, Safety};

use super::super::types::facts::facts;
use crate::rules::support::arguments::value;
use crate::rules::support::class_members::qualified;
use crate::rules::support::expressions::operand;
use crate::rules::support::names::resolve_class_name;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

/// `FilesystemIterator::SKIP_DOTS`.
const SKIP_DOTS: i64 = 4096;

pub struct AddSkipDotsFlag;

impl Rule for AddSkipDotsFlag {
    fn id(&self) -> &'static str {
        "lint/php-8.2/add-skip-dots-flag"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80200)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Instantiation])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Instantiation(new) = node else {
            return;
        };
        let Some(flags) = (new.argument_list.as_ref()).and_then(|l| l.arguments.get(1)) else {
            return;
        };
        // Rector's type of `parent` is no `FilesystemIterator`, though its constants resolve.
        if let Expression::Parent(_) = unparenthesize(new.class) {
            return;
        }
        let facts = facts(ctx);
        let codebase = &facts.codebase;
        let Some(class) = class_of(codebase, ancestors, new.class) else {
            return;
        };
        if !codebase.is_instance_of(class.as_bytes(), b"FilesystemIterator") {
            return;
        }
        if is_skip_dots_present(codebase, ancestors, value(flags)) {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Add `FilesystemIterator::SKIP_DOTS`, which PHP 8.2 no longer sets by itself.",
            Safety::Safe,
            vec![Edit::new(
                node_span(flags),
                format!(
                    "{} | \\FilesystemIterator::SKIP_DOTS",
                    operand(ctx, value(flags), 111, Some(111))
                ),
            )],
        ));
    }
}

/// The class a name, `self`, `static` or `parent` refers to.
fn class_of(
    codebase: &CodebaseMetadata,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
) -> Option<String> {
    let enclosing = || {
        ancestors.iter().rev().find_map(|n| match n {
            Node::Class(c) => Some(qualified(ancestors, c.name.value)),
            _ => None,
        })
    };
    match unparenthesize(expr) {
        Expression::Identifier(name) => Some(resolve_class_name(ancestors, name)),
        Expression::Self_(_) | Expression::Static(_) => enclosing(),
        Expression::Parent(_) => {
            let own = codebase.get_class_like(enclosing()?.as_bytes())?;
            Some(own.direct_parent_class?.to_string())
        }
        _ => None,
    }
}

fn is_skip_dots_present<'a>(
    codebase: &CodebaseMetadata,
    ancestors: &[Node<'_, '_>],
    mut expr: &'a Expression<'a>,
) -> bool {
    while let Expression::Binary(b) = unparenthesize(expr)
        && let BinaryOperator::BitwiseOr(_) = b.operator
    {
        if is_skip_dots(codebase, ancestors, b.rhs) {
            return true;
        }
        expr = b.lhs;
    }
    is_skip_dots(codebase, ancestors, expr)
}

/// Anything but a class constant fetch "can be anything" to Rector, so counts as the flag.
fn is_skip_dots(
    codebase: &CodebaseMetadata,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
) -> bool {
    match unparenthesize(expr) {
        Expression::Access(Access::ClassConstant(access)) => {
            class_constant(codebase, ancestors, access) == Some(SKIP_DOTS)
        }
        _ => true,
    }
}

/// `ValueResolver::resolveClassConstFetch()`: the integer a known class's constant holds (never an enum's).
fn class_constant(
    codebase: &CodebaseMetadata,
    ancestors: &[Node<'_, '_>],
    access: &ClassConstantAccess<'_>,
) -> Option<i64> {
    let ClassLikeConstantSelector::Identifier(name) = &access.constant else {
        return None;
    };
    let class = class_of(codebase, ancestors, access.class)?;
    constant_value(codebase, class.as_bytes(), name.value, 0)
}

/// A constant set to another class's constant (`const SKIP = \FilesystemIterator::SKIP_DOTS;`) is followed, as
/// Rector follows the constant's value expression.
fn constant_value(
    codebase: &CodebaseMetadata,
    class: &[u8],
    name: &[u8],
    depth: u8,
) -> Option<i64> {
    let metadata = codebase.get_class_like(class)?;
    if metadata.kind.is_enum() || depth > 8 {
        return None;
    }
    let constant = std::iter::once(&metadata.name)
        .chain(metadata.all_parent_classes.iter())
        .chain(metadata.all_parent_interfaces.iter())
        .find_map(|c| codebase.get_class_constant(c.as_bytes(), name))?;
    match constant
        .inferred_type
        .as_ref()
        .filter(|t| t.is_single())?
        .get_single()
    {
        TAtomic::Scalar(TScalar::Integer(i)) => i.get_literal_value(),
        TAtomic::Reference(TReference::Member {
            class_like_name,
            member_selector: TReferenceMemberSelector::Identifier(member),
        }) => constant_value(
            codebase,
            class_like_name.as_bytes(),
            member.as_bytes(),
            depth + 1,
        ),
        _ => None,
    }
}
