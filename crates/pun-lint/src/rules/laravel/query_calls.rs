//! rector-laravel 2.7.3's rules in no set that rewrite a query builder or cache call:
//! `WhereNullComparisonToWhereNullRector`, `DateWhereClauseToShorthandRector`, `WhereToWhereLikeRector` and
//! `MinutesToSecondsInCacheRector`.

use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{
    Argument, ArgumentList, Call, ClassLikeMemberSelector, Expression, Literal, Node, NodeKind,
};

use super::helper_calls::is_function;
use super::object_type::is_object_type;
use super::print::reprinted_call;
use super::query_builder::{call_parts, is_matching_call};
use super::{is_a, member_name};
use crate::problem::{Edit, Safety, Span};
use crate::rules::support::expressions::{Prec, prec, text};
use crate::rules::support::names::resolve_class_name;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

const QUERY_BUILDER: &str = "Illuminate\\Contracts\\Database\\Query\\Builder";
const MODEL: &str = "Illuminate\\Database\\Eloquent\\Model";

/// The parts of a method or static call that these rules edit.
struct Parts<'a> {
    on: &'a Expression<'a>,
    is_static: bool,
    selector: &'a ClassLikeMemberSelector<'a>,
    list: &'a ArgumentList<'a>,
}

fn parts<'a>(node: Node<'a, 'a>) -> Option<Parts<'a>> {
    match node {
        Node::MethodCall(m) => Some(Parts {
            on: m.object,
            is_static: false,
            selector: &m.method,
            list: &m.argument_list,
        }),
        Node::StaticMethodCall(s) => Some(Parts {
            on: s.class,
            is_static: true,
            selector: &s.method,
            list: &s.argument_list,
        }),
        _ => None,
    }
}

fn string_literal(expr: &Expression<'_>) -> Option<String> {
    match expr {
        Expression::Literal(Literal::String(s)) => {
            s.value.map(|v| String::from_utf8_lossy(v).into_owned())
        }
        _ => None,
    }
}

/// Keep only the first argument: remove everything after it up to the last argument's end.
fn keep_first(list: &ArgumentList<'_>) -> Option<Edit> {
    let first = node_span(list.arguments.first()?);
    let last = node_span(list.arguments.last()?);
    (last.end > first.end).then(|| Edit::new(Span::new(first.end, last.end), ""))
}

/// `WhereNullComparisonToWhereNullRector`.
pub struct PreferWhereNull;

impl Rule for PreferWhereNull {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-where-null"
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
        if !member_name(&call.method).is_some_and(|n| super::is_name(n, "where"))
            || !is_object_type(ctx, ancestors, call.object, QUERY_BUILDER)
        {
            return;
        }
        let facts = facts(ctx);
        let type_of = |a: &Argument<'_>| facts.full_type(node_span(a.value()));
        let args: Vec<&Argument<'_>> = call.argument_list.arguments.iter().collect();
        let mut method = "whereNull";
        let null = match args[..] {
            [first] => type_of(first).is_some_and(TUnion::is_string),
            [_, value] => type_of(value).is_some_and(TUnion::is_null),
            [_, operator, value] => {
                if type_of(operator)
                    .and_then(|t| t.get_single_literal_string_value())
                    .is_some_and(|v| v == b"!=")
                {
                    method = "whereNotNull";
                }
                type_of(value).is_some_and(TUnion::is_null)
            }
            _ => false,
        };
        if !null {
            return;
        }
        out.push(Report::fixed(
            node_span(call),
            format!("Use `{method}()` instead of comparing with `null` in `where()`."),
            Safety::Unsafe,
            reprinted_call(
                ctx.source,
                call.object,
                node_span(call).end,
                &format!("->{method}({})", text(ctx, args[0])),
            ),
        ));
    }
}

const WHERE_NOW_METHODS: [(&str, [(&str, &str); 4]); 2] = [
    (
        "where",
        [
            ("<", "wherePast"),
            ("<=", "whereNowOrPast"),
            (">", "whereFuture"),
            (">=", "whereNowOrFuture"),
        ],
    ),
    (
        "orWhere",
        [
            ("<", "orWherePast"),
            ("<=", "orWhereNowOrPast"),
            (">", "orWhereFuture"),
            (">=", "orWhereNowOrFuture"),
        ],
    ),
];

