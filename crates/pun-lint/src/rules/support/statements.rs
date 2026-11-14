use mago_database::file::FileId;
use mago_span::HasSpan;
use mago_syntax::cst::*;
use mago_syntax::lexer::Lexer;
use mago_syntax::settings::LexerSettings;
use mago_syntax::token::TokenKind;
use mago_syntax_core::input::Input;
use pun_problem::{Edit, Span};

use crate::syntax::{node_span, statements, unparenthesize};

#[derive(Clone, Copy)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

pub(crate) fn tokens<'c>(ctx: &'c crate::Context<'_>) -> &'c [Token] {
    ctx.tokens()
}

pub(crate) fn lex(source: &str) -> Vec<Token> {
    let mut lexer = Lexer::new(
        Input::new(FileId::zero(), source.as_bytes()),
        LexerSettings::default(),
    );
    let mut out = Vec::new();
    while let Some(Ok(token)) = lexer.advance() {
        let start = token.start.offset as usize;
        out.push(Token {
            kind: token.kind,
            start,
            end: start + token.value.len(),
        });
    }
    out
}

pub(crate) fn previous_token_end(source: &str, tokens: &[Token], pos: usize) -> usize {
    let before = tokens.partition_point(|t| t.end <= pos);
    let token = tokens[..before]
        .iter()
        .rev()
        .find(|t| !t.kind.is_trivia())
        .expect("the open tag");
    let mut end = token.end;
    if token.kind == TokenKind::OpenTag && source.as_bytes().get(end) == Some(&b'\n') && end < pos {
        end += 1;
    }
    end
}

pub(crate) fn trailing_comment(tokens: &[Token], from: usize, to: usize) -> bool {
    let inside = tokens.iter().filter(|t| t.start >= from && t.end <= to);
    let mut found = false;
    for token in inside {
        if token.kind.is_comment() {
            found = true;
        } else if !token.kind.is_trivia() {
            found = false;
        }
    }
    found
}

