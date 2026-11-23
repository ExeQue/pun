//! Rector 2.8.4's `ClosureToArrowFunctionRector`, with `ClosureArrowFunctionAnalyzer` and the way
//! `BetterStandardPrinter::pExpr_ArrowFunction` prints the comments of the moved `return`.

use mago_syntax::cst::{ArrayElement, Closure, Expression, Literal, Node, NodeKind, Statement};

use crate::problem::{Edit, Safety, Span};
use crate::rules::support::arguments::value as argument_value;
use crate::rules::support::expressions::named;
use crate::rules::support::statement_edits::reindent;
use crate::rules::support::statements::{line_indent, parent_of, tokens, within};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferArrowFunction;

impl Rule for PreferArrowFunction {
    fn id(&self) -> &'static str {
        "lint/php-7.4/prefer-arrow-function"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(70400)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Closure])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Closure(closure) = node else {
            return;
        };
        let Some((value, comments)) = arrow_expression(ctx, closure) else {
            return;
        };
        // `IS_CLOSURE_IN_ATTRIBUTE`.
        if ancestors.iter().any(|n| matches!(n, Node::Attribute(_))) {
            return;
        }
        let source = ctx.source;
        let span = node_span(closure);
        let level = line_indent(source, span.start);
        let mut arrow = String::new();
        for list in &closure.attribute_lists {
            arrow.push_str(text(source, node_span(list)));
            arrow.push(' ');
        }
        if closure.r#static.is_some() {
            arrow.push_str("static ");
        }
        arrow.push_str("fn");
        if closure.ampersand.is_some() {
            arrow.push('&');
        }
        let params: Vec<&str> = (closure.parameter_list.parameters.iter())
            .map(|p| text(source, node_span(p)))
            .collect();
        arrow.push('(');
        arrow.push_str(&params.join(", "));
        arrow.push(')');
        if let Some(hint) = &closure.return_type_hint {
            arrow.push_str(": ");
            arrow.push_str(text(source, node_span(&hint.hint)));
        }
        arrow.push_str(" => ");
        let value_span = node_span(value);
        arrow.push_str(&reindent(
            source,
            value_span,
            line_indent(source, value_span.start),
            level,
        ));
        let (fixup, parens) = context(closure, ancestors);
        if parens {
            arrow = format!("({arrow})");
        }
        let code = if comments.is_empty() {
            arrow
        } else {
            let argument = matches!(
                parent_of(ancestors),
                Some(Node::PositionalArgument(_) | Node::NamedArgument(_))
            );
            let deeper = format!("{level}    ");
            let (indent, mut code) = if argument || fixup {
                (deeper.clone(), format!("\n{deeper}"))
            } else {
                (level.to_string(), String::new())
            };
            for (i, comment) in comments.iter().enumerate() {
                if i > 0 {
                    code.push_str(&indent);
                }
                code.push_str(comment.trim_end());
                code.push('\n');
            }
            code.push_str(&indent);
            code.push_str(&arrow);
            code
        };
        out.push(Report::fixed(
            span,
            "Change the closure to an arrow function.",
            Safety::Unsafe,
            vec![Edit::new(span, code)],
        ));
    }
}

fn text(source: &str, span: Span) -> &str {
    &source[span.start..span.end]
}

/// `matchArrowFunctionExpr`: the returned expression of a closure whose only statement is `return <expr>;`, with
/// the comments before that `return`.
fn arrow_expression<'a>(
    ctx: &Context<'a>,
    closure: &'a Closure<'a>,
) -> Option<(&'a Expression<'a>, Vec<&'a str>)> {
    let [Statement::Return(ret)] = closure.body.statements.as_slice() else {
        return None;
    };
    let value = ret.value?;
    let toks = tokens(ctx);
    let ret_span = node_span(ret);
    // Comments after the `return` are a `Nop` statement in php-parser: two statements.
    let after = within(
        toks,
        ret_span.end,
        closure.body.right_brace.start.offset as usize,
    );
    if after.iter().any(|t| t.kind.is_comment()) {
        return None;
    }
    let before = within(
        toks,
        closure.body.left_brace.end.offset as usize,
        ret_span.start,
    );
    let comments: Vec<&str> = (before.iter())
        .filter(|t| t.kind.is_comment())
        .map(|t| &ctx.source[t.start..t.end])
        .collect();
    if used_referenced_value(closure) || use_in_compact(closure) {
        return None;
    }
    // `shouldSkipMoreSpecificTypeWithVarDoc`: the `return`'s doc comment has a `@var` tag.
    let doc = comments.iter().rev().find(|c| c.starts_with("/**"));
    if doc.is_some_and(|d| has_var_tag(d)) {
        return None;
    }
    Some((value, comments))
}

