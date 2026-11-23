use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{Expression, Node, NodeKind, Statement};

use super::{AnyCall, is_in_test_class};
use crate::rules::support::expressions::is_constant;
use crate::rules::support::statement_edits::line_indent;
use crate::rules::types::any_scoped;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct AddWillReturnCallbackFalseFallback;

impl Rule for AddWillReturnCallbackFalseFallback {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-will-return-callback-false-fallback"
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
        let call = AnyCall::of(node).unwrap();
        if !is_in_test_class(ctx, ancestors, None) || !call.is_name(&["willReturnCallback"]) {
            return;
        }
        let args = call.args();
        let [arg] = args[..] else { return };
        let Expression::Closure(closure) = arg.value() else {
            return;
        };
        // already ends with an explicit return, no fallback needed; a trailing `;` is no statement
        let Some(last) = closure
            .body
            .statements
            .iter()
            .rfind(|s| !matches!(s, Statement::Noop(_)))
        else {
            return;
        };
        if matches!(last, Statement::Return(_)) {
            return;
        }
        // the closure cannot fall through: its last statement throws or calls a never-returning method
        if let Statement::Expression(e) = last {
            let never = matches!(e.expression, Expression::Throw(_))
                || facts(ctx)
                    .full_type(node_span(e.expression))
                    .is_some_and(|t| !t.types.is_empty() && t.is_never());
            if never {
                return;
            }
        }
        if !has_only_bool_returns(Node::Block(&closure.body)) {
            return;
        }
        let end = node_span(last).end;
        let indent = line_indent(ctx.source, node_span(last).start);
        out.push(Report::fixed(
            node_span(closure),
            "Add `return false;` to the end of the callback, which only returns booleans.",
            Safety::Unsafe,
            vec![Edit::new(
                Span::new(end, end),
                format!("\n{indent}return false;"),
            )],
        ));
    }
}

fn has_only_bool_returns(body: Node<'_, '_>) -> bool {
    let mut found = false;
    let other = any_scoped(body, &mut |n| match n {
        Node::Return(r) => {
            found = true;
            !r.value
                .is_some_and(|v| is_constant(v, "true") || is_constant(v, "false"))
        }
        _ => false,
    });
    found && !other
}
