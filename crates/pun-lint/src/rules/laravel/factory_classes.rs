//! rector-laravel 2.7.3's `FactoryDefinitionRector` (the legacy-factories migration, Laravel 7 to 8) with its
//! `ModelFactoryNodeFactory`: the `$factory->define()`, `state()`, `afterMaking()` and `afterCreating()` calls of a
//! legacy factory file become one class-based factory per model.

use mago_syntax::cst::{
    Access, Argument, Call, Expression, FunctionLikeParameterList, Literal, NamespaceBody, Node,
    NodeKind, Sequence, Statement, Variable,
};
use std::fmt::Write as _;

use super::member_name;
use super::upgrade_models::allow_list;
use crate::problem::{Edit, Safety, Span};
use crate::rules::support::expressions::text;
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::statement_edits::{line_indent, open_list, reindent, remove};
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

const FACTORY: &str = "\\Illuminate\\Database\\Eloquent\\Factories\\Factory";

/// `FactoryDefinitionRector`.
#[derive(Default)]
pub struct PreferFactoryClasses {
    pub models: Vec<String>,
}

/// A member of a factory class being built, or its `configure()` with the `after*()` calls it chains.
enum Member {
    Method(String),
    Configure(Vec<String>),
}

struct Factory {
    key: String,
    class: String,
    model: String,
    members: Vec<Member>,
}

/// The source text of `span`, with `replaced` (sorted, inside `span`, each on one line) written instead and every
/// line moved from the indentation `from` to `to`.
fn moved(
    source: &str,
    span: Span,
    mut replaced: Vec<(Span, String)>,
    from: &str,
    to: &str,
) -> String {
    replaced.sort_by_key(|(s, _)| s.start);
    let mut out = String::new();
    let mut at = span.start;
    for (s, code) in replaced {
        out.push_str(&reindent(source, Span::new(at, s.start), from, to));
        out.push_str(&code);
        at = s.end;
    }
    out.push_str(&reindent(source, Span::new(at, span.end), from, to));
    out
}

/// `fakerVariableToPropertyFetch()`: every `$<name>` in `node` becomes `$this->faker`.
fn faker_uses(node: Node<'_, '_>, name: &[u8], out: &mut Vec<(Span, String)>) {
    if let Node::DirectVariable(v) = node
        && v.name == name
    {
        out.push((node_span(&node), "$this->faker".to_string()));
        return;
    }
    node.visit_children(|c| faker_uses(c, name, out));
}

/// The edit that removes parameter `index` from `list`, or `None` when it has no such parameter.
fn without_param(
    list: &FunctionLikeParameterList<'_>,
    keyword_end: usize,
    arrow: bool,
    index: usize,
) -> Option<Vec<(Span, String)>> {
    let params: Vec<Span> = list.parameters.iter().map(node_span).collect();
    let span = *params.get(index)?;
    if params.len() == 1 {
        // php-parser reprints a callable that lost its only parameter: `function () {`, `fn() =>`.
        let open = list.left_parenthesis.start.offset as usize;
        let close = list.right_parenthesis.start.offset as usize;
        return Some(vec![
            (
                Span::new(keyword_end, open),
                if arrow {
                    String::new()
                } else {
                    " ".to_string()
                },
            ),
            (Span::new(open + 1, close), String::new()),
        ]);
    }
    Some(vec![match index {
        0 => (Span::new(span.start, params[1].start), String::new()),
        _ => (Span::new(params[index - 1].end, span.end), String::new()),
    }])
}

enum Callable<'a> {
    Closure(&'a mago_syntax::cst::Closure<'a>),
    Arrow(&'a mago_syntax::cst::ArrowFunction<'a>),
}

fn callable<'a>(expr: &'a Expression<'a>) -> Option<Callable<'a>> {
    match expr {
        Expression::Closure(c) => Some(Callable::Closure(c)),
        Expression::ArrowFunction(a) => Some(Callable::Arrow(a)),
        _ => None,
    }
}

impl<'a> Callable<'a> {
    fn list(&self) -> &'a FunctionLikeParameterList<'a> {
        match self {
            Callable::Closure(c) => &c.parameter_list,
            Callable::Arrow(a) => &a.parameter_list,
        }
    }

