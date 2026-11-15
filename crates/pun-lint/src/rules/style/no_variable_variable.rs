use mago_syntax::cst::{Expression, Node, NodeKind, Variable};

use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoVariableVariable;

impl Rule for NoVariableVariable {
    fn id(&self) -> &'static str {
        "lint/style/no-variable-variable"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Variable])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Variable(v) = node else { return };
        match v {
            Variable::Direct(_) => return,
            Variable::Indirect(indirect) => match indirect.expression {
                Expression::Identifier(_) => return,
                Expression::ArrayAccess(access)
                    if matches!(access.array, Expression::Identifier(_)) =>
                {
                    return;
                }
                _ => {}
            },
            Variable::Nested(_) => {}
        }
        out.push(Report {
            span: node_span(v),
            message: "Variable variables are not allowed.".to_string(),
            help: None,
            fix: None,
        });
    }
}
