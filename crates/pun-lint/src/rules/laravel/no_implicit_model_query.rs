//! `lint/laravel/no-implicit-model-query`: a static call of a query builder method on an Eloquent model
//! (`User::where(...)`), which `__callStatic` forwards to a new query, starts the query with `query()`
//! (`User::query()->where(...)`). As rector-laravel 2.7.3's `EloquentMagicMethodToQueryBuilderRector`, the class must
//! be a `Model` that does not declare the method itself (`query_builder::is_proxy_call`); a class that is no model, or
//! that Pun cannot find, is left alone. As Mago's `no-implicit-model-query`, only the builder methods below count and
//! only a class written by name.

use mago_syntax::cst::{Expression, Node, NodeKind};

use super::query_builder::{call_parts, is_matching_call};
use crate::problem::{Edit, Safety, Span};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoImplicitModelQuery;

impl Rule for NoImplicitModelQuery {
    fn id(&self) -> &'static str {
        "lint/laravel/no-implicit-model-query"
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
        let Node::StaticMethodCall(s) = node else {
            return;
        };
        let Some(call) = call_parts(&node) else {
            return;
        };
        if !matches!(unparenthesize(call.on), Expression::Identifier(_))
            || !QUERY_BUILDER_METHODS
                .iter()
                .any(|m| is_matching_call(ctx, ancestors, &call, m))
        {
            return;
        }
        let method = node_span(&s.method).start;
        out.push(Report::fixed(
            node_span(s),
            format!(
                "`{}()` is forwarded to a new query implicitly; start the chain with `query()`.",
                String::from_utf8_lossy(call.name)
            ),
            Safety::Unsafe,
            vec![Edit::new(Span::new(method, method), "query()->")],
        ));
    }
}

// Public, non-static methods of the Eloquent or query builder that `Model` lacks, the ones
// rector-laravel's EloquentMagicMethodToQueryBuilderRector rewrites: not `with` (a static `Model` method) or
// SoftDeletes' `withTrashed`, `withoutTrashed` and `onlyTrashed` (builder macros).
const QUERY_BUILDER_METHODS: &[&str] = &[
    "where",
    "orWhere",
    "whereNot",
    "whereIn",
    "whereNotIn",
    "whereNull",
    "whereNotNull",
    "whereBetween",
    "whereNotBetween",
    "whereDate",
    "whereTime",
    "whereColumn",
    "whereExists",
    "whereHas",
    "whereDoesntHave",
    "whereRelation",
    "whereBelongsTo",
    "orderBy",
    "orderByDesc",
    "latest",
    "oldest",
    "inRandomOrder",
    "groupBy",
    "having",
    "withCount",
    "withSum",
    "withMax",
    "withMin",
    "withAvg",
    "withWhereHas",
    "has",
    "orHas",
    "doesntHave",
    "limit",
    "take",
    "skip",
    "offset",
    "distinct",
];
