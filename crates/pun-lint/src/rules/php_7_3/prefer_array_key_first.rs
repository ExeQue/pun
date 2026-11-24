//! Rector 2.8.4's `ArrayKeyFirstLastRector`.

use mago_syntax::cst::{Call, Expression, FunctionCall, Node, NodeKind, Statement};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{equal, named};
use crate::rules::support::statement_edits::{HOLDERS, List, holder_list, remove};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

/// `PREVIOUS_TO_NEW_FUNCTIONS`.
const PREVIOUS_TO_NEW_FUNCTIONS: &[(&str, &str)] =
    &[("reset", "array_key_first"), ("end", "array_key_last")];

pub struct PreferArrayKeyFirst;

impl Rule for PreferArrayKeyFirst {
    fn id(&self) -> &'static str {
        "lint/php-7.3/prefer-array-key-first"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70300)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(HOLDERS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let raw = crate::syntax::statements(node);
        if !raw.iter().any(|s| reset_or_end(s).is_some()) {
            return;
        }
        let Some(list) = holder_list(ctx.source, node, ancestors) else {
            return;
        };
        let mut change = Change::default();
        process(ctx, &list, 0, &mut change);
        if change.removed.is_empty() {
            return;
        }
        let mut edits = remove(ctx, &list, &change.removed);
        edits.extend(change.renamed);
        out.push(Report::fixed(
            node_span(list.stmts[change.removed[0]]),
            "Read the first or last key with `array_key_first()` or `array_key_last()` instead of moving the array's pointer.",
            Safety::Unsafe,
            edits,
        ));
    }
}

#[derive(Default)]
struct Change {
    removed: Vec<usize>,
    renamed: Vec<Edit>,
}

/// `processArrayKeyFirstLast`.
fn process(ctx: &Context<'_>, list: &List<'_>, jump_to_key: usize, change: &mut Change) {
    let total_keys = list.stmts.len().saturating_sub(1);
    for key in jump_to_key..total_keys {
        let Some(reset) = reset_or_end(list.stmts[key]) else {
            continue;
        };
        let Some(key_call) = resolve_key_func_call(ctx, list.stmts[key + 1], reset) else {
            continue;
        };
        if has_internal_pointer_change_next(ctx, list, key + 1, key_call) {
            continue;
        }
        let new_name = PREVIOUS_TO_NEW_FUNCTIONS
            .iter()
            .find(|(old, _)| named(reset, old))
            .map(|(_, new)| *new)
            .expect("reset_or_end matched one");
        change
            .renamed
            .push(Edit::new(node_span(key_call.function), new_name));
        change_next_key_call(ctx, list, key + 2, reset, new_name, change);
        change.removed.insert(0, key);
        change.removed.sort_unstable();
        return;
    }
}

/// `changeNextKeyCall`.
fn change_next_key_call(
    ctx: &Context<'_>,
    list: &List<'_>,
    from: usize,
    reset: &FunctionCall<'_>,
    new_name: &str,
    change: &mut Change,
) {
    for next_key in from..list.stmts.len() {
        if reset_or_end(list.stmts[next_key]).is_some() {
            process(ctx, list, next_key, change);
            break;
        }
        if let Some(key_call) = resolve_key_func_call(ctx, list.stmts[next_key], reset) {
            change
                .renamed
                .push(Edit::new(node_span(key_call.function), new_name));
        }
    }
}

/// `shouldSkip`, inverted: the `reset()` or `end()` call of an expression statement.
fn reset_or_end<'a>(stmt: &'a Statement<'a>) -> Option<&'a FunctionCall<'a>> {
    let Statement::Expression(e) = stmt else {
        return None;
    };
    match unparenthesize(e.expression) {
        Expression::Call(Call::Function(call)) if named(call, "reset") || named(call, "end") => {
            Some(call)
        }
        _ => None,
    }
}

/// `resolveKeyFuncCall`: the first `key()` call in `stmt` on the same argument.
fn resolve_key_func_call<'a>(
    ctx: &Context<'_>,
    stmt: &'a Statement<'a>,
    reset: &FunctionCall<'_>,
) -> Option<&'a FunctionCall<'a>> {
    let reset_arg = reset.argument_list.arguments.first()?;
    find(Node::Statement(stmt), &|node| match node {
        Node::FunctionCall(call) if named(call, "key") => call
            .argument_list
            .arguments
            .first()
            .is_some_and(|arg| equal(ctx, value(arg), value(reset_arg))),
        _ => false,
    })
    .and_then(|node| match node {
        Node::FunctionCall(call) => Some(call),
        _ => None,
    })
}

/// `hasInternalPointerChangeNext`: a `prev()` or `next()` on the array after the `key()` call.
fn has_internal_pointer_change_next(
    ctx: &Context<'_>,
    list: &List<'_>,
    from: usize,
    key_call: &FunctionCall<'_>,
) -> bool {
    let Some(key_arg) = key_call.argument_list.arguments.first() else {
        return false;
    };
    list.stmts[from..].iter().any(|stmt| {
        find(Node::Statement(stmt), &|node| match node {
            Node::FunctionCall(call) if named(call, "prev") || named(call, "next") => call
                .argument_list
                .arguments
                .first()
                .is_some_and(|arg| equal(ctx, value(arg), value(key_arg))),
            Node::FunctionPartialApplication(partial) => {
                matches!(partial.function, Expression::Identifier(id)
                    if [b"prev".as_slice(), b"next"].iter().any(|n| id.value().strip_prefix(b"\\").unwrap_or(id.value()).eq_ignore_ascii_case(n)))
            }
            _ => false,
        })
        .is_some()
    })
}

fn find<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    f: &dyn Fn(Node<'ast, 'arena>) -> bool,
) -> Option<Node<'ast, 'arena>> {
    if f(node) {
        return Some(node);
    }
    node.children().into_iter().find_map(|child| find(child, f))
}