    fn param_name(&self, index: usize) -> Option<&'a [u8]> {
        self.list().parameters.get(index).map(|p| p.variable.name)
    }

    /// The statements' node (the closure's body or the arrow function's expression).
    fn body(&self) -> Node<'a, 'a> {
        match self {
            Callable::Closure(c) => Node::Block(&c.body),
            Callable::Arrow(a) => Node::Expression(a.expression),
        }
    }

    fn faker(&self, index: usize) -> Vec<(Span, String)> {
        let mut out = Vec::new();
        if let Some(name) = self.param_name(index) {
            faker_uses(self.body(), name, &mut out);
        }
        out
    }

    /// The callable, its parameter `drop` (the faker) removed and its uses written as `$this->faker`.
    fn rewritten(&self, source: &str, drop: usize, from: &str, to: &str) -> String {
        let (span, keyword_end, arrow) = match self {
            Callable::Closure(c) => (node_span(*c), c.function.span.end.offset as usize, false),
            Callable::Arrow(a) => (node_span(*a), a.r#fn.span.end.offset as usize, true),
        };
        let mut edits = self.faker(drop);
        edits.extend(without_param(self.list(), keyword_end, arrow, drop).unwrap_or_default());
        moved(source, span, edits, from, to)
    }

    /// `createDefinition()`: the statements of the callable as a method body at `to`.
    fn statements(&self, source: &str, from: &str, to: &str) -> String {
        let edits = self.faker(0);
        match self {
            Callable::Closure(c) => {
                let inner = Span::new(
                    c.body.left_brace.end.offset as usize,
                    c.body.right_brace.start.offset as usize,
                );
                let body = moved(source, inner, edits, &format!("{from}    "), to);
                let body = body.trim();
                if body.is_empty() {
                    String::new()
                } else {
                    format!("\n{to}{body}")
                }
            }
            Callable::Arrow(a) => {
                let expr = moved(source, node_span(a.expression), edits, from, to);
                format!("\n{to}return {expr};")
            }
        }
    }
}

fn statements<'a>(node: Node<'a, 'a>) -> Option<(&'a Sequence<'a, Statement<'a>>, Span)> {
    match node {
        Node::Program(p) => {
            if p.statements
                .iter()
                .any(|s| matches!(s, Statement::Namespace(_)))
            {
                return None;
            }
            Some((&p.statements, node_span(&node)))
        }
        Node::Namespace(n) => match &n.body {
            NamespaceBody::Implicit(b) => Some((
                &b.statements,
                Span::new(node_span(&b.terminator).end, node_span(&node).end),
            )),
            NamespaceBody::BraceDelimited(b) => Some((
                &b.statements,
                Span::new(
                    b.left_brace.end.offset as usize,
                    b.right_brace.start.offset as usize,
                ),
            )),
        },
        _ => None,
    }
}

fn is_factory_variable(expr: &Expression<'_>) -> bool {
    matches!(expr, Expression::Variable(Variable::Direct(v)) if v.name == b"$factory")
}

fn string_value(expr: &Expression<'_>) -> Option<String> {
    match expr {
        Expression::Literal(Literal::String(s)) => Some(String::from_utf8(s.value?.to_vec()).ok()?),
        _ => None,
    }
}

fn is_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_' || b >= 0x80)
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80)
}

