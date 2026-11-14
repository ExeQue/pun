use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_syntax::cst::{
    Access, AssignmentOperator, ClassLikeConstantSelector, Expression, Literal, Node, NodeKind,
    Statement, UnaryPrefixOperator, Variable,
};
use pun_problem::{Edit, Fix, Safety, Span};

use crate::rules::support::class_members::{qualified, type_of};
use crate::rules::support::conditions::{comments_between, indentation, reprint};
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::statements::{
    List, Token, enclosing, is_dynamic_expr, previous_token_end, tokens,
};
use crate::rules::support::variable_usage::same_name;
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferInlineArrayReturn;

impl Rule for PreferInlineArrayReturn {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-inline-array-return"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Statement])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Statement(Statement::Return(r#return)) = node else {
            return;
        };
        let Some(Expression::Variable(Variable::Direct(returned))) =
            r#return.value.map(unparenthesize)
        else {
            return;
        };
        if ancestors.is_empty() {
            return;
        }
        let source = ctx.source;
        let tokens = tokens(ctx);
        let mut chain = ancestors.to_vec();
        chain.push(node);
        let (list, index) = enclosing(source, tokens, &chain);
        let count = list.stmts.len();
        if count < 3 || index + 1 != count || list.nop {
            return;
        }
        let name = &returned.name[1..];
        let function_like = function_like(ancestors);
        match function_like {
            Some(FunctionLike::Parameters(params)) if params.iter().any(|p| same_name(name, p)) => {
                return;
            }
            _ => {}
        }
        let empty_assign = list.stmts[..count - 1]
            .iter()
            .position(|s| is_empty_assign(s, name));
        if !exclusive(&list, empty_assign, name)
            || (empty_assign.is_none() && function_like.is_none())
        {
            return;
        }
        let facts = facts(ctx);
        let Some(items) = resolve(&facts, ancestors, &list, empty_assign) else {
            return;
        };
        if items.is_empty() {
            return;
        }
        let first = node_span(list.stmts[0]);
        let before = previous_token_end(source, tokens, first.start).max(list.inner.start);
        let start = comments_between(tokens, before, first.start)
            .first()
            .map_or(first.start, |c| c.start);
        let level = indentation(source, first.start);
        let array = print(source, tokens, level, &items);
        let span = Span::new(start, node_span(list.stmts[count - 1]).end);
        out.push(Report {
            span: node_span(list.stmts[0]),
            message: "Return the array directly instead of filling a variable.".to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, format!("return {array};"))],
            }),
        });
    }
}

enum FunctionLike {
    Parameters(Vec<Vec<u8>>),
}

fn function_like(ancestors: &[Node<'_, '_>]) -> Option<FunctionLike> {
    let n = ancestors.len();
    if !matches!(ancestors.get(n.checked_sub(1)?), Some(Node::Block(_))) {
        return None;
    }
    let owner = ancestors[..n - 1]
        .iter()
        .rev()
        .find(|a| !matches!(a, Node::MethodBody(_)))?;
    let name = |p: &mago_syntax::cst::FunctionLikeParameter<'_>| p.variable.name[1..].to_vec();
    let mut names: Vec<Vec<u8>> = match owner {
        Node::Method(m) => m.parameter_list.parameters.iter().map(name).collect(),
        Node::Function(f) => f.parameter_list.parameters.iter().map(name).collect(),
        Node::Closure(c) => c.parameter_list.parameters.iter().map(name).collect(),
        _ => return None,
    };
    if let Node::Closure(c) = owner
        && let Some(clause) = &c.use_clause
    {
        names.extend(
            clause
                .variables
                .iter()
                .filter(|v| v.ampersand.is_some())
                .map(|v| v.variable.name[1..].to_vec()),
        );
    }
    Some(FunctionLike::Parameters(names))
}

fn assignment<'a>(
    statement: &'a Statement<'a>,
) -> Option<(&'a Expression<'a>, &'a Expression<'a>)> {
    let Statement::Expression(e) = statement else {
        return None;
    };
    let Expression::Assignment(a) = e.expression else {
        return None;
    };
    if !matches!(a.operator, AssignmentOperator::Assign(_))
        || matches!(a.rhs, Expression::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Reference(_)))
    {
        return None;
    }
    Some((a.lhs, a.rhs))
}

fn is_empty_assign(statement: &Statement<'_>, name: &[u8]) -> bool {
    let Some((Expression::Variable(Variable::Direct(v)), rhs)) = assignment(statement) else {
        return false;
    };
    same_name(name, &v.name[1..])
        && match rhs {
            Expression::Array(a) => a.elements.is_empty(),
            Expression::LegacyArray(a) => a.elements.is_empty(),
            _ => false,
        }
}

