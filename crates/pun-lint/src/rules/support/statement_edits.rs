use mago_database::file::FileId;
use mago_span::HasSpan;
use mago_syntax::cst::*;
use mago_syntax::lexer::Lexer;
use mago_syntax::settings::LexerSettings;
use mago_syntax::token::TokenKind;
use mago_syntax_core::input::Input;
use pun_problem::{Edit, Span};

use crate::Context;
pub(crate) use crate::rules::support::statements::{Frame, line_indent};
use crate::syntax::{node_span, unparenthesize};

#[derive(Clone, Copy)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

pub(crate) fn tokens(source: &str, from: usize, to: usize) -> Vec<Token> {
    let input = Input::new(FileId::zero(), &source.as_bytes()[from..to]);
    let mut lexer = if from == 0 {
        Lexer::new(input, LexerSettings::default())
    } else {
        Lexer::scripting(input, LexerSettings::default())
    };
    let mut out = Vec::new();
    while let Some(Ok(token)) = lexer.advance() {
        let start = from + token.start.offset as usize;
        out.push(Token {
            kind: token.kind,
            start,
            end: start + token.value.len(),
        });
    }
    out
}

pub(crate) fn previous_token_end(ctx: &Context<'_>, from: usize, pos: usize) -> usize {
    let tokens = ctx.tokens();
    let Some(token) = tokens[..tokens.partition_point(|t| t.end <= pos)]
        .iter()
        .rev()
        .take_while(|t| t.start >= from)
        .find(|t| !t.kind.is_trivia())
    else {
        return from;
    };
    let mut end = token.end;
    if token.kind == TokenKind::OpenTag
        && ctx.source.as_bytes().get(end) == Some(&b'\n')
        && end < pos
    {
        end += 1;
    }
    end
}

pub(crate) fn trailing_comment(source: &str, from: usize, to: usize) -> bool {
    let mut found = false;
    for token in tokens(source, from, to) {
        if token.kind.is_comment() {
            found = true;
        } else if !token.kind.is_trivia() {
            found = false;
        }
    }
    found
}

pub(crate) fn first_comment(source: &str, from: usize, to: usize) -> Option<usize> {
    tokens(source, from, to)
        .into_iter()
        .find(|t| t.kind.is_comment())
        .map(|t| t.start)
}

pub(crate) struct List<'a> {
    pub stmts: Vec<&'a Statement<'a>>,
    pub nop: bool,
    pub frame: Frame,
    pub inner: Span,
}

impl<'a> List<'a> {
    pub(crate) fn is_empty(&self) -> bool {
        self.stmts.is_empty() && !self.nop
    }

    pub(crate) fn len(&self) -> usize {
        self.stmts.len() + usize::from(self.nop)
    }

    pub(crate) fn last(&self) -> Option<&'a Statement<'a>> {
        if self.nop {
            return None;
        }
        self.stmts.last().copied()
    }

    pub(crate) fn with_comments(&self, ctx: &crate::Context<'_>, i: usize) -> Span {
        let source = ctx.source;
        let span = node_span(self.stmts[i]);
        let before = self.before(ctx, i);
        Span::new(
            first_comment(source, before, span.start).unwrap_or(span.start),
            span.end,
        )
    }

    pub(crate) fn before(&self, ctx: &crate::Context<'_>, i: usize) -> usize {
        let from = if i == 0 {
            self.inner.start
        } else {
            node_span(self.stmts[i - 1]).end
        };
        previous_token_end(ctx, from, node_span(self.stmts[i]).start)
    }

    pub(crate) fn nop_span(&self, source: &str) -> Option<Span> {
        if !self.nop {
            return None;
        }
        let after = self
            .stmts
            .last()
            .map_or(self.inner.start, |s| node_span(*s).end);
        let start = first_comment(source, after, self.inner.end).unwrap_or(after);
        let end = self.inner.start + source[self.inner.start..self.inner.end].trim_end().len();
        Some(Span::new(start, end))
    }
}

fn kept(statement: &Statement<'_>) -> bool {
    match statement {
        Statement::OpeningTag(_) | Statement::ClosingTag(_) | Statement::Noop(_) => false,
        Statement::Inline(inline) => {
            !(matches!(inline.value, b"\n" | b"\r\n") && inline.span.start.offset > 0)
        }
        _ => true,
    }
}

