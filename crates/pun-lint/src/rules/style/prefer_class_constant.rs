use crate::problem::{Edit, Fix, Safety};
use mago_syntax::cst::{Literal, Node, NodeKind};

use crate::rules::support::options::{boolean, configure, part_on};
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

const ZEND_SERVER: [&str; 4] = ["ZendAPI_Queue", "ZendAPI_Job", "java", "JavaException"];

/// PHP-CS-Fixer's `class_keyword`. `namespaced-only` turns it off, leaving the rule's Rector checks
/// ([`super::prefer_class_name_constant`], [`super::prefer_class_keyword`]), which skip global names.
#[derive(Default)]
pub struct PreferClassConstant {
    pub namespaced_only: bool,
}

impl Rule for PreferClassConstant {
    fn id(&self) -> &'static str {
        "lint/style/prefer-class-constant"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        checks(options, "")?;
        configure(options, |key, value| {
            match key {
                "namespaced-only" => self.namespaced_only = boolean(value)?,
                "checks" => {}
                _ => return None,
            }
            Some(())
        })
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Literal])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Literal(Literal::String(string)) = node else {
            return;
        };
        if self.namespaced_only {
            return;
        }
        let span = node_span(string);
        let raw = &ctx.source[span.start..span.end];
        let Some(inner) = raw.get(1..raw.len().saturating_sub(1)) else {
            return;
        };
        let name = inner.trim_start_matches('\\').replace("\\\\", "\\");
        if name.is_empty() {
            return;
        }
        let declared = facts(ctx)
            .codebase
            .get_class_like(name.as_bytes())
            .is_some_and(|c| c.original_name.as_bytes() == name.as_bytes());
        if !declared || ZEND_SERVER.iter().any(|z| z.eq_ignore_ascii_case(&name)) {
            return;
        }
        out.push(Report {
            span,
            message: "Use `::class` instead of a class name in a string.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Unsafe,
                edits: vec![Edit::new(span, format!("\\{name}::class"))],
            }),
        });
    }
}

const CHECKS: [&str; 2] = [
    "StringClassNameToClassConstant",
    "UseClassKeywordForClassNameResolution",
];

/// Pun's `checks`, one for each of Rector's two rules, which pun-config hands to the rule (a PHP-CS-Fixer fixer's)
/// under its values' upstream names, the Rector rules: `string-class-name` is `StringClassNameToClassConstantRector`
/// (Rector's php55 and coding-style sets) and `class-name-resolution` `UseClassKeywordForClassNameResolutionRector`
/// (its coding-style set only). Only `checks` is read: the rule's other options are each part's.
pub(super) fn checks(options: &serde_json::Value, rector: &str) -> Result<bool, OptionsError> {
    let own = (options.get("checks")).map_or(
        serde_json::Value::Null,
        |c| serde_json::json!({ "checks": c }),
    );
    part_on(&own, "checks", &CHECKS, rector)
}