fn exclusive(list: &List<'_, '_>, empty_assign: Option<usize>, name: &[u8]) -> bool {
    let last = list.stmts.len() - 1;
    list.stmts[..last].iter().enumerate().all(|(k, s)| {
        let Some((lhs, rhs)) = assignment(s) else {
            return false;
        };
        if Some(k) == empty_assign {
            return true;
        }
        if let Expression::Instantiation(new) = rhs
            && new
                .argument_list
                .as_ref()
                .is_some_and(|l| !l.arguments.is_empty())
        {
            return false;
        }
        let mut var = lhs;
        if !matches!(var, Expression::ArrayAccess(_) | Expression::ArrayAppend(_)) {
            return false;
        }
        loop {
            match var {
                Expression::ArrayAccess(a) => var = a.array,
                Expression::ArrayAppend(a) => var = a.array,
                _ => break,
            }
        }
        matches!(var, Expression::Variable(Variable::Direct(v)) if same_name(name, &v.name[1..]))
    })
}

#[derive(Clone, PartialEq)]
enum Key {
    Int(i64),
    Str(String),
}

enum Value {
    Expr(Span),
    Map(Vec<(Key, Value)>),
}

fn resolve(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    list: &List<'_, '_>,
    empty_assign: Option<usize>,
) -> Option<Vec<(Key, Value)>> {
    let mut items: Vec<(Key, Value)> = Vec::new();
    let mut key = 0i64;
    for (k, statement) in list.stmts[..list.stmts.len() - 1].iter().enumerate() {
        if Some(k) == empty_assign {
            continue;
        }
        let (lhs, rhs) = assignment(statement)?;
        let mut dims = Vec::new();
        let mut var = lhs;
        loop {
            match var {
                Expression::ArrayAccess(a) => {
                    if is_dynamic_expr(a.index) {
                        return Some(Vec::new());
                    }
                    dims.push(key_value(facts, ancestors, a.index)?);
                    var = a.array;
                }
                Expression::ArrayAppend(a) => {
                    dims.push(Key::Int(key));
                    var = a.array;
                }
                _ => break,
            }
        }
        key += 1;
        dims.reverse();
        set(&mut items, &dims, Value::Expr(node_span(rhs)))?;
    }
    Some(items)
}

fn set(items: &mut Vec<(Key, Value)>, keys: &[Key], value: Value) -> Option<()> {
    let (first, rest) = keys.split_first()?;
    let at = match items.iter().position(|(k, _)| k == first) {
        Some(at) => at,
        None => {
            items.push((first.clone(), Value::Map(Vec::new())));
            items.len() - 1
        }
    };
    if rest.is_empty() {
        items[at].1 = value;
        return Some(());
    }
    match &mut items[at].1 {
        Value::Map(map) => set(map, rest, value),
        Value::Expr(_) => None,
    }
}

fn key_value(facts: &Facts, ancestors: &[Node<'_, '_>], index: &Expression<'_>) -> Option<Key> {
    let index = unparenthesize(index);
    match index {
        Expression::Literal(Literal::String(s)) => {
            Some(string_key(std::str::from_utf8(s.value?).ok()?))
        }
        Expression::Literal(Literal::Integer(i)) => Some(Key::Int(i64::try_from(i.value?).ok()?)),
        Expression::Literal(Literal::Float(f)) => float_key(f.value.0),
        Expression::Literal(Literal::True(_)) => Some(Key::Int(1)),
        Expression::Literal(Literal::False(_)) => Some(Key::Int(0)),
        Expression::Literal(Literal::Null(_)) => Some(Key::Str(String::new())),
        Expression::ConstantAccess(c) => {
            let name = c.name.value();
            let name = name.strip_prefix(b"\\").unwrap_or(name);
            if name.eq_ignore_ascii_case(b"true") {
                Some(Key::Int(1))
            } else if name.eq_ignore_ascii_case(b"false") {
                Some(Key::Int(0))
            } else if name.eq_ignore_ascii_case(b"null") {
                Some(Key::Str(String::new()))
            } else {
                None
            }
        }
        Expression::UnaryPrefix(u) => {
            let sign = match u.operator {
                UnaryPrefixOperator::Plus(_) => 1,
                UnaryPrefixOperator::Negation(_) => -1,
                _ => return None,
            };
            match unparenthesize(u.operand) {
                Expression::Literal(Literal::Integer(i)) => {
                    Some(Key::Int(sign * i64::try_from(i.value?).ok()?))
                }
                Expression::Literal(Literal::Float(f)) => float_key(sign as f64 * f.value.0),
                _ => None,
            }
        }
        Expression::Access(Access::ClassConstant(c)) => {
            let ClassLikeConstantSelector::Identifier(constant) = &c.constant else {
                return None;
            };
            let class = match c.class {
                Expression::Identifier(id) => resolve_class_name(ancestors, id),
                Expression::Self_(_) => this_class(ancestors)?,
                Expression::Parent(_) => {
                    let this = this_class(ancestors)?;
                    let parent = facts
                        .codebase
                        .get_class_like(this.as_bytes())?
                        .direct_parent_class?;
                    String::from_utf8_lossy(parent.as_bytes()).into_owned()
                }
                _ => return None,
            };
            if constant.value.eq_ignore_ascii_case(b"class") {
                return Some(Key::Str(class));
            }
            literal_key(facts, index).or_else(|| class_constant(facts, &class, constant.value))
        }
        _ => None,
    }
}