const WHERE_TODAY_METHODS: [(&str, [(&str, &str); 5]); 2] = [
    (
        "whereDate",
        [
            ("=", "whereToday"),
            ("<", "whereBeforeToday"),
            ("<=", "whereTodayOrBefore"),
            (">", "whereAfterToday"),
            (">=", "whereTodayOrAfter"),
        ],
    ),
    (
        "orWhereDate",
        [
            ("=", "orWhereToday"),
            ("<", "orWhereBeforeToday"),
            ("<=", "orWhereTodayOrBefore"),
            (">", "orWhereAfterToday"),
            (">=", "orWhereTodayOrAfter"),
        ],
    ),
];

/// `DateWhereClauseToShorthandRector`.
pub struct PreferDateShorthand;

impl PreferDateShorthand {
    fn is_date_call(
        ctx: &Context<'_>,
        ancestors: &[Node<'_, '_>],
        expr: &Expression<'_>,
        name: &str,
    ) -> bool {
        match expr {
            Expression::Call(Call::Function(f)) => {
                is_function(ctx, ancestors, f.function, name)
                    && f.argument_list.arguments.is_empty()
            }
            Expression::Call(Call::StaticMethod(s)) => {
                member_name(&s.method).is_some_and(|n| super::is_name(n, name))
                    && s.argument_list.arguments.is_empty()
                    && matches!(s.class, Expression::Identifier(id)
                        if is_a(ctx, &resolve_class_name(ancestors, id), &["DateTimeInterface"]))
            }
            _ => false,
        }
    }
}

impl Rule for PreferDateShorthand {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-date-shorthand"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall, NodeKind::StaticMethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (Some(p), Some(call)) = (parts(node), call_parts(&node)) else {
            return;
        };
        let name = String::from_utf8_lossy(call.name).into_owned();
        if !is_matching_call(ctx, ancestors, &call, &name) {
            return;
        }
        let args = &call.arguments;
        let new = if let Some((_, map)) = WHERE_NOW_METHODS.iter().find(|(m, _)| *m == name) {
            let Some(operator) = (args.len() == 3).then(|| string_literal(args[1])).flatten()
            else {
                return;
            };
            let value = args[2];
            let Some((_, new)) = map.iter().find(|(o, _)| *o == operator) else {
                return;
            };
            if !Self::is_date_call(ctx, ancestors, value, "now") {
                return;
            }
            *new
        } else if let Some((_, map)) = WHERE_TODAY_METHODS.iter().find(|(m, _)| *m == name) {
            let (operator, value) = match args.len() {
                2 => ("=".to_string(), args[1]),
                3 => match string_literal(args[1]) {
                    Some(operator) => (operator, args[2]),
                    None => return,
                },
                _ => return,
            };
            let Some((_, new)) = map.iter().find(|(o, _)| *o == operator) else {
                return;
            };
            if !Self::is_date_call(ctx, ancestors, value, "today") {
                return;
            }
            *new
        } else {
            return;
        };
        let mut edits = vec![Edit::new(node_span(p.selector), new)];
        edits.extend(keep_first(p.list));
        out.push(Report::fixed(
            node_span(&node),
            format!("Use `{new}()` instead of comparing with the current date."),
            Safety::Unsafe,
            edits,
        ));
    }
}

/// `WhereToWhereLikeRector`.
#[derive(Default)]
pub struct PreferWhereLike {
    pub using_postgres_driver: bool,
}

