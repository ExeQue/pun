//! Rector 2.8.4's `SetCookieRector`.

use mago_syntax::cst::{Expression, Node, NodeKind};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::arguments::{unpacked, value};
use crate::rules::support::expressions::{named, text};
use crate::rules::types::compare::mapper::expr_type;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

/// `KNOWN_OPTIONS`, from the third argument on.
const KNOWN_OPTIONS: &[&str] = &["expires", "path", "domain", "secure", "httponly"];

pub struct PreferSetcookieOptions;

impl Rule for PreferSetcookieOptions {
    fn id(&self) -> &'static str {
        "lint/php-7.3/prefer-setcookie-options"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70300)
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
        let Node::FunctionCall(call) = node else {
            return;
        };
        // `shouldSkip`.
        if !named(call, "setcookie") && !named(call, "setrawcookie") {
            return;
        }
        let args = call.argument_list.arguments.as_slice();
        if args.len() <= 2 || args.len() > 2 + KNOWN_OPTIONS.len() || args.iter().any(unpacked) {
            return;
        }
        let third = value(&args[2]);
        if matches!(third, Expression::Array(_) | Expression::LegacyArray(_)) {
            return;
        }
        if args.len() == 3
            && !expr_type(&facts(ctx), ancestors, third, true)
                .is_integer()
                .yes()
        {
            return;
        }
        // `composeNewArgs`.
        let items: Vec<String> = args[2..]
            .iter()
            .zip(KNOWN_OPTIONS)
            .map(|(arg, key)| format!("'{key}' => {}", text(ctx, value(arg))))
            .collect();
        let span = Span::new(
            node_span(&args[2]).start,
            node_span(&args[args.len() - 1]).end,
        );
        out.push(Report::fixed(
            node_span(call),
            "Pass the cookie options as an array, as PHP 7.3 allows.",
            Safety::Safe,
            vec![Edit::new(span, format!("[{}]", items.join(", ")))],
        ));
    }
}
