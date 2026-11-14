use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{ClassLikeMember, Expression, MethodBody, Node, NodeKind};

use super::add_allow_mock_objects_attribute::{ATTRIBUTES, add_attribute, should_add};
use super::annotations::named_tags;
use super::no_bare_mock_assign::scoped;
use super::{AnyCall, class_body, is_public};
use crate::rules::laravel::is_a;
use crate::rules::lint_b_doc::Doc;
use crate::rules::rector_batch5::has_attribute;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct AddDataProviderAllowMockObjects;

impl Rule for AddDataProviderAllowMockObjects {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-data-provider-allow-mock-objects"
    }

    fn min_php_version(&self) -> u32 {
        ATTRIBUTES
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
        if !should_add(ctx, &scope, node) {
            return;
        }
        let found = members.iter().any(|m| {
            let ClassLikeMember::Method(method) = m else {
                return false;
            };
            let is_test = is_public(method)
                && (method.name.value.starts_with(b"test")
                    || has_attribute(
                        &scope,
                        &method.attribute_lists,
                        "PHPUnit\\Framework\\Attributes\\Test",
                    )
                    || Doc::of(ctx, &scope, node_span(method).start).is_some_and(|doc| {
                        named_tags(ctx, &scope, &doc, "test").is_some_and(|t| !t.is_empty())
                    }));
            if !is_test
                || !has_attribute(
                    &scope,
                    &method.attribute_lists,
                    "PHPUnit\\Framework\\Attributes\\DataProvider",
                )
            {
                return false;
            }
            let MethodBody::Concrete(body) = &method.body else {
                return false;
            };
            let roots: Vec<Node<'_, '_>> = body.statements.iter().map(Node::Statement).collect();
            let mut without_expects = false;
            scoped(&roots, &mut |n| {
                if let Node::MethodCall(call) = n
                    && AnyCall::of(n).is_some_and(|c| c.is_name(&["method"]))
                    && !matches!(
                        unparenthesize(call.object),
                        Expression::Call(mago_syntax::cst::Call::Method(_))
                    )
                    && !is_stub_only(ctx, call.object)
                {
                    without_expects = true;
                }
            });
            without_expects
        });
        if found {
            add_attribute(ctx, ancestors, node, out);
        }
    }
}

const STUB: &str = "PHPUnit\\Framework\\MockObject\\Stub";
const MOCK_OBJECT: &str = "PHPUnit\\Framework\\MockObject\\MockObject";

fn is_stub_only(ctx: &Context<'_>, expr: &Expression<'_>) -> bool {
    let facts = facts(ctx);
    let Some(t) = facts.full_type(node_span(unparenthesize(expr))) else {
        return false;
    };
    let surely = |class: &str| {
        !t.types.is_empty()
            && t.types.iter().all(|atomic| match atomic {
                TAtomic::Object(TObject::Named(named)) => {
                    is_a(ctx, &named.name.to_string(), &[class])
                        || (named.intersection_types.iter().flat_map(|ts| ts.iter())).any(|part| {
                            match part {
                                TAtomic::Object(TObject::Named(p)) => {
                                    is_a(ctx, &p.name.to_string(), &[class])
                                }
                                _ => false,
                            }
                        })
                }
                _ => false,
            })
    };
    surely(STUB) && !surely(MOCK_OBJECT)
}
