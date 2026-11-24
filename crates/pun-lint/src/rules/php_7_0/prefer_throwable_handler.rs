//! Rector 2.8.4's `ExceptionHandlerTypehintRector`.

use mago_syntax::cst::{FunctionLikeParameterList, Hint, Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::rules::support::names::class_name;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferThrowableHandler;

impl Rule for PreferThrowableHandler {
    fn id(&self) -> &'static str {
        "lint/php-7.0/prefer-throwable-handler"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70000)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Function, NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (name, params): (&[u8], &FunctionLikeParameterList) = match node {
            Node::Function(f) => (f.name.value, &f.parameter_list),
            Node::Method(m) => (m.name.value, &m.parameter_list),
            _ => return,
        };
        // exception handle has 1 param exactly
        let [param] = params.parameters.as_slice() else {
            return;
        };
        let Some(hint) = &param.hint else {
            return;
        };
        // handle only Exception typehint
        let hint = match hint {
            Hint::Nullable(nullable) => nullable.hint,
            hint => hint,
        };
        let Hint::Identifier(identifier) = hint else {
            return;
        };
        if class_name(ancestors, identifier) != "Exception" {
            return;
        }
        // is probably handling exceptions
        if !name.to_ascii_lowercase().windows(6).any(|w| w == b"handle") {
            return;
        }
        out.push(Report::fixed(
            node_span(identifier),
            "An exception handler receives every `Throwable` since PHP 7.0; type the parameter `Throwable`.",
            Safety::Unsafe,
            vec![Edit::new(node_span(identifier), "\\Throwable")],
        ));
    }
}
