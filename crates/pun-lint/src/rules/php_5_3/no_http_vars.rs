//! Rector 2.8.4's `ReplaceHttpServerVarsByServerRector`.

use mago_syntax::cst::{Node, NodeKind};

use crate::problem::{Edit, Safety};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

/// `VARIABLE_RENAME_MAP`; variable names are case-sensitive.
const VARIABLE_RENAME_MAP: &[(&[u8], &str)] = &[
    (b"$HTTP_SERVER_VARS", "$_SERVER"),
    (b"$HTTP_GET_VARS", "$_GET"),
    (b"$HTTP_POST_VARS", "$_POST"),
    (b"$HTTP_POST_FILES", "$_FILES"),
    (b"$HTTP_SESSION_VARS", "$_SESSION"),
    (b"$HTTP_ENV_VARS", "$_ENV"),
    (b"$HTTP_COOKIE_VARS", "$_COOKIE"),
];

pub struct NoHttpVars;

impl Rule for NoHttpVars {
    fn id(&self) -> &'static str {
        "lint/php-5.3/no-http-vars"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(50300)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::DirectVariable])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::DirectVariable(variable) = node else {
            return;
        };
        // A property's name is a `VarLikeIdentifier` in php-parser, not a `Variable`.
        if matches!(
            ancestors
                .iter()
                .rev()
                .find(|a| !matches!(a, Node::Variable(_))),
            Some(
                Node::PropertyConcreteItem(_)
                    | Node::PropertyAbstractItem(_)
                    | Node::StaticPropertyAccess(_)
            )
        ) {
            return;
        }
        let Some((_, new)) = VARIABLE_RENAME_MAP
            .iter()
            .find(|(old, _)| *old == variable.name)
        else {
            return;
        };
        // Rector renames a parameter too, which PHP rejects, and then leaves the whole file alone.
        if ancestors.iter().any(|a| {
            let parameters = match a {
                Node::Function(f) => &f.parameter_list,
                Node::Method(m) => &m.parameter_list,
                Node::Closure(c) => &c.parameter_list,
                Node::ArrowFunction(f) => &f.parameter_list,
                _ => return false,
            };
            parameters
                .parameters
                .iter()
                .any(|p| p.variable.name == variable.name)
        }) {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            format!("Use `{new}`: PHP 5.4 removed the long `$HTTP_*_VARS` arrays."),
            Safety::Safe,
            vec![Edit::new(node_span(&node), *new)],
        ));
    }
}
