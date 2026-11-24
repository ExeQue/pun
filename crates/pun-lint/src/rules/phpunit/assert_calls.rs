//! rector-phpunit's rules in no set that rewrite one assertion call: `PreferPHPUnitSelfCallRector`,
//! `AssertClassToThisAssertRector` and `AssertThatToDirectAssertRector`.

use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{
    Argument, Call, ClassLikeMemberSelector, Expression, Modifier, Node, NodeKind,
};

use super::prefer_this_call::NON_ASSERT;
use super::{
    ASSERT, AnyCall, TEST_CASE, is_in_test_class, is_test_case_call_named, maybe_multiline,
    receiver_is, resolve, string_value,
};
use crate::rules::support::rewrites::list_removal;
use crate::rules::support::statement_edits::line_indent;
use crate::rules::types::class_scope;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

fn call_text(ctx: &Context<'_>, prefix: &str, name: &[u8], call: &AnyCall<'_>) -> String {
    let args: Vec<Span> = call.args.arguments.iter().map(node_span).collect();
    format!(
        "{prefix}{}({})",
        String::from_utf8_lossy(name),
        maybe_multiline(
            ctx,
            call.args.left_parenthesis.end.offset as usize,
            &args,
            line_indent(ctx.source, call.span.start),
        )
    )
}

/// `PreferPHPUnitSelfCallRector`.
pub struct PreferSelfCall;

impl Rule for PreferSelfCall {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-self-call"
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
        let Some(call) = AnyCall::of(node) else {
            return;
        };
        let Some(name) = call.name() else { return };
        let named = name.starts_with(b"assert") || NON_ASSERT.iter().any(|n| n.as_bytes() == name);
        if !named
            || !is_in_test_class(ctx, ancestors, None)
            || !receiver_is(ctx, ancestors, &call, &[TEST_CASE])
        {
            return;
        }
        // the method as the enclosing class sees it: declared static by TestCase or Assert
        let facts = facts(ctx);
        let Some(class) = class_scope(ancestors).and_then(|s| s.name) else {
            return;
        };
        let Some(metadata) = facts.codebase.get_class_like(class.as_bytes()) else {
            return;
        };
        let Some((_, id)) = (metadata.declaring_method_ids.iter())
            .find(|(m, _)| m.as_bytes().eq_ignore_ascii_case(name))
        else {
            return;
        };
        let declaring = id.get_class_name();
        let by_phpunit = [TEST_CASE, ASSERT]
            .iter()
            .any(|c| c.as_bytes().eq_ignore_ascii_case(declaring.as_bytes()));
        let is_static = (facts.codebase)
            .get_declaring_method(declaring.as_bytes(), name)
            .and_then(|m| m.method_metadata.as_ref())
            .is_some_and(|m| m.is_static);
        if !by_phpunit || !is_static {
            return;
        }
        out.push(Report::fixed(
            call.span,
            "Call the static PHPUnit method on `self`.",
            Safety::Safe,
            vec![Edit::new(call.span, call_text(ctx, "self::", name, &call))],
        ));
    }
}

/// `AssertClassToThisAssertRector`.
pub struct PreferThisAssert;

impl Rule for PreferThisAssert {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-this-assert"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::StaticMethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(call) = AnyCall::of(node) else {
            return;
        };
        let Expression::Identifier(class) = call.receiver else {
            return;
        };
        if !resolve(ancestors, class).eq_ignore_ascii_case(ASSERT) {
            return;
        }
        let ClassLikeMemberSelector::Identifier(name) = call.method else {
            return;
        };
        // the nearest method, through closures that have a `$this`
        let method = ancestors.iter().rev().find_map(|a| match a {
            Node::Closure(c) if c.r#static.is_some() => Some(None),
            Node::ArrowFunction(f) if f.r#static.is_some() => Some(None),
            Node::Function(_)
            | Node::Class(_)
            | Node::AnonymousClass(_)
            | Node::Interface(_)
            | Node::Trait(_)
            | Node::Enum(_) => Some(None),
            Node::Method(m) => Some(Some(*m)),
            _ => None,
        });
        let Some(Some(method)) = method else { return };
        if method
            .modifiers
            .iter()
            .any(|m| matches!(m, Modifier::Static(_)))
            || !is_in_test_class(ctx, ancestors, Some(method))
        {
            return;
        }
        out.push(Report::fixed(
            call.span,
            "Call the assertion on `$this` instead of on `Assert`.",
            Safety::Safe,
            vec![Edit::new(
                call.span,
                call_text(ctx, "$this->", name.value, &call),
            )],
        ));
    }
}

const IS_TO_ASSERT_METHOD_MAP: [(&str, &str); 10] = [
    ("isTrue", "assertTrue"),
    ("isFalse", "assertFalse"),
    ("isNull", "assertNull"),
    ("isEmpty", "assertEmpty"),
    ("isCountable", "assertIsCountable"),
    ("isArray", "assertIsArray"),
    ("isString", "assertIsString"),
    ("isInt", "assertIsInt"),
    ("isFloat", "assertIsFloat"),
    ("isBool", "assertIsBool"),
];

/// `AssertThatToDirectAssertRector`.
pub struct PreferDirectAssert;

impl Rule for PreferDirectAssert {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-direct-assert"
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
        let Some(call) = AnyCall::of(node) else {
            return;
        };
        if !is_test_case_call_named(ctx, ancestors, &call, &["assertThat"]) {
            return;
        }
        let args: Vec<&Argument<'_>> = call.args.arguments.iter().collect();
        let Some(Expression::Call(Call::Method(constraint))) = args.get(1).map(|a| a.value())
        else {
            return;
        };
        let ClassLikeMemberSelector::Identifier(constraint_name) = &constraint.method else {
            return;
        };
        let constraint_name = constraint_name.value;
        let name_span = call.name_span();
        let mut edits;
        if constraint_name == b"equalTo" {
            let Some(expected) = constraint.argument_list.arguments.first() else {
                return;
            };
            edits = vec![
                Edit::new(name_span, "assertEquals"),
                Edit::new(node_span(args[0]), super::text(ctx, expected)),
                Edit::new(node_span(args[1]), super::text(ctx, args[0])),
            ];
        } else {
            let new = match IS_TO_ASSERT_METHOD_MAP
                .iter()
                .find(|(is, _)| is.as_bytes().eq_ignore_ascii_case(constraint_name))
            {
                Some((_, assert)) => (*assert).to_string(),
                None if constraint_name.eq_ignore_ascii_case(b"isType") => {
                    let Some(expected) = constraint
                        .argument_list
                        .arguments
                        .first()
                        .and_then(|a| string_value(a.value()))
                    else {
                        return;
                    };
                    let mut chars = expected.chars();
                    let first = chars.next().map(|c| c.to_ascii_uppercase());
                    format!(
                        "assertIs{}{}",
                        first.map_or_else(String::new, String::from),
                        chars.as_str()
                    )
                }
                None => return,
            };
            let spans: Vec<Span> = args.iter().map(|a| node_span(*a)).collect();
            let mut removed = vec![false; spans.len()];
            removed[1] = true;
            edits = vec![Edit::new(name_span, new)];
            edits.extend(list_removal(&spans, &removed).unwrap_or_default());
        }
        out.push(Report::fixed(
            call.span,
            "Use the direct assertion instead of `assertThat()` with a constraint.",
            Safety::Safe,
            edits,
        ));
    }
}