fn list<'a>(
    source: &str,
    all: impl IntoIterator<Item = &'a Statement<'a>>,
    frame: Frame,
    inner: Span,
    nop: bool,
) -> List<'a> {
    let stmts: Vec<&Statement> = all.into_iter().filter(|s| kept(s)).collect();
    let after = stmts.last().map_or(inner.start, |s| node_span(*s).end);
    List {
        nop: nop && trailing_comment(source, after, inner.end),
        stmts,
        frame,
        inner,
    }
}

fn start(s: mago_span::Span) -> usize {
    s.start.offset as usize
}

fn end(s: mago_span::Span) -> usize {
    s.end.offset as usize
}

pub(crate) fn block_list<'a>(source: &str, block: &'a Block<'a>) -> List<'a> {
    let (open, close) = (end(block.left_brace), start(block.right_brace));
    list(
        source,
        block.statements.iter(),
        Frame::Braces { open, close },
        Span::new(open, close),
        true,
    )
}

pub(crate) fn body_list<'a>(source: &str, statement: &'a Statement<'a>) -> List<'a> {
    match statement {
        Statement::Block(block) => block_list(source, block),
        _ => list(
            source,
            [statement],
            Frame::Bare,
            node_span(statement),
            false,
        ),
    }
}

pub(crate) fn open_list<'a>(
    source: &str,
    statements: &'a Sequence<'a, Statement<'a>>,
    from: usize,
    to: usize,
) -> List<'a> {
    list(
        source,
        statements.iter(),
        Frame::Open,
        Span::new(from, to),
        true,
    )
}

pub(crate) const HOLDERS: &[NodeKind] = &[
    NodeKind::Program,
    NodeKind::Block,
    NodeKind::NamespaceImplicitBody,
    NodeKind::SwitchExpressionCase,
    NodeKind::SwitchDefaultCase,
    NodeKind::IfColonDelimitedBody,
    NodeKind::IfColonDelimitedBodyElseIfClause,
    NodeKind::IfColonDelimitedBodyElseClause,
    NodeKind::WhileColonDelimitedBody,
    NodeKind::ForColonDelimitedBody,
    NodeKind::ForeachColonDelimitedBody,
    NodeKind::DeclareColonDelimitedBody,
];

pub(crate) fn holder_list<'a>(
    source: &str,
    holder: Node<'a, 'a>,
    ancestors: &[Node<'a, 'a>],
) -> Option<List<'a>> {
    let open =
        |s: &'a Sequence<'a, Statement<'a>>, from: usize, to: usize| open_list(source, s, from, to);
    Some(match holder {
        Node::Block(block) => {
            if matches!(ancestors.last(), Some(Node::PropertyHookConcreteBody(_))) {
                return None;
            }
            block_list(source, block)
        }
        Node::Program(program) => open(&program.statements, 0, source.len()),
        Node::NamespaceImplicitBody(body) => {
            let to = body
                .statements
                .last()
                .map_or(end(body.span()), |s| end(s.span()));
            open(&body.statements, end(body.terminator.span()), to)
        }
        Node::SwitchExpressionCase(case) => {
            case_list(source, &case.statements, end(case.separator.span()))
        }
        Node::SwitchDefaultCase(case) => {
            case_list(source, &case.statements, end(case.separator.span()))
        }
        Node::IfColonDelimitedBody(body) => {
            let to = body
                .else_if_clauses
                .first()
                .map(|c| start(c.span()))
                .or_else(|| body.else_clause.as_ref().map(|c| start(c.span())))
                .unwrap_or(start(body.endif.span()));
            open(&body.statements, end(body.colon), to)
        }
        Node::IfColonDelimitedBodyElseIfClause(clause) => open(
            &clause.statements,
            end(clause.colon),
            clause_end(source, clause.span()),
        ),
        Node::IfColonDelimitedBodyElseClause(clause) => open(
            &clause.statements,
            end(clause.colon),
            clause_end(source, clause.span()),
        ),
        Node::WhileColonDelimitedBody(body) => open(
            &body.statements,
            end(body.colon),
            start(body.end_while.span()),
        ),
        Node::ForColonDelimitedBody(body) => open(
            &body.statements,
            end(body.colon),
            start(body.end_for.span()),
        ),
        Node::ForeachColonDelimitedBody(body) => open(
            &body.statements,
            end(body.colon),
            start(body.end_foreach.span()),
        ),
        Node::DeclareColonDelimitedBody(body) => open(
            &body.statements,
            end(body.colon),
            start(body.end_declare.span()),
        ),
        _ => return None,
    })
}