fn has_var_tag(doc: &str) -> bool {
    doc.split("@var").skip(1).any(|rest| {
        rest.starts_with([' ', '\t'])
            && rest
                .trim_start_matches([' ', '\t'])
                .starts_with(|c: char| !c.is_whitespace() && c != '*')
    })
}

/// `BetterNodeFinder::findFirstInFunctionLikeScoped`: `filter` on every node of the body, not entering a class or a
/// function-like that it rejects.
fn scoped<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    filter: &mut dyn FnMut(Node<'ast, 'arena>) -> bool,
) -> bool {
    let mut found = false;
    node.visit_children(|child| {
        if found {
            return;
        }
        if filter(child) {
            found = true;
        } else if !matches!(
            child,
            Node::Closure(_)
                | Node::ArrowFunction(_)
                | Node::Function(_)
                | Node::AnonymousClass(_)
                | Node::Class(_)
                | Node::Method(_)
        ) {
            found = scoped(child, filter);
        }
    });
    found
}

/// `shouldSkipForUsedReferencedValue`: a variable the closure uses by reference is read in its body, or a closure
/// inside uses it by reference too.
fn used_referenced_value(closure: &Closure<'_>) -> bool {
    let Some(uses) = &closure.use_clause else {
        return false;
    };
    let referenced: Vec<&[u8]> = (uses.variables.iter())
        .filter(|u| u.ampersand.is_some())
        .map(|u| u.variable.name)
        .collect();
    if referenced.is_empty() {
        return false;
    }
    let body = Node::Block(&closure.body);
    scoped(
        body,
        &mut |n| matches!(n, Node::DirectVariable(v) if referenced.contains(&v.name)),
    ) || scoped(body, &mut |n| {
        let Node::Closure(inner) = n else {
            return false;
        };
        (inner.use_clause.iter().flat_map(|u| u.variables.iter()))
            .any(|u| u.ampersand.is_some() && referenced.contains(&u.variable.name))
    })
}

/// `shouldSkipForUseVariableUsedByCompact`.
fn use_in_compact(closure: &Closure<'_>) -> bool {
    let Some(uses) = &closure.use_clause else {
        return false;
    };
    let names: Vec<&[u8]> = (uses.variables.iter())
        .map(|u| &u.variable.name[1..])
        .collect();
    scoped(Node::Block(&closure.body), &mut |n| match n {
        Node::FunctionCall(call) if named(call, "compact") => {
            (call.argument_list.arguments.iter()).any(|a| in_compact(argument_value(a), &names))
        }
        _ => false,
    })
}

fn in_compact(value: &Expression<'_>, names: &[&[u8]]) -> bool {
    let elements = match value {
        Expression::Literal(Literal::String(s)) => {
            return s.value.is_some_and(|v| names.contains(&v));
        }
        Expression::Array(a) => &a.elements,
        Expression::LegacyArray(a) => &a.elements,
        _ => return false,
    };
    elements.iter().any(|e| match e {
        ArrayElement::KeyValue(e) => in_compact(e.value, names),
        ArrayElement::Value(e) => in_compact(e.value, names),
        ArrayElement::Variadic(_) | ArrayElement::Missing(_) => false,
    })
}

/// Where php-parser prints the new arrow function: whether its parent fixes up the precedence of the slot (so the
/// printer does not print it at `MAX_PRECEDENCE`), and whether that slot needs parentheses around an arrow function.
fn context(closure: &Closure<'_>, ancestors: &[Node<'_, '_>]) -> (bool, bool) {
    let span = node_span(closure);
    match parent_of(ancestors) {
        Some(
            Node::Assignment(_)
            | Node::Binary(_)
            | Node::UnaryPrefix(_)
            | Node::Clone(_)
            | Node::PrintConstruct(_)
            | Node::IncludeConstruct(_)
            | Node::IncludeOnceConstruct(_)
            | Node::RequireConstruct(_)
            | Node::RequireOnceConstruct(_)
            | Node::YieldValue(_)
            | Node::YieldPair(_),
        ) => (true, true),
        Some(Node::Conditional(c)) => {
            let then = c.then.is_some_and(|t| node_span(t) == span);
            (!then, !then)
        }
        Some(Node::Throw(_)) => (true, false),
        _ => (false, false),
    }
}
