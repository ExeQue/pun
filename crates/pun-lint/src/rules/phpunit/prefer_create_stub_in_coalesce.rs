use crate::problem::{Edit, Safety};
use mago_syntax::cst::{
    Argument, ArgumentList, ArrayElement, BinaryOperator, Call, Expression, Node, NodeKind,
    PartialArgument,
};

use super::{AnyCall, is_in_test_class};
use crate::syntax::unparenthesize;
use crate::{Context, Report, Rule};

pub struct PreferCreateStubInCoalesce;

impl Rule for PreferCreateStubInCoalesce {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-create-stub-in-coalesce"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::MethodCall,
            NodeKind::StaticMethodCall,
            NodeKind::Instantiation,
            NodeKind::AnonymousClass,
            NodeKind::ArrayElement,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let values: Vec<&Expression<'_>> = match node {
            Node::ArrayElement(ArrayElement::KeyValue(kv)) => vec![kv.value],
            Node::ArrayElement(ArrayElement::Value(v)) => vec![v.value],
            Node::ArrayElement(ArrayElement::Variadic(v)) => vec![v.value],
            Node::MethodCall(_) | Node::StaticMethodCall(_) => AnyCall::of(node)
                .map_or_default(|c| c.args().into_iter().map(Argument::value).collect()),
            Node::Instantiation(new) => args(new.argument_list.as_ref()),
            Node::AnonymousClass(new) => new
                .argument_list
                .iter()
                .flat_map(|a| a.arguments.iter())
                .filter_map(PartialArgument::value)
                .collect(),
            _ => Vec::new(),
        };
        let names: Vec<_> = values
            .into_iter()
            .filter_map(|value| {
                let Expression::Binary(binary) = unparenthesize(value) else {
                    return None;
                };
                if !matches!(binary.operator, BinaryOperator::NullCoalesce(_)) {
                    return None;
                }
                let Expression::Call(Call::Method(right)) = unparenthesize(binary.rhs) else {
                    return None;
                };
                AnyCall::of(Node::MethodCall(right)).filter(|c| c.is_name(&["createMock"]))
            })
            .collect();
        if names.is_empty() || !is_in_test_class(ctx, ancestors, None) {
            return;
        }
        for call in names {
            out.push(Report::fixed(
                call.name_span(),
                "Use `createStub()`: nothing expects calls on this mock.",
                Safety::Safe,
                vec![Edit::new(call.name_span(), "createStub")],
            ));
        }
    }
}

fn args<'a>(list: Option<&'a ArgumentList<'a>>) -> Vec<&'a Expression<'a>> {
    list.iter()
        .flat_map(|l| l.arguments.iter())
        .map(Argument::value)
        .collect()
}
