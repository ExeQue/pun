use mago_syntax::cst::{
    Access, BinaryOperator, ClassLikeConstantSelector, Expression, Literal, Node, NodeKind,
};
use pun_problem::{Edit, Fix, Safety};

use crate::rules::support::arguments::value;
use crate::rules::support::expressions::text;
use crate::rules::support::names::function_call_matches;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferDynamicClassConstant;

impl Rule for PreferDynamicClassConstant {
    fn id(&self) -> &'static str {
        "lint/php-8.3/prefer-dynamic-class-constant"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80300)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(c) = node else { return };
        let call = c;
        if !function_call_matches(ancestors, call, "constant") {
            return;
        }
        let [arg] = call.argument_list.arguments.as_slice() else {
            return;
        };
        // `Foo::class . '::' . $name` is `(Foo::class . '::') . $name`.
        let Expression::Binary(outer) = unparenthesize(value(arg)) else {
            return;
        };
        let Expression::Binary(inner) = unparenthesize(outer.lhs) else {
            return;
        };
        if !matches!(outer.operator, BinaryOperator::StringConcat(_))
            || !matches!(inner.operator, BinaryOperator::StringConcat(_))
        {
            return;
        }
        let Expression::Access(Access::ClassConstant(class)) = unparenthesize(inner.lhs) else {
            return;
        };
        if !matches!(&class.constant, ClassLikeConstantSelector::Identifier(name) if name.value == b"class")
        {
            return;
        }
        if !matches!(unparenthesize(inner.rhs), Expression::Literal(Literal::String(s)) if s.value == Some(b"::"))
        {
            return;
        }
        let span = node_span(call);
        let code = format!(
            "{}::{{{}}}",
            text(ctx, class.class),
            text(ctx, unparenthesize(outer.rhs))
        );
        out.push(Report {
            span,
            message: "Use a dynamic class constant fetch (`Foo::{$name}`) instead of `constant()`."
                .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, code)],
            }),
        });
    }
}
