//! Rector 2.8.4's `ListEachRector`, with `AssignManipulator::matchListAndEach`, which `WhileEachToForeachRector`
//! shares.

use mago_syntax::cst::{
    ArrayElement, Assignment, Expression, FunctionCall, Node, NodeKind, Statement,
    TokenSeparatedSequence, UnaryPrefixOperator,
};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::expressions::{named, text};
use crate::rules::support::statement_edits::{Frame, enclosing, line_indent, replace_statement};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoListEach;

impl Rule for NoListEach {
    fn id(&self) -> &'static str {
        "lint/php-7.2/no-list-each"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70200)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Statement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Statement(statement @ Statement::Expression(e)) = node else {
            return;
        };
        let Expression::Assignment(assign) = unparenthesize(e.expression) else {
            return;
        };
        let Some((items, each)) = match_list_and_each(assign) else {
            return;
        };
        // `shouldSkipAssign`: two items, not both empty.
        let [first, second] = items.as_slice() else {
            return;
        };
        let args = text(ctx, &each.argument_list);
        let stmts: Vec<String> = match (first, second) {
            // only key: list($key, ) = each($values);
            (Some(key), None) => vec![format!("{} = key{args};", text(ctx, *key))],
            // only value: list(, $value) = each($values);
            (None, Some(value)) => vec![
                format!("{} = current{args};", text(ctx, *value)),
                format!("next{args};"),
            ],
            // both: list($key, $value) = each($values);
            (Some(key), Some(value)) => vec![
                format!("{} = key{args};", text(ctx, *key)),
                format!("{} = current{args};", text(ctx, *value)),
                format!("next{args};"),
            ],
            (None, None) => return,
        };
        out.push(Report::fixed(
            node_span(statement),
            "`each()` is removed in PHP 8.0; read the pair with `key()`, `current()` and `next()`.",
            Safety::Safe,
            replace_with_statements(ctx, statement, ancestors, &stmts),
        ));
    }
}

/// Replaces `statement` with `stmts`, one per line. Rector's printer drops the blank lines before a statement it
/// replaces with several, so they go too.
pub(crate) fn replace_with_statements(
    ctx: &Context<'_>,
    statement: &Statement<'_>,
    ancestors: &[Node<'_, '_>],
    stmts: &[String],
) -> Vec<Edit> {
    let (list, index) = enclosing(ctx.source, statement, ancestors);
    let mut edits = vec![replace_statement(ctx.source, &list, index, |indent| {
        stmts.join(&format!("\n{indent}"))
    })];
    let start = node_span(statement).start;
    let before = list.before(ctx, index);
    let gap = &ctx.source[before..start];
    if list.frame != Frame::Bare && gap.trim().is_empty() && gap.matches('\n').count() > 1 {
        edits.push(Edit::new(
            Span::new(before, start),
            format!("\n{}", line_indent(ctx.source, start)),
        ));
    }
    edits
}

/// `matchListAndEach`: `list(...) = each(...)` or `[...] = each(...)`, behind an optional `@`. The items are the
/// list's values in order, `None` for an empty one; a trailing comma adds an empty item, as PHP-Parser reads it.
pub(crate) fn match_list_and_each<'a>(
    assign: &'a Assignment<'a>,
) -> Option<(Vec<Option<&'a Expression<'a>>>, &'a FunctionCall<'a>)> {
    if !assign.operator.is_assign() {
        return None;
    }
    // could be behind error suppress
    let mut bare = unparenthesize(assign.rhs);
    if let Expression::UnaryPrefix(u) = bare
        && matches!(u.operator, UnaryPrefixOperator::ErrorControl(_))
    {
        bare = unparenthesize(u.operand);
    }
    let Expression::Call(mago_syntax::cst::Call::Function(each)) = bare else {
        return None;
    };
    let elements = match assign.lhs {
        Expression::List(list) => &list.elements,
        Expression::Array(array) => &array.elements,
        _ => return None,
    };
    // A first-class callable `each(...)` is a partial application, not a call.
    if !named(each, "each") {
        return None;
    }
    Some((items(elements)?, each))
}

fn items<'a>(
    elements: &'a TokenSeparatedSequence<'a, ArrayElement<'a>>,
) -> Option<Vec<Option<&'a Expression<'a>>>> {
    let mut items = Vec::new();
    for element in elements {
        items.push(match element {
            ArrayElement::Value(v) => Some(v.value),
            ArrayElement::KeyValue(kv) => Some(kv.value),
            ArrayElement::Missing(_) => None,
            ArrayElement::Variadic(_) => return None,
        });
    }
    if elements.has_trailing_token() {
        items.push(None);
    }
    Some(items)
}