fn clause_end(source: &str, span: mago_span::Span) -> usize {
    let from = end(span);
    tokens(source, from, source.len())
        .into_iter()
        .find(|t| !t.kind.is_trivia())
        .map_or(source.len(), |t| t.start)
}

fn case_list<'a>(
    source: &str,
    statements: &'a Sequence<'a, Statement<'a>>,
    from: usize,
) -> List<'a> {
    let to = statements.last().map_or(from, |s| end(s.span()));
    list(
        source,
        statements.iter(),
        Frame::Open,
        Span::new(from, to),
        false,
    )
}

pub(crate) fn remove(ctx: &crate::Context<'_>, list: &List<'_>, indices: &[usize]) -> Vec<Edit> {
    let source = ctx.source;
    if indices.len() == list.stmts.len() && !list.nop {
        match list.frame {
            Frame::Braces { open, close } => {
                let indent = line_indent(source, open - 1);
                return vec![Edit::new(Span::new(open, close), format!("\n{indent}"))];
            }
            Frame::Bare => {
                let span = node_span(list.stmts[0]);
                let indent = line_indent(source, span.start);
                return vec![Edit::new(span, format!("{{\n{indent}}}"))];
            }
            Frame::Open => {}
        }
    }
    indices
        .iter()
        .map(|&i| {
            let span = node_span(list.stmts[i]);
            let before = list.before(ctx, i);
            if i > 0 {
                return Edit::new(Span::new(before, span.end), "");
            }
            let start = first_comment(source, before, span.start).unwrap_or(span.start);
            let rest = &source[span.end..list.inner.end];
            let blanks = rest.len() - rest.trim_start().len();
            Edit::new(Span::new(start, span.end + blanks), "")
        })
        .collect()
}

pub(crate) fn reindent(source: &str, span: Span, from: &str, to: &str) -> String {
    let text = &source[span.start..span.end];
    if from == to {
        return text.to_string();
    }
    let toks = tokens(source, span.start, span.end);
    let mut documents = Vec::new();
    let mut open = None;
    for t in &toks {
        match t.kind {
            TokenKind::DocumentStart(_) => open = Some(t.start),
            TokenKind::DocumentEnd => documents.extend(open.take().map(|s| (s, t.end))),
            _ => {}
        }
    }
    let protected = |pos: usize| {
        documents.iter().any(|&(s, e)| s < pos && pos <= e)
            || toks
                .iter()
                .any(|t| !t.kind.is_trivia() && t.start < pos && pos < t.end)
    };
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut offset = span.start;
    while let Some(i) = rest.find('\n') {
        out.push_str(&rest[..=i]);
        offset += i + 1;
        rest = &rest[i + 1..];
        if protected(offset) || rest.starts_with('\n') {
            continue;
        }
        if let Some(stripped) = rest.strip_prefix(from) {
            out.push_str(to);
            offset += from.len();
            rest = stripped;
        }
    }
    out.push_str(rest);
    out
}

pub(crate) fn moved_statements(ctx: &crate::Context<'_>, list: &List<'_>, to: &str) -> String {
    let source = ctx.source;
    let mut parts: Vec<String> = (0..list.stmts.len())
        .map(|i| {
            let span = list.with_comments(ctx, i);
            reindent(source, span, line_indent(source, span.start), to)
        })
        .collect();
    if let Some(span) = list.nop_span(source) {
        parts.push(reindent(source, span, line_indent(source, span.start), to));
    }
    parts.join(&format!("\n{to}"))
}

pub(crate) fn inserted_statements(ctx: &crate::Context<'_>, list: &List<'_>, to: &str) -> String {
    let mut text = moved_statements(ctx, list, to);
    if list.nop {
        text.push('\n');
    }
    text
}

