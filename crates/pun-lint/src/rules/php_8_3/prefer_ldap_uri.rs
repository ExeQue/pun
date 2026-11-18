use mago_span::HasSpan;
use mago_syntax::cst::{Access, BinaryOperator, Call, Expression, Literal, Node, NodeKind};
use pun_problem::{Edit, Fix, Safety};

use crate::rules::support::arguments::{remove_argument, value};
use crate::rules::support::expressions::{binary, operand, text};
use crate::rules::support::names::function_call_matches;
use crate::rules::support::statements::is_dynamic_expr;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferLdapUri;

impl Rule for PreferLdapUri {
    fn id(&self) -> &'static str {
        "lint/php-8.3/prefer-ldap-uri"
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
        if !function_call_matches(ancestors, call, "ldap_connect") {
            return;
        }
        let [host, port] = call.argument_list.arguments.as_slice() else {
            return;
        };
        let (host, port) = (value(host), value(port));
        let Some(code) = uri(ctx, host, port) else {
            return;
        };
        let span = node_span(call);
        out.push(Report {
            span,
            message: "Pass the host and port to `ldap_connect()` as one URI.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![
                    Edit::new(node_span(host), code),
                    remove_argument(&call.argument_list, 1),
                ],
            }),
        });
    }
}

/// The URI argument that replaces the host and port, as Rector's `refactor()` builds it.
fn uri(ctx: &Context<'_>, host: &Expression<'_>, port: &Expression<'_>) -> Option<String> {
    match (unparenthesize(host), unparenthesize(port)) {
        (Expression::Literal(Literal::String(h)), Expression::Literal(Literal::Integer(p))) => {
            // None for a string Pun can't write back (its edits are UTF-8) or an integer without a value.
            let h = std::str::from_utf8(h.value?).ok()?;
            Some(single_quoted(&format!("{h}:{}", p.value?)))
        }
        (h, p) if is_dynamic_expr(h) && is_dynamic_expr(p) => {
            let h_concat = is_concat(h);
            if (h_concat && !is_concat(p)) || !interpolable(h) || !interpolable(p) {
                // Rector interpolates in the other cases too, which only works for a variable or an access on
                // one: `"{$a . $b}"` is a parse error and `"{foo()}"` the text `{foo()}`. Pun concatenates.
                let (_, l, r) = binary(&BinaryOperator::StringConcat(h.span()));
                Some(format!(
                    "{} . ':' . {}",
                    operand(ctx, h, l, Some(l)),
                    operand(ctx, p, r, None)
                ))
            } else {
                Some(format!("\"{{{}}}:{{{}}}\"", text(ctx, h), text(ctx, p)))
            }
        }
        _ => None,
    }
}

fn is_concat(expr: &Expression<'_>) -> bool {
    matches!(expr, Expression::Binary(b) if matches!(b.operator, BinaryOperator::StringConcat(_)))
}

/// Whether `"{expr}"` is PHP: a variable, or a property, array or method access on one.
fn interpolable(expr: &Expression<'_>) -> bool {
    match expr {
        Expression::Variable(_) => true,
        Expression::ArrayAccess(a) => interpolable(a.array),
        Expression::Access(Access::Property(p)) => interpolable(p.object),
        Expression::Access(Access::NullSafeProperty(p)) => interpolable(p.object),
        Expression::Call(Call::Method(m)) => interpolable(m.object),
        Expression::Call(Call::NullSafeMethod(m)) => interpolable(m.object),
        _ => false,
    }
}

/// PhpParser's `pSingleQuotedString`.
fn single_quoted(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::from("'");
    for (i, &c) in chars.iter().enumerate() {
        let escape = c == '\''
            || (c == '\\'
                && (matches!(chars.get(i + 1), None | Some('\'' | '\\'))
                    || (i > 0 && chars[i - 1] == '\\')));
        if escape {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}
