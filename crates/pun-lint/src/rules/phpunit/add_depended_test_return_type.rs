use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{ClassLikeMember, Method, MethodBody, Node, NodeKind};

use super::{class_body, is_in_test_class, is_public};
use crate::rules::support::class_members::member_from;
use crate::rules::support::docblocks::doc_comment;
use crate::rules::support::names::has_attribute;
use crate::rules::types::declare::{return_declaration, union_order};
use crate::rules::types::facts::facts;
use crate::rules::types::silent_void::Place;
use crate::rules::types::{class_scope, has_only_return_with_expr, returns_scoped};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct AddDependedTestReturnType;

impl Rule for AddDependedTestReturnType {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-depended-test-return-type"
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
        let (members, left, _) = class_body(node).unwrap();
        let mut scope = ancestors.to_vec();
        scope.push(node);
        if !is_in_test_class(ctx, &scope, None) {
            return;
        }
        let facts = facts(ctx);
        let class = class_scope(&scope);
        let place = Place {
            facts: &facts,
            ancestors: &scope,
            scope: class.as_ref(),
        };
        for (at, member) in members.iter().enumerate() {
            let ClassLikeMember::Method(method) = member else {
                continue;
            };
            let from = member_from(members, left, at);
            if !is_public(method)
                || !is_test_method(ctx.source, &scope, method, from)
                || method.return_type_hint.is_some()
            {
                continue;
            }
            let MethodBody::Concrete(body) = &method.body else {
                return;
            };
            let returns = returns_scoped(body);
            if !has_only_return_with_expr(&place, body, &returns) {
                return;
            }
            let [sole] = returns[..] else {
                continue;
            };
            let value = sole.value.unwrap();
            let Some(t) = facts
                .native_type(node_span(value))
                .filter(|t| !t.types.is_empty())
            else {
                continue;
            };
            let Some(declared) = return_declaration(
                union_order(t.types.as_ref()),
                &facts.codebase,
                ctx.php_version.id(),
                &[],
                true,
            ) else {
                continue;
            };
            let end = method.parameter_list.right_parenthesis.end.offset as usize;
            out.push(Report::fixed(
                node_span(&method.name),
                format!("Add the `{declared}` return type: a test that depends on this one gets its value."),
                Safety::Unsafe,
                vec![Edit::new(Span::new(end, end), format!(": {declared}"))],
            ));
        }
    }
}

fn is_test_method(
    source: &str,
    ancestors: &[Node<'_, '_>],
    method: &Method<'_>,
    from: usize,
) -> bool {
    if method.name.value.starts_with(b"test")
        || has_attribute(
            ancestors,
            &method.attribute_lists,
            "PHPUnit\\Framework\\Attributes\\Test",
        )
    {
        return true;
    }
    let Some(doc) = doc_comment(source, from, node_span(method).start) else {
        return false;
    };
    let doc = &source[doc.start..doc.end];
    doc.match_indices("@test").any(|(i, m)| {
        doc[i + m.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '-' || c == '\\'))
    })
}
