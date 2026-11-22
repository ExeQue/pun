use crate::problem::{Edit, Fix, Safety};
use mago_syntax::cst::{Argument, Expression, Literal, Node, NodeKind};

use crate::rules::support::arguments::value;
use crate::rules::support::expressions::{Prec, args, in_parent, named};
use crate::rules::support::values::int_version;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferPhpVersionId;

impl Rule for PreferPhpVersionId {
    fn id(&self) -> &'static str {
        "lint/style/prefer-php-version-id"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(c) = node else { return };
        let call = c;
        let args = args(call);
        if !named(call, "version_compare")
            || args.len() != 3
            || args.iter().any(|a| matches!(a, Argument::Named(_)))
        {
            return;
        }
        let (left, right) = (value(args[0]), value(args[1]));
        if !is_php_version(left) && !is_php_version(right) {
            return;
        }
        let (Some(left), Some(right)) = (operand(left), operand(right)) else {
            return;
        };
        let Expression::Literal(Literal::String(op)) = value(args[2]) else {
            return;
        };
        let (op, p) = match op.value.unwrap_or_default() {
            b"=" | b"==" | b"eq" => ("===", 80),
            b"!=" | b"<>" | b"ne" => ("!==", 80),
            b">" | b"gt" => (">", 70),
            b"<" | b"lt" => ("<", 70),
            b">=" | b"ge" => (">=", 70),
            b"<=" | b"le" => ("<=", 70),
            _ => return,
        };
        let span = node_span(call);
        let code = in_parent(
            span,
            ancestors,
            format!("{left} {op} {right}"),
            Prec::Infix(p, p, p),
        );
        out.push(Report {
            span,
            message: "Compare `PHP_VERSION_ID` instead of calling `version_compare()`.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, code)],
            }),
        });
    }
}

fn is_php_version(expr: &Expression<'_>) -> bool {
    matches!(expr, Expression::ConstantAccess(c) if c.name.value().strip_prefix(b"\\").unwrap_or(c.name.value()) == b"PHP_VERSION")
}

fn operand(expr: &Expression<'_>) -> Option<String> {
    if is_php_version(expr) {
        return Some("PHP_VERSION_ID".to_string());
    }
    let Expression::Literal(Literal::String(s)) = expr else {
        return None;
    };
    Some(int_version(&String::from_utf8_lossy(s.value?)).to_string())
}
