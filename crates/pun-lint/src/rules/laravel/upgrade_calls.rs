//! rector-laravel 2.7.3's upgrade rules below Laravel 12 that rewrite one call or cast:
//! `Redirect301ToPermanentRedirectRector` (laravel-5.7), `ReplaceFakerInstanceWithHelperRector` (laravel-9),
//! `DatabaseExpressionCastsToMethodCallRector`, `DatabaseExpressionToStringToMethodCallRector`,
//! `DispatchNonShouldQueueToDispatchSyncRector`, `ReplaceAssertTimesSendWithAssertSentTimesRector`,
//! `ReplaceWithoutJobsEventsAndNotificationsWithFacadeFakeRector` (laravel-10), `AssertSeeToAssertSeeHtmlRector` and
//! `RefactorBlueprintGeometryColumnsRector` (laravel-11).

use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{
    Argument, ArgumentList, Call, ClassLikeMemberSelector, Expression, Node, NodeKind, StringPart,
    UnaryPrefixOperator, Variable,
};

use super::helper_calls::is_function;
use super::object_type::{is_object_type, object_class};
use super::{class_name, is_a, member_name, replace};
use crate::problem::{Edit, Safety, Span};
use crate::rules::support::expressions::text;
use crate::rules::support::values::{Value, value};
use crate::rules::types::class_scope;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const DB: &str = "Illuminate\\Support\\Facades\\DB";
const GET_VALUE: &str =
    "getValue(\\Illuminate\\Support\\Facades\\DB::connection()->getQueryGrammar())";

/// The edits that rename `selector` and remove the argument at `index` from `list`.
fn remove_argument(list: &ArgumentList<'_>, index: usize) -> Option<Edit> {
    let args: Vec<&Argument<'_>> = list.arguments.iter().collect();
    let span = node_span(*args.get(index)?);
    Some(match (index.checked_sub(1), args.get(index + 1)) {
        (_, Some(next)) => Edit::new(Span::new(span.start, node_span(*next).start), ""),
        (Some(before), None) => Edit::new(Span::new(node_span(args[before]).end, span.end), ""),
        (None, None) => Edit::new(span, ""),
    })
}

fn rename(selector: &ClassLikeMemberSelector<'_>, name: &str) -> Edit {
    Edit::new(node_span(selector), name)
}

fn is_false(expr: &Expression<'_>) -> bool {
    matches!(value(expr), Some(Value::Bool(false)))
}

/// `DB::raw(...)`, with the class name exactly the `DB` facade.
fn is_db_raw(expr: &Expression<'_>, ancestors: &[Node<'_, '_>]) -> bool {
    let Expression::Call(Call::StaticMethod(call)) = unparenthesize(expr) else {
        return false;
    };
    class_name(call.class, ancestors).is_some_and(|c| c.eq_ignore_ascii_case(DB))
        && member_name(&call.method).is_some_and(|n| n.eq_ignore_ascii_case(b"raw"))
}

/// `Redirect301ToPermanentRedirectRector`.
pub struct PreferPermanentRedirect;

impl Rule for PreferPermanentRedirect {
    fn id(&self) -> &'static str {
        "lint/laravel-5.7/prefer-permanent-redirect"
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
        let Node::StaticMethodCall(call) = node else {
            return;
        };
        if ![
            "Illuminate\\Support\\Facades\\Route",
            "Illuminate\\Routing\\Route",
        ]
        .iter()
        .any(|c| is_object_type(ctx, ancestors, call.class, c))
        {
            return;
        }
        let Some(third) = call.argument_list.arguments.get(2) else {
            return;
        };
        if !matches!(value(third.value()), Some(Value::Int(301))) {
            return;
        }
        let Some(removed) = remove_argument(&call.argument_list, 2) else {
            return;
        };
        out.push(Report::fixed(
            node_span(call),
            "Use `permanentRedirect()` instead of a redirect with status 301.",
            Safety::Safe,
            vec![rename(&call.method, "permanentRedirect"), removed],
        ));
    }
}

/// `AssertSeeToAssertSeeHtmlRector`.
pub struct PreferAssertSeeHtml;

