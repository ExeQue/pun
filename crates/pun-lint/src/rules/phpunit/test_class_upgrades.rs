//! rector-phpunit's PHPUnit 10.0 upgrades that add a first member to a test class: `AddProphecyTraitRector` and
//! `ParentTestClassConstructorRector`.

use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{ClassLikeMember, Modifier, Node, NodeKind, TraitUse};

use super::{ASSERT, AnyCall, is_in_test_class, receiver_is, resolve};
use crate::rules::support::class_members::walk;
use crate::rules::support::statement_edits::line_indent;
use crate::rules::types::class_scope;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const PROPHECY_TRAIT: &str = "Prophecy\\PhpUnit\\ProphecyTrait";

/// The edit that makes `member` (lines without indentation) the class's first member.
fn prepend_member(ctx: &Context<'_>, class: &mago_syntax::cst::Class<'_>, member: &str) -> Edit {
    let indent = format!("{}    ", line_indent(ctx.source, node_span(class).start));
    let at = class.left_brace.end.offset as usize;
    let mut text = String::new();
    for line in member.lines() {
        text.push('\n');
        if !line.is_empty() {
            text.push_str(&indent);
            text.push_str(line);
        }
    }
    if class.members.is_empty() {
        let close = class.right_brace.start.offset as usize;
        let before = &ctx.source[at..close];
        if before.trim().is_empty() {
            text.push('\n');
            text.push_str(line_indent(ctx.source, close));
            return Edit::new(Span::new(at, close), text);
        }
    }
    Edit::new(Span::new(at, at), text)
}

fn in_test_class(ctx: &Context<'_>, node: Node<'_, '_>, ancestors: &[Node<'_, '_>]) -> bool {
    let mut chain = ancestors.to_vec();
    chain.push(node);
    is_in_test_class(ctx, &chain, None)
}

/// `AddProphecyTraitRector`.
pub struct AddProphecyTrait;

impl Rule for AddProphecyTrait {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-prophecy-trait"
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
        if !in_test_class(ctx, node, ancestors) {
            return;
        }
        // TestsNodeAnalyzer::isAssertMethodCallName($node, 'prophesize')
        let mut prophesizes = false;
        walk(node, &mut |n, path| {
            if prophesizes {
                return false;
            }
            if let Some(call) = AnyCall::of(n)
                && call.is_name(&["prophesize"])
            {
                let mut chain = ancestors.to_vec();
                chain.extend_from_slice(path);
                chain.push(n);
                prophesizes = receiver_is(ctx, &chain, &call, &[ASSERT]);
            }
            !prophesizes
        });
        if !prophesizes || uses_prophecy_trait(ctx, node, ancestors, class.members.iter()) {
            return;
        }
        out.push(Report::fixed(
            node_span(&class.name),
            "Use `ProphecyTrait`; PHPUnit 10 no longer has `prophesize()`.",
            Safety::Safe,
            vec![prepend_member(
                ctx,
                class,
                &format!("use \\{PROPHECY_TRAIT};"),
            )],
        ));
    }
}

fn uses_prophecy_trait<'a>(
    ctx: &Context<'_>,
    node: Node<'_, '_>,
    ancestors: &[Node<'_, '_>],
    members: impl Iterator<Item = &'a ClassLikeMember<'a>>,
) -> bool {
    let uses: Vec<&TraitUse<'_>> = members
        .filter_map(|m| match m {
            ClassLikeMember::TraitUse(t) => Some(t),
            _ => None,
        })
        .collect();
    if uses.iter().any(|u| {
        u.trait_names
            .iter()
            .any(|t| resolve(ancestors, t).eq_ignore_ascii_case(PROPHECY_TRAIT))
    }) {
        return true;
    }
    let mut chain = ancestors.to_vec();
    chain.push(node);
    let Some(name) = class_scope(&chain).and_then(|s| s.name) else {
        return false;
    };
    let facts = facts(ctx);
    let codebase = &facts.codebase;
    let Some(metadata) = codebase.get_class_like(name.as_bytes()) else {
        return false;
    };
    std::iter::once(name.clone())
        .chain(metadata.all_parent_classes.iter().map(ToString::to_string))
        .filter_map(|c| codebase.get_class_like(c.as_bytes()))
        .any(|c| {
            c.used_traits
                .iter()
                .any(|t| t.to_string().eq_ignore_ascii_case(PROPHECY_TRAIT))
        })
}

/// `ParentTestClassConstructorRector`.
pub struct AddParentTestConstructor;

impl Rule for AddParentTestConstructor {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-parent-test-constructor"
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
        if !in_test_class(ctx, node, ancestors) {
            return;
        }
        // loaded automatically by PHPUnit
        let name = class.name.value;
        if class
            .modifiers
            .iter()
            .any(|m| matches!(m, Modifier::Abstract(_)))
            || name.ends_with(b"Test")
            || name.ends_with(b"TestCase")
        {
            return;
        }
        // it already has a constructor, skip as it might require specific tweaking
        if class.members.iter().any(|m| {
            matches!(m, ClassLikeMember::Method(method) if method.name.value.eq_ignore_ascii_case(b"__construct"))
        }) {
            return;
        }
        out.push(Report::fixed(
            node_span(&class.name),
            "Add a constructor that passes the class name to `TestCase`, as PHPUnit 10 requires of a non-test class.",
            Safety::Unsafe,
            vec![prepend_member(
                ctx,
                class,
                "public function __construct()\n{\n    parent::__construct(static::class);\n}",
            )],
        ));
    }
}