pub(crate) fn line_indent(source: &str, pos: usize) -> &str {
    let line_start = source[..pos].rfind('\n').map_or(0, |i| i + 1);
    let rest = &source[line_start..];
    let len = rest.len() - rest.trim_start_matches([' ', '\t']).len();
    &rest[..len]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Frame {
    Braces { open: usize, close: usize },
    Bare,
    Open,
}

pub(crate) struct List<'ast, 'arena> {
    pub stmts: Vec<&'ast Statement<'arena>>,
    pub nop: bool,
    pub frame: Frame,
    pub inner: Span,
}

impl List<'_, '_> {
    pub(crate) fn is_empty(&self) -> bool {
        self.stmts.is_empty() && !self.nop
    }

    pub(crate) fn len(&self) -> usize {
        self.stmts.len() + usize::from(self.nop)
    }
}

fn kept(statement: &Statement<'_>) -> bool {
    !matches!(
        statement,
        Statement::OpeningTag(_) | Statement::ClosingTag(_) | Statement::Noop(_)
    )
}

fn list<'ast, 'arena>(
    tokens: &[Token],
    all: impl IntoIterator<Item = &'ast Statement<'arena>>,
    frame: Frame,
    inner: Span,
) -> List<'ast, 'arena>
where
    'arena: 'ast,
{
    let stmts: Vec<&Statement> = all.into_iter().filter(|s| kept(s)).collect();
    let after = stmts.last().map_or(inner.start, |s| node_span(*s).end);
    List {
        nop: trailing_comment(tokens, after, inner.end),
        stmts,
        frame,
        inner,
    }
}

pub(crate) fn block_list<'ast, 'arena>(
    tokens: &[Token],
    block: &'ast Block<'arena>,
) -> List<'ast, 'arena> {
    let (open, close) = (
        block.left_brace.end.offset as usize,
        block.right_brace.start.offset as usize,
    );
    list(
        tokens,
        block.statements.iter(),
        Frame::Braces { open, close },
        Span::new(open, close),
    )
}

pub(crate) fn body_list<'ast, 'arena>(
    tokens: &[Token],
    statement: &'ast Statement<'arena>,
) -> List<'ast, 'arena> {
    match statement {
        Statement::Block(block) => block_list(tokens, block),
        _ => list(tokens, [statement], Frame::Bare, node_span(statement)),
    }
}

pub(crate) fn open_list<'ast, 'arena>(
    tokens: &[Token],
    statements: &'ast Sequence<'arena, Statement<'arena>>,
    from: usize,
    to: usize,
) -> List<'ast, 'arena> {
    list(tokens, statements.iter(), Frame::Open, Span::new(from, to))
}

pub(crate) fn holder_list<'ast, 'arena>(
    source: &str,
    tokens: &[Token],
    holder: Node<'ast, 'arena>,
) -> List<'ast, 'arena> {
    let open = |s: &'ast Sequence<'arena, Statement<'arena>>, from: usize, to: usize| {
        open_list(tokens, s, from, to)
    };
    let end = |s: mago_span::Span| s.start.offset as usize;
    let after = |s: mago_span::Span| s.end.offset as usize;
    match holder {
        Node::Block(block) => block_list(tokens, block),
        Node::Program(program) => open(&program.statements, 0, source.len()),
        Node::NamespaceImplicitBody(body) => {
            let to = body
                .statements
                .last()
                .map_or(after(body.span()), |s| after(s.span()));
            let to = tokens
                .iter()
                .find(|t| t.start >= to && !t.kind.is_trivia())
                .map_or(source.len(), |t| t.start);
            open(&body.statements, after(body.terminator.span()), to)
        }
        Node::SwitchExpressionCase(case) => {
            let from = after(case.separator.span());
            open(
                &case.statements,
                from,
                case_end(source, tokens, &case.statements, from),
            )
        }
        Node::SwitchDefaultCase(case) => {
            let from = after(case.separator.span());
            open(
                &case.statements,
                from,
                case_end(source, tokens, &case.statements, from),
            )
        }
        Node::IfColonDelimitedBody(body) => {
            let to = body
                .else_if_clauses
                .first()
                .map(|c| end(c.span()))
                .or_else(|| body.else_clause.as_ref().map(|c| end(c.span())))
                .unwrap_or(end(body.endif.span()));
            open(&body.statements, after(body.colon), to)
        }
        Node::IfColonDelimitedBodyElseIfClause(clause) => {
            let to = after(clause.span());
            open(&clause.statements, after(clause.colon), to)
        }
        Node::IfColonDelimitedBodyElseClause(clause) => open(
            &clause.statements,
            after(clause.colon),
            after(clause.span()),
        ),
        Node::WhileColonDelimitedBody(body) => open(
            &body.statements,
            after(body.colon),
            end(body.end_while.span()),
        ),
        Node::ForColonDelimitedBody(body) => open(
            &body.statements,
            after(body.colon),
            end(body.end_for.span()),
        ),
        Node::ForeachColonDelimitedBody(body) => open(
            &body.statements,
            after(body.colon),
            end(body.end_foreach.span()),
        ),
        Node::DeclareColonDelimitedBody(body) => open(
            &body.statements,
            after(body.colon),
            end(body.end_declare.span()),
        ),
        _ => body_list(tokens, statements(holder)[0]),
    }
}

fn case_end(
    source: &str,
    tokens: &[Token],
    statements: &Sequence<'_, Statement<'_>>,
    from: usize,
) -> usize {
    let after = statements.last().map_or(from, |s| node_span(s).end);
    tokens
        .iter()
        .find(|t| t.start >= after && !t.kind.is_trivia())
        .map_or(source.len(), |t| t.start)
}

pub(crate) fn enclosing<'ast, 'arena>(
    source: &str,
    tokens: &[Token],
    ancestors: &[Node<'ast, 'arena>],
) -> (List<'ast, 'arena>, usize) {
    let n = ancestors.len();
    let this = node_span(&ancestors[n - 1]);
    let list = holder_list(source, tokens, ancestors[n - 2]);
    let index = list
        .stmts
        .iter()
        .position(|s| node_span(*s) == this)
        .expect("a statement is in its parent's list");
    (list, index)
}

pub(crate) fn remove(
    source: &str,
    tokens: &[Token],
    list: &List<'_, '_>,
    indices: &[usize],
) -> Vec<Edit> {
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
            Frame::Open => {
                let first = node_span(list.stmts[0]).start;
                let before = previous_token_end(source, tokens, first).max(list.inner.start);
                let end = node_span(list.stmts[list.stmts.len() - 1]).end;
                return vec![Edit::new(Span::new(before, end), "")];
            }
        }
    }
    indices
        .iter()
        .map(|&i| {
            let span = node_span(list.stmts[i]);
            let before = previous_token_end(source, tokens, span.start).max(list.inner.start);
            if i > 0 {
                return Edit::new(Span::new(before, span.end), "");
            }
            let start = tokens
                .iter()
                .find(|t| t.start >= before && t.end <= span.start && t.kind.is_comment())
                .map_or(span.start, |t| t.start);
            let rest = &source[span.end..list.inner.end];
            let blanks = rest.len() - rest.trim_start().len();
            Edit::new(Span::new(start, span.end + blanks), "")
        })
        .collect()
}