impl Rule for PreferFactoryClasses {
    fn id(&self) -> &'static str {
        "lint/laravel-8/prefer-factory-classes"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.models = allow_list(options)?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Program, NodeKind::Namespace])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some((stmts, inner)) = statements(node) else {
            return;
        };
        let mut scope = ancestors.to_vec();
        scope.push(node);
        let source = ctx.source;
        let list = open_list(source, stmts, inner.start, inner.end);
        let stmts = &list.stmts;
        let mut factories: Vec<Factory> = Vec::new();
        let mut removed = Vec::new();
        for (i, statement) in stmts.iter().enumerate() {
            let Statement::Expression(e) = statement else {
                continue;
            };
            // do not refactor if the factory variable changes
            if let Expression::Assignment(a) = e.expression
                && is_factory_variable(a.lhs)
            {
                return;
            }
            let Expression::Call(Call::Method(call)) = e.expression else {
                continue;
            };
            let Some(method) =
                member_name(&call.method).map(|n| String::from_utf8_lossy(n).into_owned())
            else {
                continue;
            };
            let args: Vec<&Argument<'_>> = call.argument_list.arguments.iter().collect();
            let Some(Argument::Positional(first)) = args.first() else {
                continue;
            };
            let Expression::Access(Access::ClassConstant(constant)) = first.value else {
                continue;
            };
            let names = ["define", "state", "afterMaking", "afterCreating"];
            if first.ellipsis.is_some() || !names.iter().any(|n| n.eq_ignore_ascii_case(&method)) {
                continue;
            }
            let Expression::Identifier(id) = constant.class else {
                continue;
            };
            if !self.models.is_empty() {
                let resolved = resolve_class_name(&scope, id);
                if !self
                    .models
                    .iter()
                    .any(|m| m.eq_ignore_ascii_case(&resolved))
                {
                    return;
                }
            }
            let written = text(ctx, id).trim_start_matches('\\').to_string();
            let index = match factories.iter().position(|f| f.key == written) {
                Some(index) => index,
                None => {
                    let last = written.rsplit('\\').next().unwrap().to_string();
                    factories.push(Factory {
                        key: written.clone(),
                        class: format!("{last}Factory"),
                        model: text(ctx, first.value).to_string(),
                        members: Vec::new(),
                    });
                    factories.len() - 1
                }
            };
            removed.push(i);
            if !is_factory_variable(call.object) {
                continue;
            }
            let from = line_indent(source, node_span(*statement).start);
            let to = format!("{from}        ");
            let factory = &mut factories[index];
            let value = |n: usize| args.get(n).map(|a| a.value());
            match method.to_ascii_lowercase().as_str() {
                "define" => {
                    let Some(c) = value(1).and_then(callable) else {
                        continue;
                    };
                    let body = c.statements(source, from, &to);
                    factory.members.push(Member::Method(format!(
                        "public function definition()\n{from}    {{{body}\n{from}    }}"
                    )));
                }
                "state" => {
                    let (Some(state), Some(name)) = (value(2), value(1)) else {
                        continue;
                    };
                    let Some(name) = string_value(name).filter(|n| is_identifier(n)) else {
                        // Rector drops such a state, or writes an invalid method name
                        return;
                    };
                    let state = match callable(state) {
                        Some(c) if c.param_name(0).is_some() => c.rewritten(source, 0, from, &to),
                        _ => moved(source, node_span(state), Vec::new(), from, &to),
                    };
                    factory.members.push(Member::Method(format!(
                        "public function {name}()\n{from}    {{\n{to}return $this->state({state});\n{from}    }}"
                    )));
                }
                _ => {
                    let Some(c) = value(1).and_then(callable) else {
                        continue;
                    };
                    let after = format!("{method}({})", c.rewritten(source, 1, from, &to));
                    match factory.members.iter_mut().find_map(|m| match m {
                        Member::Configure(calls) => Some(calls),
                        Member::Method(_) => None,
                    }) {
                        Some(calls) => calls.push(after),
                        None => factory.members.push(Member::Configure(vec![after])),
                    }
                }
            }
        }
        if factories.is_empty() {
            return;
        }
        let indent = line_indent(source, node_span(stmts[*removed.last().unwrap()]).start);
        let classes: Vec<String> = factories
            .iter()
            .map(|f| {
                let mut class = format!(
                    "class {} extends {FACTORY}\n{indent}{{\n{indent}    protected $model = {};",
                    f.class, f.model
                );
                for member in &f.members {
                    let code = match member {
                        Member::Method(code) => code.clone(),
                        Member::Configure(calls) => format!(
                            "public function configure()\n{indent}    {{\n{indent}        return $this->{};\n{indent}    }}",
                            calls.join("->")
                        ),
                    };
                    let _ = write!(class, "\n{indent}    {code}");
                }
                let _ = write!(class, "\n{indent}}}");
                class
            })
            .collect();
        // Rector removes the statements and appends the classes; php-parser's format-preserving printer
        // replaces the removed statements at the end of the list in place when they are as many as the classes,
        // and otherwise drops every removed statement and writes each class on the next line.
        let trailing = removed
            .iter()
            .rev()
            .zip((0..stmts.len()).rev())
            .take_while(|(r, i)| r == &i)
            .count();
        let mut edits;
        if trailing == classes.len() {
            let (dropped, replaced) = removed.split_at(removed.len() - trailing);
            edits = remove(ctx, &list, dropped);
            for (&i, class) in replaced.iter().zip(classes) {
                edits.push(Edit::new(node_span(stmts[i]), class));
            }
        } else {
            edits = remove(ctx, &list, &removed);
            let end = node_span(stmts[stmts.len() - 1]).end;
            let added: String = classes.iter().map(|c| format!("\n{indent}{c}")).collect();
            match edits.iter_mut().find(|e| e.span.end == end) {
                Some(edit) => edit.text.push_str(&added),
                None => edits.push(Edit::new(Span::new(end, end), added)),
            }
        }
        out.push(Report::fixed(
            node_span(stmts[removed[0]]),
            "Define the legacy factory as a class-based factory.",
            Safety::Unsafe,
            edits,
        ));
    }
}
