//! Rector 2.8.4's `WrapVariableVariableNameInCurlyBracesRector`. The `global $$foo->bar` case it also handles does
//! not parse since PHP 7.0, so only a variable variable named by a variable (`$$foo`) is left.

use mago_syntax::cst::{NestedVariable, Node, NodeKind, Variable};

use crate::problem::{Edit, Safety};
use crate::rules::support::expressions::text;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferBracedVariableVariable;

impl Rule for PreferBracedVariableVariable {
    fn id(&self) -> &'static str {
        "lint/php-7.0/prefer-braced-variable-variable"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70000)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::NestedVariable])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::NestedVariable(variable) = node else {
            return;
        };
        // The outermost `$$...` is rewritten with the ones it names.
        if ancestors
            .iter()
            .rev()
            .take_while(|a| matches!(a, Node::Variable(_) | Node::NestedVariable(_)))
            .any(|a| matches!(a, Node::NestedVariable(_)))
        {
            return;
        }
        out.push(Report::fixed(
            node_span(variable),
            "Wrap the name of a variable variable in braces: `${$name}`.",
            Safety::Safe,
            vec![Edit::new(node_span(variable), braced(ctx, variable))],
        ));
    }
}

fn braced(ctx: &Context<'_>, variable: &NestedVariable<'_>) -> String {
    let inner = match variable.variable {
        Variable::Nested(nested) => braced(ctx, nested),
        inner => text(ctx, inner).to_string(),
    };
    format!("${{{inner}}}")
}
