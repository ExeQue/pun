//! Rector 2.8.4's `StringsAssertNakedRector`.

use mago_syntax::cst::{Expression, Literal, Node, NodeKind, Statement};

use crate::problem::{Edit, Safety};
use crate::rules::support::arguments::value;
use crate::rules::support::expressions::named;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct NoStringAssert;

impl Rule for NoStringAssert {
    fn id(&self) -> &'static str {
        "lint/php-7.2/no-string-assert"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70200)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        if !named(call, "assert") {
            return;
        }
        let Some(first) = call.argument_list.arguments.first() else {
            return;
        };
        let Expression::Literal(Literal::String(string)) = value(first) else {
            return;
        };
        let Some(code) = string.value else {
            return;
        };
        let code = String::from_utf8_lossy(code);
        let Some(expr) = parse_expression(&code) else {
            return;
        };
        out.push(Report::fixed(
            node_span(string),
            "A string assertion is removed in PHP 8.0; pass the expression to `assert()` directly.",
            Safety::Safe,
            vec![Edit::new(node_span(string), expr)],
        ));
    }
}

/// The expression `<?php {code};` holds as its first statement, when it parses.
fn parse_expression(code: &str) -> Option<String> {
    let source = format!("<?php {code};");
    let arena = mago_allocator::LocalArena::new();
    let file = mago_database::file::File::new(
        std::borrow::Cow::Borrowed(b"assert.php".as_slice()),
        mago_database::file::FileType::Host,
        None,
        std::borrow::Cow::Owned(source.clone().into_bytes()),
    );
    let program = mago_syntax::parser::parse_file(&arena, &file);
    if !program.errors.is_empty() {
        return None;
    }
    let first = program
        .statements
        .iter()
        .find(|s| !matches!(s, Statement::OpeningTag(_)))?;
    let Statement::Expression(e) = first else {
        return None;
    };
    let span = node_span(e.expression);
    Some(source[span.start..span.end].to_string())
}
