use mago_syntax::cst::{Expression, Node, NodeKind, UnaryPrefixOperator};

use super::{get_arg, replace};
use crate::rules::refactor::{called, function_name};
use crate::rules::support::expressions::text;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const FILE_DELETE: &str = "\\Illuminate\\Support\\Facades\\File::delete";

pub struct PreferFileFacadeDelete;

impl Rule for PreferFileFacadeDelete {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-file-facade-delete"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall, NodeKind::FunctionPartialApplication])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(Expression::Identifier(name)) = called(node) else {
            return;
        };
        if !function_name(ctx, ancestors, name).eq_ignore_ascii_case("unlink") {
            return;
        }
        let code = match node {
            Node::FunctionCall(call) => {
                // the stream context argument has no equivalent on the facade
                if call.argument_list.arguments.len() != 1 {
                    return;
                }
                let Some(arg) = get_arg(&call.argument_list, "filename", 0) else {
                    return;
                };
                format!("{FILE_DELETE}({})", text(ctx, arg.value()))
            }
            _ => format!("{FILE_DELETE}(...)"),
        };
        // File::delete() suppresses the failure itself, so the suppressor is dropped
        let span = node_span(&node);
        let suppressor = ancestors.iter().rev().find_map(|a| match a {
            Node::UnaryPrefix(u) => Some(*u),
            _ => None,
        });
        let span = match suppressor {
            Some(u)
                if matches!(u.operator, UnaryPrefixOperator::ErrorControl(_))
                    && node_span(unparenthesize(u.operand)) == span =>
            {
                node_span(u)
            }
            _ => span,
        };
        out.push(replace(
            span,
            "Use `File::delete()` instead of `unlink()`.".to_string(),
            code,
        ));
    }
}
