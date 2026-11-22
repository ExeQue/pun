use crate::problem::{Edit, Fix, Safety, Span};
use mago_span::HasSpan;
use mago_syntax::cst::{Method, Node, NodeKind};

use crate::rules::support::names::resolve_class;
use crate::syntax::span;
use crate::{Context, Report, Rule};

pub struct PreferTestAttribute;

impl Rule for PreferTestAttribute {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-test-attribute"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Method(method) = node else {
            return;
        };
        let name_span = method.name.span();
        let name = &ctx.source[name_span.start.offset as usize..name_span.end.offset as usize];
        if !name.starts_with("test") && !name.starts_with("Test") {
            return;
        }
        if name[4..]
            .bytes()
            .next()
            .is_some_and(|first| first != b'_' && !first.is_ascii_uppercase())
        {
            return;
        }
        if has_test_attribute(ancestors, method) {
            return;
        }
        let new_name = new_name(name);
        let start = method.span().start.offset as usize;
        let line_start = ctx.source[..start].rfind('\n').map_or(0, |pos| pos + 1);
        let indent: String = ctx.source[line_start..start]
            .chars()
            .take_while(|c| *c == ' ' || *c == '\t')
            .collect();
        out.push(Report {
            span: span(method.name.span()),
            message: format!(
                "Use `#[Test]` attribute instead of `test` prefix on method `{name}`."
            ),
            help: Some(format!(
                "Rename to `{new_name}` and add `#[Test]` attribute."
            )),
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![
                    Edit::new(span(method.name.span()), new_name),
                    Edit::new(
                        Span::new(start, start),
                        format!("#[\\PHPUnit\\Framework\\Attributes\\Test]\n{indent}"),
                    ),
                ],
            }),
        });
    }
}

fn has_test_attribute(ancestors: &[Node<'_, '_>], method: &Method<'_>) -> bool {
    method.attribute_lists.iter().any(|list| {
        list.attributes.iter().any(|attribute| {
            resolve_class(ancestors, &attribute.name)
                .eq_ignore_ascii_case(b"PHPUnit\\Framework\\Attributes\\Test")
        })
    })
}

fn new_name(name: &str) -> String {
    let after_test = &name[4..];
    if after_test.is_empty() || after_test == "_" {
        return name.to_string();
    }
    if let Some(rest) = after_test.strip_prefix('_') {
        return rest.to_string();
    }
    if name.starts_with("test") {
        return after_test[..1].to_ascii_lowercase() + &after_test[1..];
    }
    after_test.to_string()
}
