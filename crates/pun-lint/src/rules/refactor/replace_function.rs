use mago_syntax::cst::{Expression, FunctionCall, Node, NodeKind};
use pun_problem::{Edit, Safety, Span};

use super::{
    assert_class, assert_function, assert_method, function_name, is_name, items, object, string,
    string_map,
};
use crate::rules::phpunit::maybe_multiline;
use crate::rules::support::statement_edits::line_indent;
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

fn printed_args(ctx: &Context<'_>, call: &FunctionCall<'_>) -> String {
    let list = &call.argument_list;
    let spans: Vec<Span> = list.arguments.iter().map(node_span).collect();
    let indent = line_indent(ctx.source, node_span(call).start);
    maybe_multiline(
        ctx,
        list.left_parenthesis.end.offset as usize,
        &spans,
        indent,
    )
}

fn resolved<'ast, 'arena>(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    node: Node<'ast, 'arena>,
) -> Option<(&'ast FunctionCall<'arena>, String)> {
    let Node::FunctionCall(call) = node else {
        return None;
    };
    let Expression::Identifier(name) = call.function else {
        return None;
    };
    Some((call, function_name(ctx, ancestors, name)))
}

fn dereferenced(span: Span, ancestors: &[Node<'_, '_>], code: String) -> String {
    let parent = ancestors.iter().rev().find(|a| node_span(*a) != span);
    let lhs = parent.is_some_and(|p| {
        node_span(p).start == span.start
            && matches!(
                p,
                Node::ArrayAccess(_)
                    | Node::PropertyAccess(_)
                    | Node::NullSafePropertyAccess(_)
                    | Node::MethodCall(_)
                    | Node::NullSafeMethodCall(_)
                    | Node::StaticMethodCall(_)
                    | Node::StaticPropertyAccess(_)
                    | Node::ClassConstantAccess(_)
                    | Node::FunctionCall(_)
            )
    });
    if lhs { format!("({code})") } else { code }
}

#[derive(Default)]
pub struct ReplaceFunctionWithConstant {
    pub replacements: Vec<(String, String)>,
}

impl Rule for ReplaceFunctionWithConstant {
    fn id(&self) -> &'static str {
        "lint/refactor/replace-function-with-constant"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.replacements = string_map(options)?;
        Ok(())
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
        if self.replacements.is_empty() {
            return;
        }
        let Some((_, name)) = resolved(ctx, ancestors, node) else {
            return;
        };
        let Some((_, constant)) = self.replacements.iter().find(|(f, _)| *f == name) else {
            return;
        };
        out.push(Report::fixed(
            node_span(&node),
            format!("Replace the call to `{name}()` with the constant `{constant}`."),
            Safety::Unsafe,
            vec![Edit::new(node_span(&node), constant.clone())],
        ));
    }
}

#[derive(Default)]
pub struct ReplaceFunctionWithNew {
    pub replacements: Vec<(String, String)>,
}

impl Rule for ReplaceFunctionWithNew {
    fn id(&self) -> &'static str {
        "lint/refactor/replace-function-with-new"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.replacements = string_map(options)?;
        Ok(())
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
        if self.replacements.is_empty() {
            return;
        }
        let Some((call, name)) = resolved(ctx, ancestors, node) else {
            return;
        };
        let Some((function, class)) = self.replacements.iter().find(|(f, _)| is_name(&name, f))
        else {
            return;
        };
        let args = printed_args(ctx, call);
        let span = node_span(&node);
        let code = dereferenced(span, ancestors, format!("new \\{class}({args})"));
        out.push(Report::fixed(
            span,
            format!("Replace the call to `{function}()` with `new \\{class}()`."),
            Safety::Unsafe,
            vec![Edit::new(span, code)],
        ));
    }
}

pub struct StaticReplacement {
    pub function: String,
    pub class: String,
    pub method: String,
}

#[derive(Default)]
pub struct ReplaceFunctionWithStatic {
    pub replacements: Vec<StaticReplacement>,
}

impl Rule for ReplaceFunctionWithStatic {
    fn id(&self) -> &'static str {
        "lint/refactor/replace-function-with-static"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        const KEYS: &[&str] = &["old-func-name", "new-class-name", "new-method-name"];
        let mut replacements = Vec::new();
        for item in items(options)? {
            let map = object(item, KEYS, KEYS)?;
            let replacement = StaticReplacement {
                function: string(map, "old-func-name")?,
                class: string(map, "new-class-name")?,
                method: string(map, "new-method-name")?,
            };
            assert_function(&replacement.function)?;
            assert_class(&replacement.class)?;
            assert_method(&replacement.method)?;
            replacements.push(replacement);
        }
        self.replacements = replacements;
        Ok(())
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
        if self.replacements.is_empty() {
            return;
        }
        let Some((call, name)) = resolved(ctx, ancestors, node) else {
            return;
        };
        let Some(r) = self
            .replacements
            .iter()
            .find(|r| is_name(&name, &r.function))
        else {
            return;
        };
        let class = match r.class.as_str() {
            "self" | "parent" | "static" => r.class.clone(),
            other => format!("\\{other}"),
        };
        let args = printed_args(ctx, call);
        out.push(Report::fixed(
            node_span(&node),
            format!(
                "Replace the call to `{}()` with `{class}::{}()`.",
                r.function, r.method
            ),
            Safety::Unsafe,
            vec![Edit::new(
                node_span(&node),
                format!("{class}::{}({args})", r.method),
            )],
        ));
    }
}
