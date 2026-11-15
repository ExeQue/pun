use mago_syntax::cst::{
    ArrayElement, Call, Expression, Hint, Literal, Modifier, Node, NodeKind, Variable,
};
use pun_problem::{Edit, Fix, Safety};

use crate::rules::support::arguments::{unpacked, value};
use crate::rules::support::expressions::named;
use crate::rules::support::values::{Known, known};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoCallUserFuncMethod;

impl Rule for NoCallUserFuncMethod {
    fn id(&self) -> &'static str {
        "lint/functions/no-call-user-func-method"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Call])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Call(Call::Function(call)) = node else {
            return;
        };
        if !named(call, "call_user_func") {
            return;
        }
        let args: Vec<_> = call.argument_list.arguments.iter().collect();
        let Some(first) = args.first().filter(|a| !unpacked(a)) else {
            return;
        };
        let elements: Vec<_> = match value(first) {
            Expression::Array(array) => array.elements.iter().collect(),
            Expression::LegacyArray(array) => array.elements.iter().collect(),
            _ => return,
        };
        let [object, method] = elements[..] else {
            return;
        };
        let (Some(object), Some(method)) = (item(object), item(method)) else {
            return;
        };
        let Expression::Literal(Literal::String(method)) = method else {
            return;
        };
        let Some(method) = method.value.and_then(|m| std::str::from_utf8(m).ok()) else {
            return;
        };
        let object = unparenthesize(object);
        let at = node_span(call).start;
        if !is_identifier(method) || !has_class(ancestors, object, at) {
            return;
        }
        let source = ctx.source;
        let target = node_span(object);
        let rest: Vec<&str> = args[1..]
            .iter()
            .map(|a| {
                let s = node_span(*a);
                &source[s.start..s.end]
            })
            .collect();
        let code = format!(
            "{}->{method}({})",
            &source[target.start..target.end],
            rest.join(", ")
        );
        out.push(Report {
            span: node_span(call),
            message: "Call the method directly instead of through `call_user_func()`.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Unsafe,
                edits: vec![Edit::new(node_span(call), code)],
            }),
        });
    }
}

fn item<'a>(element: &'a ArrayElement<'a>) -> Option<&'a Expression<'a>> {
    match element {
        ArrayElement::Value(e) => Some(e.value),
        ArrayElement::KeyValue(e) => Some(e.value),
        _ => None,
    }
}

fn has_class(ancestors: &[Node<'_, '_>], expr: &Expression<'_>, at: usize) -> bool {
    let Expression::Variable(Variable::Direct(var)) = expr else {
        return false;
    };
    if var.name == b"$this" {
        return this_available(ancestors);
    }
    matches!(
        known(ancestors, expr, at),
        Some(Known::Hint(Hint::Identifier(_)) | Known::Value(Expression::Instantiation(_)))
    )
}

fn this_available(ancestors: &[Node<'_, '_>]) -> bool {
    for node in ancestors.iter().rev() {
        match node {
            Node::Method(m) => {
                return !m.modifiers.iter().any(|m| matches!(m, Modifier::Static(_)));
            }
            Node::Closure(c) if c.r#static.is_some() => return false,
            Node::ArrowFunction(f) if f.r#static.is_some() => return false,
            Node::Function(_) => return false,
            _ => {}
        }
    }
    false
}

/// A name `$this->name` can take: a letter, `_` or a byte from 0x80, then those and digits.
pub(super) fn is_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b == b'_' || b.is_ascii_alphabetic() || b >= 0x80)
        && bytes.all(|b| b == b'_' || b.is_ascii_alphanumeric() || b >= 0x80)
}