pub(crate) struct IfView<'a> {
    pub condition: &'a Expression<'a>,
    pub stmts: List<'a>,
    pub elseifs: Vec<(&'a Expression<'a>, List<'a>, Span)>,
    pub else_: Option<(List<'a>, Span)>,
    pub colon: bool,
}

pub(crate) fn if_view<'a>(source: &str, r#if: &'a If<'a>) -> IfView<'a> {
    match &r#if.body {
        IfBody::Statement(body) => IfView {
            condition: r#if.condition,
            stmts: body_list(source, body.statement),
            elseifs: body
                .else_if_clauses
                .iter()
                .map(|c| (c.condition, body_list(source, c.statement), node_span(c)))
                .collect(),
            else_: body
                .else_clause
                .as_ref()
                .map(|c| (body_list(source, c.statement), node_span(c))),
            colon: false,
        },
        IfBody::ColonDelimited(body) => {
            let to = body
                .else_if_clauses
                .first()
                .map(|c| start(c.span()))
                .or_else(|| body.else_clause.as_ref().map(|c| start(c.span())))
                .unwrap_or(start(body.endif.span()));
            IfView {
                condition: r#if.condition,
                stmts: open_list(source, &body.statements, end(body.colon), to),
                elseifs: body
                    .else_if_clauses
                    .iter()
                    .map(|c| {
                        let list = open_list(
                            source,
                            &c.statements,
                            end(c.colon),
                            clause_end(source, c.span()),
                        );
                        (c.condition, list, node_span(c))
                    })
                    .collect(),
                else_: body.else_clause.as_ref().map(|c| {
                    let list = open_list(
                        source,
                        &c.statements,
                        end(c.colon),
                        clause_end(source, c.span()),
                    );
                    (list, node_span(c))
                }),
                colon: true,
            }
        }
    }
}

pub(crate) fn expression<'a>(statement: &'a Statement<'a>) -> Option<&'a Expression<'a>> {
    match statement {
        Statement::Expression(e) => Some(unparenthesize(e.expression)),
        _ => None,
    }
}

pub(crate) fn assign<'a>(expr: &'a Expression<'a>) -> Option<&'a Assignment<'a>> {
    match unparenthesize(expr) {
        Expression::Assignment(a) if is_plain_assign(a) => Some(a),
        _ => None,
    }
}

pub(crate) fn is_plain_assign(a: &Assignment<'_>) -> bool {
    matches!(a.operator, AssignmentOperator::Assign(_))
        && !matches!(a.rhs, Expression::UnaryPrefix(u) if u.operator.is_reference())
}

pub(crate) fn variable_name<'a>(expr: &'a Expression<'a>) -> Option<&'a [u8]> {
    match unparenthesize(expr) {
        Expression::Variable(Variable::Direct(v)) => Some(&v.name[1..]),
        _ => None,
    }
}

pub(crate) fn is_exit_or_throw(expr: &Expression<'_>) -> bool {
    matches!(
        unparenthesize(expr),
        Expression::Construct(Construct::Exit(_) | Construct::Die(_)) | Expression::Throw(_)
    )
}

fn tokens_with_comments(source: &str, span: Span) -> Vec<(TokenKind, String)> {
    tokens(source, span.start, span.end)
        .into_iter()
        .filter(|t| t.kind != TokenKind::Whitespace)
        .map(|t| {
            let text = &source[t.start..t.end];
            let text = if t.kind.is_comment() {
                text.lines().map(str::trim).collect::<Vec<_>>().join("\n")
            } else {
                text.to_string()
            };
            (t.kind, text)
        })
        .collect()
}

pub(crate) fn same_with_comments(ctx: &crate::Context<'_>, a: &List<'_>, b: &List<'_>) -> bool {
    let source = ctx.source;
    tokens_with_comments(source, printed(ctx, a)) == tokens_with_comments(source, printed(ctx, b))
}

pub(crate) fn printed(ctx: &crate::Context<'_>, list: &List<'_>) -> Span {
    let source = ctx.source;
    let start = if list.stmts.is_empty() {
        list.nop_span(source).map_or(list.inner.start, |s| s.start)
    } else {
        list.with_comments(ctx, 0).start
    };
    let end = list
        .nop_span(source)
        .map(|s| s.end)
        .or_else(|| list.stmts.last().map(|s| node_span(*s).end))
        .unwrap_or(start);
    Span::new(start, end.max(start))
}

