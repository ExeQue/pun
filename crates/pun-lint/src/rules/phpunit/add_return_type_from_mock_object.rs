use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{Node, NodeKind};

use super::{TEST_CASE, class_is, own_class_like};
use crate::rules::laravel::is_a;
use crate::rules::types::facts::facts;
use crate::rules::types::silent_void::Place;
use crate::rules::types::{
    add_return_type, class_scope, has_only_return_with_expr, returns_scoped, untyped,
};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const MOCK_OBJECT: &str = "PHPUnit\\Framework\\MockObject\\MockObject";

const SCALAR_TYPES: u32 = 70000;

pub struct AddReturnTypeFromMockObject;

impl Rule for AddReturnTypeFromMockObject {
    fn id(&self) -> &'static str {
        "lint/phpunit/add-return-type-from-mock-object"
    }

    fn min_php_version(&self) -> u32 {
        SCALAR_TYPES
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
        let Node::Method(method) = node else { return };
        if method.return_type_hint.is_some() {
            return;
        }
        if !own_class_like(ancestors).is_some_and(|class| class_is(ctx, class, &[TEST_CASE])) {
            return;
        }
        let Some(f) = untyped(node, ancestors, ctx) else {
            return;
        };
        let returns = returns_scoped(f.body);
        let [sole] = returns[..] else {
            return;
        };
        let facts = facts(ctx);
        let scope = class_scope(ancestors);
        let place = Place {
            facts: &facts,
            ancestors,
            scope: scope.as_ref(),
        };
        if !has_only_return_with_expr(&place, f.body, &returns) {
            return;
        }
        let value = sole.value.unwrap();
        let Some(t) = facts.native_type(node_span(value)) else {
            return;
        };
        let mock = match t.types.as_ref() {
            [TAtomic::Object(TObject::Named(named))] => match &named.intersection_types {
                None => is_a(ctx, &named.name.to_string(), &[MOCK_OBJECT]),
                Some(others) => {
                    let is_mock = |name: String| name.eq_ignore_ascii_case(MOCK_OBJECT);
                    others.len() == 1
                        && (is_mock(named.name.to_string())
                            || matches!(&others[0], TAtomic::Object(TObject::Named(n)) if is_mock(n.name.to_string())))
                }
            },
            _ => false,
        };
        if mock {
            out.push(add_return_type(f.span, f.at, &format!("\\{MOCK_OBJECT}")));
        }
    }
}