impl Rule for PreferWhereLike {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-where-like"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.using_postgres_driver = false;
        let serde_json::Value::Object(map) = options else {
            return match options {
                serde_json::Value::Null => Ok(()),
                _ => Err("options must be a table".into()),
            };
        };
        for (key, value) in map {
            match (key.as_str(), value) {
                ("using-postgres-driver", serde_json::Value::Bool(b)) => {
                    self.using_postgres_driver = *b;
                }
                ("using-postgres-driver", _) => {
                    return Err("`using-postgres-driver` must be a boolean".into());
                }
                _ => {
                    return Err(format!(
                        "unknown option `{key}`; the option is using-postgres-driver"
                    )
                    .into());
                }
            }
        }
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall, NodeKind::StaticMethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(p) = parts(node) else { return };
        let target = if p.is_static { MODEL } else { QUERY_BUILDER };
        if !is_object_type(ctx, ancestors, p.on, target) {
            return;
        }
        let Some(name) = member_name(p.selector) else {
            return;
        };
        let new = match name.to_ascii_lowercase().as_slice() {
            b"where" => "whereLike",
            b"orwhere" => "orWhereLike",
            _ => return,
        };
        let args: Vec<&Argument<'_>> = p.list.arguments.iter().collect();
        if args.len() != 3
            || is_object_type(
                ctx,
                ancestors,
                args[2].value(),
                "Illuminate\\Contracts\\Database\\Query\\Expression",
            )
        {
            return;
        }
        let Some(like) = string_literal(args[1].value()).map(|s| s.to_lowercase()) else {
            return;
        };
        if ![
            "like",
            "like binary",
            "ilike",
            "not like",
            "not like binary",
            "not ilike",
        ]
        .contains(&like.as_str())
        {
            return;
        }
        let new = if like.contains("not") {
            new.replace("Like", "NotLike")
        } else {
            new.to_string()
        };
        let mut edits = vec![
            Edit::new(node_span(p.selector), new.clone()),
            Edit::new(
                Span::new(node_span(args[0]).end, node_span(args[1]).end),
                "",
            ),
        ];
        let case_sensitive = matches!(like.as_str(), "like binary" | "not like binary")
            || (self.using_postgres_driver && matches!(like.as_str(), "like" | "not like"));
        if case_sensitive {
            let end = node_span(args[2]).end;
            let flag = match args[2] {
                Argument::Named(_) => ", caseSensitive: true",
                Argument::Positional(_) => ", true",
            };
            edits.push(Edit::new(Span::new(end, end), flag));
        }
        out.push(Report::fixed(
            node_span(&node),
            format!("Use `{new}()` instead of a `like` comparison in `where()`."),
            Safety::Unsafe,
            edits,
        ));
    }
}

const CACHE_METHODS: [(&str, &str, usize); 10] = [
    ("Illuminate\\Support\\Facades\\Cache", "put", 2),
    ("Illuminate\\Contracts\\Cache\\Repository", "put", 2),
    ("Illuminate\\Contracts\\Cache\\Store", "put", 2),
    ("Illuminate\\Contracts\\Cache\\Repository", "add", 2),
    ("Illuminate\\Contracts\\Cache\\Store", "add", 2),
    ("Illuminate\\Support\\Facades\\Cache", "add", 2),
    ("Illuminate\\Contracts\\Cache\\Repository", "remember", 2),
    ("Illuminate\\Support\\Facades\\Cache", "remember", 2),
    ("Illuminate\\Contracts\\Cache\\Store", "remember", 2),
    ("Illuminate\\Contracts\\Cache\\Store", "putMany", 1),
];

/// `MinutesToSecondsInCacheRector`.
pub struct PreferCacheSeconds;

impl Rule for PreferCacheSeconds {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-cache-seconds"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall, NodeKind::StaticMethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(p) = parts(node) else { return };
        let Some(name) = member_name(p.selector) else {
            return;
        };
        let Some((_, _, position)) = CACHE_METHODS.iter().find(|(class, method, position)| {
            is_object_type(ctx, ancestors, p.on, class)
                && super::is_name(name, method)
                && p.list.arguments.get(*position).is_some()
        }) else {
            return;
        };
        let value = p.list.arguments.nodes[*position].value();
        let numeric = facts(ctx)
            .full_type(node_span(value))
            .is_some_and(|t| !t.types.is_empty() && (t.is_int() || t.is_float()));
        if !numeric
            || matches!(unparenthesize(value), Expression::Binary(b)
                if matches!(b.operator, mago_syntax::cst::BinaryOperator::Multiplication(_)))
        {
            return;
        }
        // the left precedence of `*` (support::expressions::binary)
        let left = 41;
        let code = text(ctx, value);
        let code = match prec(value) {
            Prec::Infix(p, ..) | Prec::Prefix(p) if p >= left => format!("({code})"),
            _ => code.to_string(),
        };
        let span = node_span(value);
        out.push(Report::fixed(
            span,
            "Pass the cache lifetime in seconds: multiply the minutes by 60.",
            Safety::Unsafe,
            vec![Edit::new(span, format!("{code} * 60"))],
        ));
    }
}
