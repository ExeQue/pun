use crate::problem::{Safety, Span};
use mago_syntax::cst::{Expression, MethodCall, Node, NodeKind};

use super::{
    Is, Item, STATEMENT_LISTS, Trinary, comment_spans, expect_argument, expect_call, has_not,
    is_expect_chain, is_name, kept_text, method_call, rewrite_list, shift, statement_expression,
    statement_list, type_is,
};
use crate::syntax::{node_span, same_expression, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferToMatchArray;

type Expectation<'a> = (&'a Expression<'a>, Option<&'a Expression<'a>>);

impl Rule for PreferToMatchArray {
    fn id(&self) -> &'static str {
        "lint/pest/prefer-to-match-array"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(STATEMENT_LISTS)
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(list) = statement_list(ctx, node) else {
            return;
        };
        let stmts = &list.stmts;
        let chain = |i: usize| {
            stmts
                .get(i)
                .and_then(|s| statement_expression(s))
                .and_then(method_call)
                .filter(|c| is_expect_chain(c))
        };
        let mut items: Vec<Item<'_>> = Vec::new();
        let mut i = 0;
        let mut changed = false;
        while i < stmts.len() {
            let Some(first) = chain(i) else {
                items.push(Item::Old(i));
                i += 1;
                continue;
            };
            let Some((array, key)) = array_from_expect(first) else {
                items.push(Item::Old(i));
                i += 1;
                continue;
            };
            if type_is(ctx, array, Is::Array) != Trinary::Yes || !assertion(first) {
                items.push(Item::Old(i));
                i += 1;
                continue;
            }
            let mut expectations: Vec<Expectation<'_>> = vec![(key, expected_value(first))];
            let mut j = i + 1;
            while let Some(next) = chain(j) {
                let Some((next_array, key)) = array_from_expect(next) else {
                    break;
                };
                if !same_expression(ctx.source, array, next_array) || !assertion(next) {
                    break;
                }
                expectations.push((key, expected_value(next)));
                j += 1;
            }
            if expectations.len() < 2 || expectations.iter().any(|(_, v)| v.is_none()) {
                items.push(Item::Old(i));
                i += 1;
                continue;
            }
            let expect = expect_call(first).unwrap();
            let comments: Vec<Span> = stmts[i..j]
                .iter()
                .flat_map(|s| comment_spans(ctx, node_span(*s).start))
                .collect();
            let expect_span = node_span(expect);
            let arguments = expect.argument_list.arguments.as_slice();
            let single = (arguments.len() == 1).then(|| node_span(&arguments[0]));
            let name = node_span(expect.function);
            items.push(Item::New {
                print: Box::new(move |indent| {
                    let text =
                        |e: &Expression<'_>| kept_text(ctx, node_span(unparenthesize(e)), indent);
                    let delta = indent as isize - super::indent_at(ctx, expect_span.start) as isize;
                    let call = match single {
                        Some(arg) => format!(
                            "{}{}{}",
                            shift(ctx, Span::new(expect_span.start, arg.start), delta),
                            text(array),
                            shift(ctx, Span::new(arg.end, expect_span.end), delta)
                        ),
                        None => format!("{}({})", kept_text(ctx, name, indent), text(array)),
                    };
                    let pairs: Vec<String> = expectations
                        .iter()
                        .filter_map(|(k, v)| Some(format!("{} => {}", text(k), text((*v)?))))
                        .collect();
                    format!("{call}->toMatchArray([{}]);", pairs.join(", "))
                }),
                comments,
            });
            changed = true;
            i = j;
        }
        if list.nop {
            items.push(Item::Old(stmts.len()));
        }
        if !changed {
            return;
        }
        if let Some(edits) = rewrite_list(ctx, node, ancestors, &list, &items) {
            out.push(Report::fixed(
                node_span(stmts[0]),
                "Use `toMatchArray()` instead of consecutive checks of an array's keys.",
                Safety::Safe,
                edits,
            ));
        }
    }
}

fn array_from_expect<'a>(
    call: &'a MethodCall<'a>,
) -> Option<(&'a Expression<'a>, &'a Expression<'a>)> {
    match unparenthesize(expect_argument(call)?) {
        Expression::ArrayAccess(access) => Some((access.array, access.index)),
        _ => None,
    }
}

fn assertion(call: &MethodCall<'_>) -> bool {
    (is_name(&call.method, "toBe") || is_name(&call.method, "toEqual")) && !has_not(call)
}

fn expected_value<'a>(call: &'a MethodCall<'a>) -> Option<&'a Expression<'a>> {
    match call.argument_list.arguments.as_slice() {
        [argument] => Some(argument.value()),
        _ => None,
    }
}
