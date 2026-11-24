//! Rector 2.8.4's `ThisCallOnStaticMethodToStaticCallRector`, with `StaticAnalyzer::isStaticMethod`.

use mago_syntax::cst::{
    ClassLikeMember, ClassLikeMemberSelector, Modifier, Node, NodeKind, Sequence,
};

use crate::problem::{Edit, Safety, Span};
use crate::rules::laravel::is_a;
use crate::rules::phpunit::TEST_CASE;
use crate::rules::support::class_members::qualified;
use crate::rules::support::names::class_name;
use crate::syntax::{is_this, node_span};
use crate::{Context, Report, Rule};

pub struct PreferStaticMethodCall;

impl Rule for PreferStaticMethodCall {
    fn id(&self) -> &'static str {
        "lint/php-7.0/prefer-static-method-call"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70000)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::MethodCall(call) = node else {
            return;
        };
        if !is_this(call.object) {
            return;
        }
        let ClassLikeMemberSelector::Identifier(name) = &call.method else {
            return;
        };
        // Each enclosing class is refactored in turn, the outermost first, and its traversal skips interpolated
        // strings.
        for (i, ancestor) in ancestors.iter().enumerate() {
            let (members, is_final, own) = match ancestor {
                Node::Class(class) => (
                    &class.members,
                    class
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Final(_))),
                    Some(qualified(&ancestors[..i], class.name.value)),
                ),
                Node::AnonymousClass(class) => (&class.members, false, None),
                _ => continue,
            };
            if ancestors[i..]
                .iter()
                .any(|a| matches!(a, Node::InterpolatedString(_) | Node::DocumentString(_)))
            {
                return;
            }
            // skip PHPUnit calls, as they accept both self:: and $this-> formats
            let is_test_case = match (ancestor, own) {
                (_, Some(own)) => is_a(ctx, &own, &[TEST_CASE]),
                (Node::AnonymousClass(class), None) => class.extends.as_ref().is_some_and(|e| {
                    e.types
                        .iter()
                        .any(|p| is_a(ctx, &class_name(&ancestors[..i], p), &[TEST_CASE]))
                }),
                _ => false,
            };
            if is_test_case {
                continue;
            }
            let Some(private) = is_static_method(members, name.value) else {
                continue;
            };
            // `resolveClassSelf`.
            let reference = if is_final || private {
                "self"
            } else {
                "static"
            };
            let span = Span::new(node_span(call.object).start, node_span(&call.method).start);
            out.push(Report::fixed(
                node_span(call),
                format!(
                    "`{}()` is a static method; call it with `{reference}::`.",
                    String::from_utf8_lossy(name.value)
                ),
                Safety::Safe,
                vec![Edit::new(span, format!("{reference}::"))],
            ));
            return;
        }
    }
}

/// `StaticAnalyzer::isStaticMethod`: a static method the class declares. `Some(true)` when it is private. Its
/// `@method static` docblock fallback matches nothing in Rector 2.8.4's runs (the class's resolved PHPDoc is not
/// there), so it is left out.
fn is_static_method(members: &Sequence<'_, ClassLikeMember<'_>>, name: &[u8]) -> Option<bool> {
    let method = members.iter().find_map(|m| match m {
        ClassLikeMember::Method(m) if m.name.value.eq_ignore_ascii_case(name) => Some(m),
        _ => None,
    })?;
    method.is_static().then(|| {
        method
            .modifiers
            .iter()
            .any(|m| matches!(m, Modifier::Private(_)))
    })
}
