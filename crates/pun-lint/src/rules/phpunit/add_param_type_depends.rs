use crate::problem::Safety;
use mago_syntax::cst::{ClassLikeMember, Expression, Literal, Method, Node, NodeKind};

use super::add_mock_object_type::type_edit;
use super::annotations::{TagValue, named_tags};
use super::{class_body, is_in_test_class, is_public, resolve, string_value, text};
use crate::rules::support::docblocks::Doc;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct AddParamTypeDepends;

impl Rule for AddParamTypeDepends {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-param-type-depends"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (members, _, _) = class_body(node).unwrap();
        let mut scope = ancestors.to_vec();
        scope.push(node);
        if !is_in_test_class(ctx, &scope, None) {
            return;
        }
        let methods: Vec<&Method<'_>> = members
            .iter()
            .filter_map(|m| match m {
                ClassLikeMember::Method(m) => Some(m),
                _ => None,
            })
            .collect();
        let mut edits = Vec::new();
        for method in &methods {
            if !is_public(method) || method.parameter_list.parameters.len() != 1 {
                continue;
            }
            let param = method.parameter_list.parameters.first().unwrap();
            if param.hint.is_some() {
                continue;
            }
            let Some(name) = depends_method(ctx, &scope, method) else {
                continue;
            };
            if name.is_empty() {
                continue;
            }
            let Some(depended) = methods
                .iter()
                .find(|m| m.name.value.eq_ignore_ascii_case(name.as_bytes()))
            else {
                continue;
            };
            let Some(return_type) = &depended.return_type_hint else {
                continue;
            };
            edits.push(type_edit(param, text(ctx, &return_type.hint)));
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Type the parameter with the return type of the test it depends on.",
            Safety::Safe,
            edits,
        ));
    }
}

fn depends_method(
    ctx: &Context<'_>,
    scope: &[Node<'_, '_>],
    method: &Method<'_>,
) -> Option<String> {
    let attribute = method
        .attribute_lists
        .iter()
        .flat_map(|l| l.attributes.iter())
        .find(|a| {
            resolve(scope, &a.name).eq_ignore_ascii_case("PHPUnit\\Framework\\Attributes\\Depends")
        });
    if let Some(attribute) = attribute {
        let first = attribute.argument_list.as_ref()?.arguments.first()?;
        let value = first.value()?;
        if let Expression::Literal(Literal::String(_)) = value {
            return Some(string_value(value)?.trim_matches(['(', ')']).to_string());
        }
    }
    let doc = Doc::of(ctx, scope, node_span(method).start)?;
    match named_tags(ctx, scope, &doc, "depends")?
        .into_iter()
        .next()?
        .1
    {
        TagValue::Generic(value) => Some(value.trim_matches(['(', ')']).to_string()),
        _ => None,
    }
}