impl Rule for PreferAssertSeeHtml {
    fn id(&self) -> &'static str {
        "lint/laravel-11/prefer-assert-see-html"
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
        let Node::MethodCall(call) = node else { return };
        if !is_object_type(
            ctx,
            ancestors,
            call.object,
            "Illuminate\\Testing\\TestResponse",
        ) {
            return;
        }
        let new = match member_name(&call.method) {
            Some(b"assertSee") => "assertSeeHtml",
            Some(b"assertDontSee") => "assertDontSeeHtml",
            Some(b"assertSeeInOrder") => "assertSeeHtmlInOrder",
            _ => return,
        };
        let args: Vec<&Argument<'_>> = call.argument_list.arguments.iter().collect();
        if args.len() != 2 || !is_false(args[1].value()) {
            return;
        }
        let span = Span::new(node_span(&call.method).start, node_span(call).end);
        out.push(Report::fixed(
            node_span(call),
            format!("Use `{new}()` instead of passing `false` to escape the HTML."),
            Safety::Safe,
            vec![Edit::new(span, format!("{new}({})", text(ctx, args[0])))],
        ));
    }
}

/// `ReplaceAssertTimesSendWithAssertSentTimesRector`.
pub struct PreferAssertSentTimes;

impl Rule for PreferAssertSentTimes {
    fn id(&self) -> &'static str {
        "lint/laravel-10/prefer-assert-sent-times"
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
        let Node::StaticMethodCall(call) = node else {
            return;
        };
        if member_name(&call.method) != Some(b"assertTimesSent")
            || !is_object_type(
                ctx,
                ancestors,
                call.class,
                "Illuminate\\Support\\Facades\\Notification",
            )
        {
            return;
        }
        let mut edits = vec![rename(&call.method, "assertSentTimes")];
        let args: Vec<&Argument<'_>> = call.argument_list.arguments.iter().collect();
        if let (Some(first), Some(last)) = (args.first(), args.last())
            && args.len() > 1
        {
            let reversed: Vec<&str> = args.iter().rev().map(|a| text(ctx, *a)).collect();
            edits.push(Edit::new(
                Span::new(node_span(*first).start, node_span(*last).end),
                reversed.join(", "),
            ));
        }
        out.push(Report::fixed(
            node_span(call),
            "Use `assertSentTimes()` instead of `assertTimesSent()`.",
            Safety::Safe,
            edits,
        ));
    }
}

/// `DatabaseExpressionCastsToMethodCallRector`.
pub struct NoExpressionStringCast;

impl Rule for NoExpressionStringCast {
    fn id(&self) -> &'static str {
        "lint/laravel-10/no-expression-string-cast"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::UnaryPrefix])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::UnaryPrefix(cast) = node else {
            return;
        };
        if !matches!(
            cast.operator,
            UnaryPrefixOperator::StringCast(..) | UnaryPrefixOperator::BinaryCast(..)
        ) || !is_db_raw(cast.operand, ancestors)
        {
            return;
        }
        out.push(replace(
            node_span(cast),
            "Call `getValue()` on the database expression instead of casting it to a string."
                .to_string(),
            format!("{}->{GET_VALUE}", text(ctx, unparenthesize(cast.operand))),
        ));
    }
}

/// `DatabaseExpressionToStringToMethodCallRector`.
pub struct NoExpressionToString;

impl Rule for NoExpressionToString {
    fn id(&self) -> &'static str {
        "lint/laravel-10/no-expression-to-string"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::MethodCall(call) = node else { return };
        if !member_name(&call.method).is_some_and(|n| n.eq_ignore_ascii_case(b"__toString"))
            || !is_db_raw(call.object, ancestors)
        {
            return;
        }
        out.push(replace(
            Span::new(node_span(&call.method).start, node_span(call).end),
            "Call `getValue()` on the database expression instead of `__toString()`.".to_string(),
            GET_VALUE.to_string(),
        ));
    }
}

/// `RefactorBlueprintGeometryColumnsRector`.
pub struct PreferGeometryColumn;