pub(crate) fn moved_text(source: &str, tokens: &[Token], list: &List<'_, '_>, to: &str) -> String {
    let inner = &source[list.inner.start..list.inner.end];
    let start = list.inner.start + (inner.len() - inner.trim_start().len());
    let end = list.inner.start + inner.trim_end().len();
    let from = line_indent(source, start);
    let cut = if from.len() > to.len() && from.starts_with(to) {
        from.len() - to.len()
    } else {
        0
    };
    reindent(source, tokens, Span::new(start, end), cut)
}

pub(crate) fn statement_lines(
    source: &str,
    tokens: &[Token],
    list: &List<'_, '_>,
    to: &str,
) -> String {
    let mut spans: Vec<Span> = Vec::new();
    let mut previous = list.inner.start;
    let comment_after = |from: usize, to: usize| {
        tokens
            .iter()
            .find(|t| t.start >= from && t.end <= to && t.kind.is_comment())
            .map(|t| t.start)
    };
    for statement in &list.stmts {
        let span = node_span(*statement);
        let start = comment_after(previous, span.start).unwrap_or(span.start);
        spans.push(Span::new(start, span.end));
        previous = span.end;
    }
    if let Some(start) = comment_after(previous, list.inner.end) {
        let end = list.inner.start + source[list.inner.start..list.inner.end].trim_end().len();
        spans.push(Span::new(start, end));
    }
    spans
        .iter()
        .map(|span| {
            let from = line_indent(source, span.start);
            let cut = if from.len() > to.len() && from.starts_with(to) {
                from.len() - to.len()
            } else {
                0
            };
            reindent(source, tokens, *span, cut)
        })
        .collect::<Vec<_>>()
        .join(&format!("\n{to}"))
}

pub(crate) fn reindent(source: &str, tokens: &[Token], span: Span, cut: usize) -> String {
    let text = &source[span.start..span.end];
    if cut == 0 {
        return text.to_string();
    }
    let mut documents = Vec::new();
    let mut open = None;
    for t in tokens {
        match t.kind {
            TokenKind::DocumentStart(_) => open = Some(t.start),
            TokenKind::DocumentEnd => documents.extend(open.take().map(|s| (s, t.end))),
            _ => {}
        }
    }
    let protected = |pos: usize| {
        documents.iter().any(|&(s, e)| s < pos && pos <= e)
            || tokens
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
        if protected(offset) {
            continue;
        }
        let blanks = rest.len() - rest.trim_start_matches([' ', '\t']).len();
        let n = blanks.min(cut);
        rest = &rest[n..];
        offset += n;
    }
    out.push_str(rest);
    out
}

pub(crate) fn is_dynamic_expr(expr: &Expression<'_>) -> bool {
    let mut expr = unparenthesize(expr);
    if let Expression::UnaryPrefix(u) = expr
        && matches!(
            u.operator,
            UnaryPrefixOperator::Plus(_) | UnaryPrefixOperator::Negation(_)
        )
    {
        expr = unparenthesize(u.operand);
    }
    match expr {
        Expression::Array(a) => is_dynamic_array(&a.elements),
        Expression::LegacyArray(a) => is_dynamic_array(&a.elements),
        Expression::Literal(Literal::String(_) | Literal::Integer(_) | Literal::Float(_)) => false,
        Expression::MagicConstant(_) => false,
        Expression::CompositeString(CompositeString::Document(d)) => {
            d.parts.iter().any(|p| !matches!(p, StringPart::Literal(_)))
        }
        Expression::CompositeString(_) => true,
        Expression::Literal(_) | Expression::ConstantAccess(_) => false,
        Expression::Access(Access::ClassConstant(c)) => {
            let class_is_name = matches!(
                c.class,
                Expression::Identifier(_) | Expression::Self_(_) | Expression::Parent(_)
            );
            !(class_is_name && matches!(c.constant, ClassLikeConstantSelector::Identifier(_)))
        }
        _ => true,
    }
}

