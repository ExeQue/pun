use mago_syntax::cst::{MethodBody, Node, NodeKind};
use pun_problem::{Edit, Fix, Safety, Span};
use std::fmt::Write as _;

use crate::rules::support::class_members::modifiers_text;
use crate::rules::support::serialize_methods::{
    append_parameter, class_method, rename, set_return_type,
};
use crate::rules::support::statement_edits::{block_list, line_indent};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferUnserializeMethod;

impl Rule for PreferUnserializeMethod {
    fn id(&self) -> &'static str {
        "lint/classes/prefer-unserialize-method"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80500)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if class_method(node, "__unserialize").is_some() {
            return;
        }
        let Some(method) = class_method(node, "__wakeup") else {
            return;
        };
        let MethodBody::Concrete(block) = &method.body else {
            return;
        };
        let source = ctx.source;
        let list = block_list(source, block);
        let edits = if list.len() == 1 {
            let old = list
                .nop_span(source)
                .filter(|_| list.stmts.is_empty())
                .unwrap_or_else(|| list.with_comments(ctx, 0));
            vec![
                rename(method, "__unserialize"),
                append_parameter(source, method, "array $data"),
                set_return_type(method, "void"),
                Edit::new(old, foreach(line_indent(source, old.start))),
            ]
        } else {
            let span = node_span(method);
            let indent = line_indent(source, span.start);
            let mut code = String::new();
            for list in &method.attribute_lists {
                let s = node_span(list);
                code.push_str(&source[s.start..s.end]);
                code.push('\n');
                code.push_str(indent);
            }
            code.push_str(&modifiers_text(&method.modifiers));
            code.push_str("function ");
            if method.ampersand.is_some() {
                code.push('&');
            }
            code.push_str("__unserialize(");
            for parameter in &method.parameter_list.parameters {
                let s = node_span(parameter);
                code.push_str(&source[s.start..s.end]);
                code.push_str(", ");
            }
            let body = foreach(&format!("{indent}    "));
            let _ = write!(
                code,
                "array $data): void\n{indent}{{\n{indent}    {body}\n{indent}}}"
            );
            vec![Edit::new(Span::new(span.start, span.end), code)]
        };
        out.push(Report {
            span: node_span(&method.name),
            message: "Replace the deprecated `__wakeup()` with `__unserialize()`.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Unsafe,
                edits,
            }),
        });
    }
}

fn foreach(indent: &str) -> String {
    format!(
        "foreach ($data as $property => $value) {{\n\
         {indent}    if (property_exists($this, $property)) {{\n\
         {indent}        $this->{{$property}} = $value;\n\
         {indent}    }}\n\
         {indent}}}"
    )
}