impl Rule for PreferGeometryColumn {
    fn id(&self) -> &'static str {
        "lint/laravel-11/prefer-geometry-column"
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
        let Node::MethodCall(call) = node else { return };
        let Some(name) = member_name(&call.method) else {
            return;
        };
        let types = [
            "point",
            "linestring",
            "polygon",
            "geometrycollection",
            "multipoint",
            "multilinestring",
            "multipolygon",
        ];
        if !types
            .iter()
            .any(|t| name.eq_ignore_ascii_case(t.as_bytes()))
            || !is_object_type(
                ctx,
                ancestors,
                call.object,
                "Illuminate\\Database\\Schema\\Blueprint",
            )
        {
            return;
        }
        let name = String::from_utf8_lossy(name);
        let quoted = crate::rules::refactor::configured_codemods::single_quoted(&name);
        let insert = match call.argument_list.arguments.last() {
            Some(last) => {
                let end = node_span(last).end;
                Edit::new(Span::new(end, end), format!(", {quoted}"))
            }
            None => {
                let at = node_span(&call.argument_list.left_parenthesis).end;
                Edit::new(Span::new(at, at), quoted)
            }
        };
        out.push(Report::fixed(
            node_span(call),
            format!("Use `geometry()` with the `{name}` subtype instead of `{name}()`."),
            Safety::Unsafe,
            vec![rename(&call.method, "geometry"), insert],
        ));
    }
}

/// `ReplaceWithoutJobsEventsAndNotificationsWithFacadeFakeRector`.
pub struct PreferFacadeFake;

impl Rule for PreferFacadeFake {
    fn id(&self) -> &'static str {
        "lint/laravel-10/prefer-facade-fake"
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
        let Node::MethodCall(call) = node else { return };
        let (method, facade) = match member_name(&call.method) {
            Some(b"withoutJobs") => ("withoutJobs", "Bus"),
            Some(b"withoutEvents") => ("withoutEvents", "Event"),
            Some(b"withoutNotifications") => ("withoutNotifications", "Notification"),
            _ => return,
        };
        if !is_object_type(
            ctx,
            ancestors,
            call.object,
            "Illuminate\\Foundation\\Testing\\TestCase",
        ) {
            return;
        }
        out.push(replace(
            node_span(call),
            format!("Use `{facade}::fake()` instead of `{method}()`."),
            format!("\\Illuminate\\Support\\Facades\\{facade}::fake()"),
        ));
    }
}

/// `DispatchNonShouldQueueToDispatchSyncRector`.
pub struct PreferDispatchSync;

const SHOULD_QUEUE: &str = "Illuminate\\Contracts\\Queue\\ShouldQueue";
const BUS: &str = "Illuminate\\Support\\Facades\\Bus";

impl Rule for PreferDispatchSync {
    fn id(&self) -> &'static str {
        "lint/laravel-10/prefer-dispatch-sync"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::FunctionCall,
            NodeKind::MethodCall,
            NodeKind::StaticMethodCall,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let dispatch = |s: &ClassLikeMemberSelector<'_>| {
            member_name(s).is_some_and(|n| n.eq_ignore_ascii_case(b"dispatch"))
        };
        let (list, name_span, new, bus) = match node {
            Node::FunctionCall(f) if is_function(ctx, ancestors, f.function, "dispatch") => (
                &f.argument_list,
                node_span(f.function),
                "dispatch_sync",
                false,
            ),
            Node::MethodCall(m)
                if dispatch(&m.method)
                    && (is_dispatchable(ctx, m.object)
                        || is_object_type(
                            ctx,
                            ancestors,
                            m.object,
                            "Illuminate\\Contracts\\Bus\\Dispatcher",
                        )) =>
            {
                (
                    &m.argument_list,
                    node_span(&m.method),
                    "dispatchSync",
                    false,
                )
            }
            Node::StaticMethodCall(s)
                if dispatch(&s.method)
                    && class_name(s.class, ancestors)
                        .is_some_and(|c| c.eq_ignore_ascii_case(BUS)) =>
            {
                (&s.argument_list, node_span(&s.method), "dispatchSync", true)
            }
            _ => return,
        };
        let [arg] = &list.arguments.iter().collect::<Vec<_>>()[..] else {
            return;
        };
        let value = unparenthesize(arg.value());
        let facts = facts(ctx);
        let Some(t) = facts.full_type(node_span(value)) else {
            return;
        };
        if t.is_mixed() {
            return;
        }
        let queued = t.types.iter().any(|a| match a {
            TAtomic::Mixed(_) | TAtomic::Object(TObject::Any) => true,
            _ => object_class(a)
                .is_some_and(|c| is_a(ctx, &c, &[SHOULD_QUEUE]) || is_a(ctx, SHOULD_QUEUE, &[&c])),
        });
        if queued {
            return;
        }
        let closure = matches!(value, Expression::Closure(_) | Expression::ArrowFunction(_))
            || t.types.iter().any(|a| match a {
                TAtomic::Callable(c) => c.is_closure(),
                _ => object_class(a).is_some_and(|c| c.eq_ignore_ascii_case("Closure")),
            });
        if closure && !bus {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            format!("Use `{new}()` for a job that does not implement `ShouldQueue`."),
            Safety::Unsafe,
            vec![Edit::new(name_span, new)],
        ));
    }
}

