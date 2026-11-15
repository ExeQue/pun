use mago_syntax::cst::{Node, NodeKind};

use super::{in_string, is_by_ref_argument, is_variable, is_written, replace};
use crate::rules::support::expressions::text;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferEnvHelper;

impl Rule for PreferEnvHelper {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-env-helper"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ArrayAccess])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::ArrayAccess(access) = node else {
            return;
        };
        if !is_variable(access.array, "_ENV") {
            return;
        }
        let span = node_span(access);
        if is_written(span, ancestors)
            || is_by_ref_argument(ctx, span, ancestors)
            || in_string(ancestors)
        {
            return;
        }
        out.push(replace(
            span,
            "Use `Env::get()` instead of reading `$_ENV`.".to_string(),
            format!(
                "\\Illuminate\\Support\\Env::get({})",
                text(ctx, access.index)
            ),
        ));
    }
}