pub(crate) fn enclosing<'a>(
    source: &str,
    statement: &'a Statement<'a>,
    ancestors: &[Node<'a, 'a>],
) -> (List<'a>, usize) {
    let n = ancestors.len();
    let list = match ancestors[n - 1] {
        Node::Block(block) => block_list(source, block),
        holder => match holder_list(source, holder, &ancestors[..n - 1]) {
            Some(list) => list,
            None => body_list(source, statement),
        },
    };
    let this = node_span(statement);
    let index = list
        .stmts
        .iter()
        .position(|s| node_span(*s) == this)
        .unwrap();
    (list, index)
}

pub(crate) fn replace_statement(
    source: &str,
    list: &List<'_>,
    index: usize,
    text: impl Fn(&str) -> String,
) -> Edit {
    let span = node_span(list.stmts[index]);
    let indent = line_indent(source, span.start);
    if list.frame == Frame::Bare {
        let inner = format!("{indent}    ");
        return Edit::new(span, format!("{{\n{inner}{}\n{indent}}}", text(&inner)));
    }
    Edit::new(span, text(indent))
}

pub(crate) fn comments(source: &str, from: usize, to: usize) -> Option<Span> {
    let comments: Vec<Token> = tokens(source, from, to)
        .into_iter()
        .filter(|t| t.kind.is_comment())
        .collect();
    Some(Span::new(comments.first()?.start, comments.last()?.end))
}

pub(crate) fn braced(ctx: &crate::Context<'_>, list: &List<'_>, indent: &str) -> String {
    if list.is_empty() {
        return format!("{{\n{indent}}}");
    }
    let inner = format!("{indent}    ");
    format!(
        "{{\n{inner}{}\n{indent}}}",
        moved_statements(ctx, list, &inner)
    )
}

pub(crate) fn has_equal_variable(source: &str, node: Node<'_, '_>, target: Span) -> bool {
    if matches!(
        node,
        Node::DirectVariable(_) | Node::IndirectVariable(_) | Node::NestedVariable(_)
    ) && crate::syntax::same_code(source, node_span(&node), target)
    {
        return true;
    }
    let mut found = false;
    node.visit_children(|child| found = found || has_equal_variable(source, child, target));
    found
}

pub(crate) fn is_static_or_global(chain: &[Node<'_, '_>], name: &[u8]) -> bool {
    for j in 0..chain.len().saturating_sub(1) {
        let (holder, Node::Statement(statement)) = (chain[j], chain[j + 1]) else {
            continue;
        };
        if !HOLDERS.contains(&holder.kind())
            || chain[j + 1..]
                .iter()
                .any(|n| matches!(n, Node::Class(_) | Node::AnonymousClass(_)))
        {
            continue;
        }
        let this = node_span(statement);
        let before = crate::syntax::statements(holder);
        let declared = before
            .iter()
            .take_while(|s| node_span(**s) != this)
            .any(|s| match s {
                Statement::Global(g) => g
                    .variables
                    .iter()
                    .any(|v| matches!(v, Variable::Direct(d) if &d.name[1..] == name)),
                Statement::Static(s) => s.items.iter().any(|i| &i.variable().name[1..] == name),
                _ => false,
            });
        if declared {
            return true;
        }
    }
    false
}

pub(crate) fn is_used_by_reference(ancestors: &[Node<'_, '_>], name: &[u8], at: usize) -> bool {
    let mut outermost = None;
    for node in ancestors {
        let (params, uses) = match node {
            Node::Function(f) => (Some(&f.parameter_list), None),
            Node::Method(m) => (Some(&m.parameter_list), None),
            Node::Closure(c) => (Some(&c.parameter_list), c.use_clause.as_ref()),
            Node::ArrowFunction(a) => (Some(&a.parameter_list), None),
            _ => continue,
        };
        outermost.get_or_insert(*node);
        let by_ref_param = params.is_some_and(|p| {
            p.parameters
                .iter()
                .any(|p| p.ampersand.is_some() && &p.variable.name[1..] == name)
        });
        if by_ref_param || uses.is_some_and(|u| by_ref_use(u, name)) {
            return true;
        }
    }
    fn closures_before(node: Node<'_, '_>, name: &[u8], at: usize) -> bool {
        if node_span(&node).start >= at {
            return false;
        }
        if let Node::Closure(c) = node
            && c.use_clause.as_ref().is_some_and(|u| by_ref_use(u, name))
        {
            return true;
        }
        let mut found = false;
        node.visit_children(|child| found = found || closures_before(child, name, at));
        found
    }
    let Some(outermost) = outermost else {
        return false;
    };
    let mut found = false;
    outermost.visit_children(|child| found = found || closures_before(child, name, at));
    found
}