/// The object's type is one class that uses `Dispatchable`.
fn is_dispatchable(ctx: &Context<'_>, object: &Expression<'_>) -> bool {
    let facts = facts(ctx);
    let Some(t) = facts.full_type(node_span(unparenthesize(object))) else {
        return false;
    };
    let [atomic] = &t.types[..] else { return false };
    object_class(atomic).is_some_and(|c| {
        facts.codebase.get_class_like(c.as_bytes()).is_some()
            && is_a(ctx, &c, &["Illuminate\\Foundation\\Bus\\Dispatchable"])
    })
}

/// `ReplaceFakerInstanceWithHelperRector`.
pub struct PreferFakeHelper;

const FACTORY: &str = "Illuminate\\Database\\Eloquent\\Factories\\Factory";

impl Rule for PreferFakeHelper {
    fn id(&self) -> &'static str {
        "lint/laravel-9/prefer-fake-helper"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::PropertyAccess,
            NodeKind::MethodCall,
            NodeKind::InterpolatedString,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let in_factory = class_scope(ancestors)
            .and_then(|s| s.name)
            .is_some_and(|c| !c.eq_ignore_ascii_case(FACTORY) && is_a(ctx, &c, &[FACTORY]));
        if !in_factory {
            return;
        }
        let random_enum = |n: &Node<'_, '_>| matches!(n, Node::MethodCall(m) if member_name(&m.method).is_some_and(|n| n.eq_ignore_ascii_case(b"randomEnum")));
        if random_enum(&node) {
            return;
        }
        if let Node::InterpolatedString(string) = node {
            if ancestors.iter().any(random_enum) {
                return;
            }
            let mut parts = Vec::new();
            let mut literal = String::new();
            let mut changed = false;
            for part in &string.parts {
                if let StringPart::BracedExpression(braced) = part
                    && let Some(code) = faker_reference(ctx, braced.expression)
                {
                    if !literal.is_empty() {
                        parts.push(format!("\"{}\"", std::mem::take(&mut literal)));
                    }
                    parts.push(code);
                    changed = true;
                } else {
                    literal.push_str(text(ctx, part));
                }
            }
            if !changed {
                return;
            }
            if !literal.is_empty() {
                parts.push(format!("\"{literal}\""));
            }
            out.push(replace(
                node_span(string),
                "Use the `fake()` helper instead of `$this->faker`.".to_string(),
                parts.join(" . "),
            ));
            return;
        }
        if ancestors
            .iter()
            .any(|a| matches!(a, Node::InterpolatedString(_)))
            || (matches!(node, Node::PropertyAccess(_)) && ancestors.iter().any(random_enum))
        {
            return;
        }
        let object = match node {
            Node::PropertyAccess(p) => p.object,
            Node::MethodCall(m) => m.object,
            _ => return,
        };
        if is_this_faker(object) {
            out.push(replace(
                node_span(object),
                "Use the `fake()` helper instead of `$this->faker`.".to_string(),
                "fake()".to_string(),
            ));
        }
    }
}

fn is_this_faker(expr: &Expression<'_>) -> bool {
    let Expression::Access(mago_syntax::cst::Access::Property(p)) = expr else {
        return false;
    };
    matches!(p.object, Expression::Variable(Variable::Direct(v)) if v.name == b"$this")
        && member_name(&p.property).is_some_and(|n| n.eq_ignore_ascii_case(b"faker"))
}

/// `$this->faker->x` or `$this->faker->x()` with `fake()` for `$this->faker`.
fn faker_reference(ctx: &Context<'_>, expr: &Expression<'_>) -> Option<String> {
    let object = match expr {
        Expression::Access(mago_syntax::cst::Access::Property(p)) => p.object,
        Expression::Call(Call::Method(m)) => m.object,
        _ => return None,
    };
    if !is_this_faker(object) {
        return None;
    }
    let all = node_span(expr);
    let rest = &ctx.source[node_span(object).end..all.end];
    Some(format!("fake(){rest}"))
}