fn literal_key(facts: &Facts, expr: &Expression<'_>) -> Option<Key> {
    let t = type_of(facts, expr)?;
    let [atomic] = &t.types[..] else { return None };
    match atomic {
        TAtomic::Scalar(TScalar::Integer(i)) => Some(Key::Int(i.get_literal_value()?)),
        TAtomic::Scalar(s @ TScalar::String(_)) => Some(string_key(
            std::str::from_utf8(s.get_known_literal_string_value()?).ok()?,
        )),
        _ => None,
    }
}

fn class_constant(facts: &Facts, class: &str, name: &[u8]) -> Option<Key> {
    let metadata = facts.codebase.get_class_like(class.as_bytes())?;
    if metadata.kind.is_enum() && metadata.enum_cases.keys().any(|c| c.as_bytes() == name) {
        return None;
    }
    let owners = std::iter::once(class.as_bytes().to_vec())
        .chain(
            metadata
                .all_parent_classes
                .iter()
                .map(|c| c.as_bytes().to_vec()),
        )
        .chain(
            metadata
                .all_parent_interfaces
                .iter()
                .map(|c| c.as_bytes().to_vec()),
        );
    for owner in owners {
        if let Some(constant) = facts.codebase.get_class_constant(&owner, name) {
            return match constant
                .inferred_type
                .as_ref()
                .filter(|t| t.is_single())?
                .get_single()
            {
                TAtomic::Scalar(TScalar::Integer(i)) => Some(Key::Int(i.get_literal_value()?)),
                TAtomic::Scalar(s @ TScalar::String(_)) => Some(string_key(
                    std::str::from_utf8(s.get_known_literal_string_value()?).ok()?,
                )),
                _ => None,
            };
        }
    }
    None
}

fn this_class(ancestors: &[Node<'_, '_>]) -> Option<String> {
    ancestors
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, a)| match a {
            Node::Class(c) => Some(qualified(&ancestors[..i], c.name.value)),
            Node::Enum(e) => Some(qualified(&ancestors[..i], e.name.value)),
            Node::Interface(e) => Some(qualified(&ancestors[..i], e.name.value)),
            _ => None,
        })
}

fn string_key(s: &str) -> Key {
    let digits = s.strip_prefix('-').unwrap_or(s);
    let canonical = !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && (digits == "0" || !digits.starts_with('0'))
        && s != "-0";
    match s.parse::<i64>() {
        Ok(i) if canonical => Key::Int(i),
        _ => Key::Str(s.to_string()),
    }
}

fn float_key(f: f64) -> Option<Key> {
    (f.is_finite() && f.abs() < 9.2e18).then(|| Key::Int(f.trunc() as i64))
}

fn print(source: &str, tokens: &[Token], level: usize, items: &[(Key, Value)]) -> String {
    let printed: Vec<String> = items
        .iter()
        .enumerate()
        .map(|(position, (key, value))| {
            let value = match value {
                Value::Expr(span) => reprint(
                    source,
                    tokens,
                    *span,
                    level as isize - indentation(source, span.start) as isize,
                ),
                Value::Map(map) => print(source, tokens, level, map),
            };
            match key {
                Key::Int(i) if *i == position as i64 => value,
                Key::Int(i) => format!("{i} => {value}"),
                Key::Str(s) => format!("{} => {value}", single_quoted(s)),
            }
        })
        .collect();
    format!("[{}]", printed.join(", "))
}

fn single_quoted(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::from("'");
    for (i, c) in s.char_indices() {
        if c == '\'' || (c == '\\' && matches!(bytes.get(i + 1), None | Some(b'\'' | b'\\'))) {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('\'');
    out
}