fn by_ref_use(clause: &ClosureUseClause<'_>, name: &[u8]) -> bool {
    clause
        .variables
        .iter()
        .any(|v| v.ampersand.is_some() && &v.variable.name[1..] == name)
}

pub(crate) fn has_binary(node: Node<'_, '_>, op: &dyn Fn(&BinaryOperator<'_>) -> bool) -> bool {
    if let Node::Binary(b) = node
        && op(&b.operator)
    {
        return true;
    }
    let mut found = false;
    node.visit_children(|child| found = found || has_binary(child, op));
    found
}

pub(crate) fn is_in_constant_expression(ancestors: &[Node<'_, '_>]) -> bool {
    ancestors.iter().any(|a| {
        matches!(
            a,
            Node::FunctionLikeParameterDefaultValue(_)
                | Node::PropertyConcreteItem(_)
                | Node::ClassLikeConstantItem(_)
        )
    })
}

pub(crate) fn is_safe_to_evaluate_eagerly(expr: &Expression<'_>) -> bool {
    match crate::syntax::unparenthesize(expr) {
        Expression::Variable(v) => matches!(v, Variable::Direct(_)),
        e => !crate::rules::support::statements::is_dynamic_expr(e),
    }
}

pub(crate) fn compared_values<'a>(
    binary: &'a Binary<'a>,
    chain: &dyn Fn(&BinaryOperator<'_>) -> bool,
    compare: &dyn Fn(&BinaryOperator<'_>) -> bool,
) -> Option<Vec<(&'a Expression<'a>, &'a Expression<'a>)>> {
    let comparison = |e: &'a Expression<'a>| match unparenthesize(e) {
        Expression::Binary(b) if compare(&b.operator) => Some((b.lhs, b.rhs)),
        _ => None,
    };
    let mut out = vec![comparison(binary.rhs)?];
    let mut current = binary;
    while let Expression::Binary(left) = unparenthesize(current.lhs)
        && chain(&left.operator)
    {
        out.push(comparison(left.rhs)?);
        current = left;
    }
    out.push(comparison(current.lhs)?);
    out.reverse();
    Some(out)
}

pub(crate) fn replace_run(
    ctx: &crate::Context<'_>,
    list: &List<'_>,
    first: usize,
    last: usize,
    new: &str,
) -> Edit {
    let source = ctx.source;
    let end = node_span(list.stmts[last]).end;
    if first > 0 {
        let prev = node_span(list.stmts[first - 1]);
        let indent = line_indent(source, prev.start);
        return Edit::new(Span::new(prev.end, end), format!("\n{indent}{new}"));
    }
    let start = list.with_comments(ctx, first).start;
    let next = match list.stmts.get(last + 1) {
        Some(_) => Some(list.with_comments(ctx, last + 1).start),
        None => list.nop_span(source).map(|s| s.start),
    };
    match next {
        Some(next) => Edit::new(
            Span::new(start, next),
            format!("{new}\n{}", line_indent(source, next)),
        ),
        None => Edit::new(Span::new(start, end), new),
    }
}

pub(crate) fn contains_html(node: Node<'_, '_>, ancestors: &[Node<'_, '_>]) -> bool {
    let mut stack = vec![ancestors.first().copied().unwrap_or(node)];
    while let Some(node) = stack.pop() {
        if let Node::Statement(Statement::Inline(inline)) = node
            && !(matches!(inline.value, b"\n" | b"\r\n") && inline.span.start.offset > 0)
        {
            return true;
        }
        node.visit_children(|child| stack.push(child));
    }
    false
}

pub(crate) fn lines_between(source: &str, from: usize, to: usize) -> usize {
    source[from..to].bytes().filter(|&b| b == b'\n').count()
}
