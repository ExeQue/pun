//! Rector 2.8.4's `ClassOnThisVariableObjectRector`.

use mago_syntax::cst::{ClassLikeConstantSelector, Expression, Modifier, Node, NodeKind, Variable};

use crate::problem::{Edit, Safety};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferStaticClass;

impl Rule for PreferStaticClass {
    fn id(&self) -> &'static str {
        "lint/php-8.0/prefer-static-class"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80000)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ClassConstantAccess])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::ClassConstantAccess(fetch) = node else {
            return;
        };
        // `shouldSkip`.
        let Expression::Variable(Variable::Direct(variable)) = fetch.class else {
            return;
        };
        if variable.name != b"$this" {
            return;
        }
        let ClassLikeConstantSelector::Identifier(name) = &fetch.constant else {
            return;
        };
        if !name.value.eq_ignore_ascii_case(b"class") {
            return;
        }
        // Rector refactors a `Class_` node and every fetch below it, so the outermost class decides.
        let class_name = match ancestors
            .iter()
            .find(|a| matches!(a, Node::Class(_) | Node::AnonymousClass(_)))
        {
            Some(Node::Class(class))
                if class
                    .modifiers
                    .iter()
                    .any(|m| matches!(m, Modifier::Final(_))) =>
            {
                "self"
            }
            Some(_) => "static",
            None => return,
        };
        out.push(Report::fixed(
            node_span(&node),
            format!("Use `{class_name}::class` instead of `$this::class`."),
            Safety::Safe,
            vec![Edit::new(node_span(fetch.class), class_name)],
        ));
    }
}
