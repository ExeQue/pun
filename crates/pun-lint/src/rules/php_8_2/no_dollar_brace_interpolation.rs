//! Rector 2.8.4's `VariableInStringInterpolationFixerRector`: `"${var}"` and `"${var['key']}"` become `"{$var}"` and
//! `"{$var['key']}"`, and `"${expr}"` becomes `"{${expr}}"`, in double-quoted strings and heredocs (not backticks).
//!
//! Pun keeps the meaning where Rector does not: after a literal `$` (`"$${v}"`), Rector's `"${$v}"` reads `$v`'s
//! value as a variable name, so Pun escapes that `$` too (`"\${$v}"`, as PHP-CS-Fixer's
//! `simple_to_complex_string_variable` writes it).

use mago_syntax::cst::{
    ArrayAccess, Expression, Identifier, IndirectVariable, Node, NodeKind, StringPart, Variable,
};
use pun_problem::{Edit, Safety, Span};

use crate::rules::support::expressions::text;
use crate::syntax::{node_span, span};
use crate::{Context, Report, Rule};

pub struct NoDollarBraceInterpolation;

impl Rule for NoDollarBraceInterpolation {
    fn id(&self) -> &'static str {
        "lint/php-8.2/no-dollar-brace-interpolation"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80200)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::InterpolatedString, NodeKind::DocumentString])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let parts = match node {
            Node::InterpolatedString(s) => &s.parts,
            Node::DocumentString(s) => &s.parts,
            _ => return,
        };
        let mut edits = Vec::new();
        for part in parts {
            let StringPart::Expression(Expression::Variable(Variable::Indirect(v))) = part else {
                continue;
            };
            edits.extend(escape_dollar_before(ctx.source, v));
            edits.push(rewrite(ctx, v));
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Use `{$var}` instead of the `${var}` interpolation PHP 8.2 deprecates.",
            Safety::Safe,
            edits,
        ));
    }
}

fn rewrite(ctx: &Context<'_>, v: &IndirectVariable<'_>) -> Edit {
    let open = span(v.dollar_left_brace);
    if is_name(v.expression) {
        return Edit::new(open, "{$");
    }
    Edit::new(
        Span::new(open.start, span(v.right_brace).end),
        format!("{{${{{}}}}}", text(ctx, v.expression)),
    )
}

/// `${name}` and `${name[offset]}`, the forms whose name follows `${` as a label.
fn is_name(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::Identifier(Identifier::Local(_)) => true,
        Expression::ArrayAccess(ArrayAccess { array, .. }) => {
            matches!(array, Expression::Identifier(Identifier::Local(_)))
        }
        _ => false,
    }
}

/// An unescaped literal `$` right before `${` would join the `{$` the fix writes into a new `${`.
fn escape_dollar_before(source: &str, v: &IndirectVariable<'_>) -> Option<Edit> {
    let at = span(v.dollar_left_brace).start;
    let before = &source.as_bytes()[..at];
    let (&last, rest) = before.split_last()?;
    let backslashes = rest.iter().rev().take_while(|&&b| b == b'\\').count();
    (last == b'$' && backslashes % 2 == 0).then(|| Edit::new(Span::new(at - 1, at - 1), "\\"))
}
