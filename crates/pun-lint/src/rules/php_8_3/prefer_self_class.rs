use mago_syntax::cst::{Node, NodeKind};
use pun_problem::{Edit, Fix, Safety};

use crate::rules::support::names::function_call_matches_any;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferSelfClass;

impl Rule for PreferSelfClass {
    fn id(&self) -> &'static str {
        "lint/php-8.3/prefer-self-class"
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
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(c) = node else { return };
        let call = c;
        if !call.argument_list.arguments.is_empty() {
            return;
        }
        let (function, target) =
            match function_call_matches_any(ancestors, call, &["get_class", "get_parent_class"]) {
                Some("get_class") => ("get_class", "self"),
                Some(_) => ("get_parent_class", "parent"),
                None => return,
            };
        let span = node_span(call);
        out.push(Report {
            span,
            message: format!("Use `{target}::class` instead of `{function}()` without arguments."),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, format!("{target}::class"))],
            }),
        });
    }
}
