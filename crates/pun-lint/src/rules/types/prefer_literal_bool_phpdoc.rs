use mago_syntax::cst::{Hint, Node, NodeKind};

use super::compare::mapper::flatten;
use super::docblock::docs_report;
use crate::rules::support::phpdoc_writer::DocTarget;
use crate::rules::support::phpdoc_writer::type_node::TypeNode;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferLiteralBoolPhpdoc;

impl Rule for PreferLiteralBoolPhpdoc {
    fn id(&self) -> &'static str {
        "lint/types/prefer-literal-bool-phpdoc"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Function, NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (hint, span) = match node {
            Node::Function(f) => (f.return_type_hint.as_ref(), node_span(&f.name)),
            Node::Method(m) => (m.return_type_hint.as_ref(), node_span(&m.name)),
            _ => return,
        };
        let Some(constant) = hint.and_then(|h| native_constant_bool(&h.hint)) else {
            return;
        };
        let target = DocTarget::of(ctx, node_span(&node).start);
        let mut info = target.info(ctx.source);
        let Some(TypeNode::Union { types, wrapped }) = info.return_tag().map(|t| t.ty.clone())
        else {
            return;
        };
        let is = |t: &TypeNode, name: &str| matches!(t, TypeNode::Identifier(n) if n == name);
        // already holds the constant: replacing `bool` would write it twice
        if types.iter().any(|t| is(t, constant)) || !types.iter().any(|t| is(t, "bool")) {
            return;
        }
        let types = types
            .into_iter()
            .map(|t| match is(&t, "bool") {
                true => TypeNode::ident(constant),
                false => t,
            })
            .collect();
        info.set_return_type(TypeNode::Union { types, wrapped });
        out.extend(docs_report(
            span,
            &format!(
                "Narrow `bool` in the `@return` tag to `{constant}`, as the native type does."
            ),
            ctx.source,
            &[(target, info)],
        ));
    }
}

/// `false` or `true` when the native union return type allows exactly one of them.
fn native_constant_bool(hint: &Hint<'_>) -> Option<&'static str> {
    if !matches!(hint, Hint::Union(_)) {
        return None;
    }
    let mut members = Vec::new();
    flatten(hint, true, &mut members);
    let (mut has_false, mut has_true) = (false, false);
    for member in members {
        match member {
            // ambiguous, both values possible
            Hint::Bool(_) => return None,
            Hint::False(_) => has_false = true,
            Hint::True(_) => has_true = true,
            _ => {}
        }
    }
    match (has_false, has_true) {
        (true, false) => Some("false"),
        (false, true) => Some("true"),
        _ => None,
    }
}