fn is_dynamic_array(elements: &TokenSeparatedSequence<'_, ArrayElement<'_>>) -> bool {
    elements.iter().filter_map(array_item).any(|(key, value)| {
        let key_allowed = key.is_none_or(|k| {
            matches!(
                unparenthesize(k),
                Expression::Literal(Literal::String(_) | Literal::Integer(_))
            )
        });
        !key_allowed || is_dynamic_expr(value)
    })
}

pub(crate) fn array_item<'arena>(
    element: &ArrayElement<'arena>,
) -> Option<(
    Option<&'arena Expression<'arena>>,
    &'arena Expression<'arena>,
)> {
    match element {
        ArrayElement::KeyValue(e) => Some((Some(e.key), e.value)),
        ArrayElement::Value(e) => Some((None, e.value)),
        ArrayElement::Variadic(e) => Some((None, e.value)),
        ArrayElement::Missing(_) => None,
    }
}

pub(crate) fn precedence(op: &BinaryOperator<'_>) -> u8 {
    match op {
        BinaryOperator::Exponentiation(_) => 20,
        BinaryOperator::Instanceof(_) => 18,
        BinaryOperator::Multiplication(_)
        | BinaryOperator::Division(_)
        | BinaryOperator::Modulo(_) => 16,
        BinaryOperator::Addition(_) | BinaryOperator::Subtraction(_) => 15,
        BinaryOperator::LeftShift(_) | BinaryOperator::RightShift(_) => 14,
        BinaryOperator::StringConcat(_) => 13,
        BinaryOperator::LessThan(_)
        | BinaryOperator::LessThanOrEqual(_)
        | BinaryOperator::GreaterThan(_)
        | BinaryOperator::GreaterThanOrEqual(_) => 12,
        BinaryOperator::Equal(_)
        | BinaryOperator::NotEqual(_)
        | BinaryOperator::Identical(_)
        | BinaryOperator::NotIdentical(_)
        | BinaryOperator::AngledNotEqual(_)
        | BinaryOperator::Spaceship(_) => 11,
        BinaryOperator::BitwiseAnd(_) => 10,
        BinaryOperator::BitwiseXor(_) => 9,
        BinaryOperator::BitwiseOr(_) => 8,
        BinaryOperator::And(_) => 7,
        BinaryOperator::Or(_) => 6,
        BinaryOperator::NullCoalesce(_) => 5,
        BinaryOperator::LowAnd(_) => 3,
        BinaryOperator::LowXor(_) => 2,
        BinaryOperator::LowOr(_) => 1,
    }
}

pub(crate) fn operand_precedence(expr: &Expression<'_>) -> u8 {
    match expr {
        Expression::Binary(b) => precedence(&b.operator),
        Expression::Conditional(_)
        | Expression::Assignment(_)
        | Expression::Yield(_)
        | Expression::Throw(_)
        | Expression::Pipe(_)
        | Expression::ArrowFunction(_) => 4,
        _ => u8::MAX,
    }
}

pub(crate) fn keeps_parentheses(inner: &Expression<'_>, parent: Option<Node<'_, '_>>) -> bool {
    let own = operand_precedence(inner);
    if own == u8::MAX {
        return false;
    }
    match parent {
        Some(
            Node::Parenthesized(_)
            | Node::ExpressionStatement(_)
            | Node::Return(_)
            | Node::PositionalArgument(_)
            | Node::NamedArgument(_)
            | Node::Echo(_),
        ) => false,
        Some(Node::Assignment(_)) => own < 4,
        Some(Node::Binary(b)) => own <= precedence(&b.operator),
        _ => true,
    }
}

pub(crate) fn bare_text<'s>(
    source: &'s str,
    expr: &Expression<'_>,
    parent: Option<Node<'_, '_>>,
) -> &'s str {
    let inner = unparenthesize(expr);
    let span = if matches!(expr, Expression::Parenthesized(_)) && keeps_parentheses(inner, parent) {
        node_span(expr)
    } else {
        node_span(inner)
    };
    &source[span.start..span.end]
}

pub(crate) fn parent_of<'ast, 'arena>(
    ancestors: &[Node<'ast, 'arena>],
) -> Option<Node<'ast, 'arena>> {
    ancestors
        .iter()
        .rev()
        .copied()
        .find(|n| !matches!(n, Node::Expression(_)))
}
