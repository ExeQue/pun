use mago_codex::metadata::CodebaseMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::array::TArray;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{Expression, FunctionLikeParameter, Node};
use pun_problem::{Edit, Span};

use super::declare::{atomic_declaration, union_order};
use super::facts::Facts;
use crate::rules::class_method::type_of;

const NULL_FALSE_TRUE_STANDALONE_TYPE: u32 = 80200;
const UNION_TYPES: u32 = 80000;
const NULLABLE_TYPE: u32 = 70100;
const DNF_TYPES: u32 = 80200;

pub(crate) fn hint_at(param: &FunctionLikeParameter<'_>) -> usize {
    param
        .ampersand
        .or(param.ellipsis)
        .map_or(param.variable.span.start.offset, |s| s.start.offset) as usize
}

pub(crate) fn add_hint(param: &FunctionLikeParameter<'_>, declaration: &str) -> Edit {
    let at = hint_at(param);
    Edit::new(Span::new(at, at), format!("{declaration} "))
}

pub(crate) fn is_mixed(t: &TUnion) -> bool {
    t.types.is_empty() || t.types.iter().any(|a| matches!(a, TAtomic::Mixed(_)))
}

pub(crate) fn param_declaration(
    t: &TUnion,
    codebase: &CodebaseMetadata,
    php_version: u32,
) -> Option<String> {
    param_declaration_in(t, codebase, php_version, &[])
}

pub(crate) fn param_declaration_in(
    t: &TUnion,
    codebase: &CodebaseMetadata,
    php_version: u32,
    order: &[String],
) -> Option<String> {
    let expanded = expand(&t.types)?;
    declaration_of(
        normalize_bool(union_order(&expanded)),
        codebase,
        php_version,
        order,
    )
}

pub(crate) fn param_declaration_unsorted(
    types: &[TAtomic],
    codebase: &CodebaseMetadata,
    php_version: u32,
) -> Option<String> {
    let expanded = expand(types)?;
    declaration_of(
        normalize_bool(expanded.iter().collect()),
        codebase,
        php_version,
        &[],
    )
}

fn declaration_of(
    types: Vec<&TAtomic>,
    codebase: &CodebaseMetadata,
    php_version: u32,
    order: &[String],
) -> Option<String> {
    if let [one] = types.as_slice() {
        return single(one, codebase, php_version);
    }
    let mut names: Vec<String> = Vec::new();
    for t in &types {
        let name = match t {
            TAtomic::Null => "null".to_string(),
            TAtomic::Scalar(TScalar::Bool(b)) if b.is_false() => "false".to_string(),
            TAtomic::Scalar(TScalar::Bool(b)) if b.is_true() => {
                if php_version < NULL_FALSE_TRUE_STANDALONE_TYPE {
                    "bool".to_string()
                } else {
                    "true".to_string()
                }
            }
            TAtomic::Void | TAtomic::Never | TAtomic::Mixed(_) => return None,
            _ => member(t, codebase, php_version)?,
        };
        if name.contains('&') && php_version < DNF_TYPES {
            return None;
        }
        if !names.contains(&name) {
            names.push(name);
        }
    }
    order_classes(&mut names, order);
    match names.as_slice() {
        [] => None,
        [one] => Some(one.clone()),
        [a, b] if !a.contains('&') && !b.contains('&') && (a == "null" || b == "null") => {
            let other = if a == "null" { b } else { a };
            if php_version < NULLABLE_TYPE {
                return None;
            }
            if other == "false" && php_version < NULL_FALSE_TRUE_STANDALONE_TYPE {
                return Some("?bool".to_string());
            }
            Some(format!("?{other}"))
        }
        many => {
            if php_version < UNION_TYPES {
                return None;
            }
            let mut many: Vec<String> = many.iter().filter(|n| *n != "null").cloned().collect();
            if many.len() < names.len() {
                many.push("null".to_string());
            }
            let object = many.iter().any(|n| n == "object");
            let iterable = many.iter().any(|n| n == "iterable");
            many.retain(|n| {
                !(object
                    && (n.starts_with('\\') || n == "self" || n == "static" || n.contains('&')))
                    && !(iterable && n == "array")
            });
            let plain: Vec<String> = many
                .iter()
                .filter(|n| !n.contains('&'))
                .map(|n| n.to_ascii_lowercase())
                .collect();
            many.retain(|n| {
                !n.contains('&')
                    || !n
                        .split('&')
                        .any(|part| plain.contains(&part.to_ascii_lowercase()))
            });
            match many.as_slice() {
                [one] => return Some(one.clone()),
                [a, b]
                    if (a == "null") != (b == "null") && !a.contains('&') && !b.contains('&') =>
                {
                    let other = if a == "null" { b } else { a };
                    if other != "false" && other != "true" {
                        return Some(format!("?{other}"));
                    }
                }
                _ => {}
            }
            Some(
                many.iter()
                    .map(|n| {
                        if n.contains('&') {
                            format!("({n})")
                        } else {
                            n.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("|"),
            )
        }
    }
}

fn order_classes(names: &mut [String], order: &[String]) {
    let slots: Vec<usize> = (0..names.len())
        .filter(|&i| names[i].starts_with('\\'))
        .collect();
    let mut classes: Vec<String> = slots.iter().map(|&i| names[i].clone()).collect();
    classes.sort_by_key(|n| {
        let short = n.rsplit('\\').next().unwrap_or(n).to_ascii_lowercase();
        order
            .iter()
            .position(|o| *o == short)
            .unwrap_or(order.len())
    });
    for (slot, class) in slots.into_iter().zip(classes) {
        names[slot] = class;
    }
}

pub(crate) fn doc_order(ctx: &crate::Context<'_>, ancestors: &[Node<'_, '_>]) -> Vec<String> {
    let Some(at) = ancestors
        .iter()
        .rposition(|a| matches!(a, Node::Method(_) | Node::Function(_)))
    else {
        return Vec::new();
    };
    let start = crate::syntax::node_span(&ancestors[at]).start;
    let Some(doc) = crate::rules::lint_b_doc::Doc::of(ctx, &ancestors[..at], start) else {
        return Vec::new();
    };
    doc.text_all()
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '\\'))
        .filter(|w| !w.is_empty())
        .map(|w| w.rsplit('\\').next().unwrap_or(w).to_ascii_lowercase())
        .collect()
}

pub(crate) fn constant_values<'ast, 'arena>(
    facts: &Facts,
    ancestors: &[Node<'ast, 'arena>],
    expr: &'ast Expression<'arena>,
    values: &[TUnion],
) -> Option<Vec<TAtomic>> {
    use mago_syntax::cst::ArrayElement;
    let flat: Vec<TAtomic> = values
        .iter()
        .flat_map(|v| v.types.iter().cloned())
        .collect();
    let source = assigned(ancestors, expr);
    let expr = if std::ptr::eq(source, expr) {
        expr
    } else {
        match constant_values(facts, ancestors, source, values) {
            Some(types) if types.len() == flat.len() && types.iter().all(|t| flat.contains(t)) => {
                return Some(types);
            }
            _ => expr,
        }
    };
    let elements: Option<Vec<&ArrayElement<'_>>> = match crate::syntax::unparenthesize(expr) {
        Expression::Array(a) => Some(a.elements.iter().collect()),
        Expression::LegacyArray(a) => Some(a.elements.iter().collect()),
        _ => None,
    };
    let written: Option<Vec<&Expression<'_>>> = elements.and_then(|elements| {
        elements
            .iter()
            .map(|e| match e {
                ArrayElement::KeyValue(e) => Some(e.value),
                ArrayElement::Value(e) => Some(e.value),
                _ => None,
            })
            .collect()
    });
    if let Some(written) = written {
        let mut out = Vec::new();
        for value in written {
            out.extend(type_of(facts, value)?.types.iter().cloned());
        }
        return Some(out);
    }
    let string_keyed = type_of(facts, expr).is_some_and(|t| {
        t.types.iter().any(|a| {
            matches!(a, TAtomic::Array(TArray::Keyed(k))
                if k.known_items.iter().flat_map(|i| i.keys()).any(|key| !key.is_integer()))
        })
    });
    (!string_keyed || flat.iter().all(|t| *t == flat[0])).then_some(flat)
}

fn expand(types: &[TAtomic]) -> Option<Vec<TAtomic>> {
    let mut out = Vec::new();
    for t in types {
        match t {
            TAtomic::GenericParameter(p) => {
                if is_mixed(&p.constraint) {
                    return None;
                }
                out.extend(expand(&p.constraint.types)?);
            }
            TAtomic::Scalar(TScalar::ArrayKey) => {
                out.push(TAtomic::Scalar(TScalar::int()));
                out.push(TAtomic::Scalar(TScalar::string()));
            }
            TAtomic::Scalar(TScalar::Generic) => {
                out.push(TAtomic::Scalar(TScalar::int()));
                out.push(TAtomic::Scalar(TScalar::float()));
                out.push(TAtomic::Scalar(TScalar::string()));
                out.push(TAtomic::Scalar(TScalar::bool()));
            }
            _ => out.push(t.clone()),
        }
    }
    Some(out)
}

fn normalize_bool(types: Vec<&TAtomic>) -> Vec<&TAtomic> {
    let is_true = |t: &TAtomic| matches!(t, TAtomic::Scalar(TScalar::Bool(b)) if b.is_true());
    let is_false = |t: &TAtomic| matches!(t, TAtomic::Scalar(TScalar::Bool(b)) if b.is_false());
    let is_bool = |t: &TAtomic| matches!(t, TAtomic::Scalar(TScalar::Bool(b)) if b.is_general());
    let has_bool = types.iter().any(|t| is_bool(t))
        || (types.iter().any(|t| is_true(t)) && types.iter().any(|t| is_false(t)));
    if !has_bool {
        return types;
    }
    let mut out: Vec<&TAtomic> = Vec::new();
    let mut placed = false;
    for t in types {
        if is_true(t) || is_false(t) || is_bool(t) {
            if !placed {
                out.push(&BOOL);
                placed = true;
            }
            continue;
        }
        out.push(t);
    }
    out
}

static BOOL: TAtomic = TAtomic::Scalar(TScalar::bool());

fn single(t: &TAtomic, codebase: &CodebaseMetadata, php_version: u32) -> Option<String> {
    match t {
        TAtomic::Null if php_version >= NULL_FALSE_TRUE_STANDALONE_TYPE => Some("null".to_string()),
        TAtomic::Null | TAtomic::Void | TAtomic::Never | TAtomic::Mixed(_) => None,
        TAtomic::Scalar(TScalar::Bool(b)) if !b.is_general() => Some(str::to_string(
            if php_version < NULL_FALSE_TRUE_STANDALONE_TYPE {
                "bool"
            } else if b.is_true() {
                "true"
            } else {
                "false"
            },
        )),
        _ => member(t, codebase, php_version),
    }
}

fn member(t: &TAtomic, codebase: &CodebaseMetadata, php_version: u32) -> Option<String> {
    match t {
        TAtomic::Object(TObject::Named(named))
            if named.is_this && named.intersection_types.is_none() =>
        {
            Some("self".to_string())
        }
        TAtomic::Object(TObject::Named(named))
            if !named.is_this && !named.is_static && named.intersection_types.is_some() =>
        {
            let plain = |n: &mago_codex::ttype::atomic::object::named::TNamedObject| {
                let mut n = n.clone();
                n.intersection_types = None;
                n
            };
            let mut all = vec![plain(named)];
            for other in named.intersection_types.iter().flat_map(|o| o.iter()) {
                let TAtomic::Object(TObject::Named(other)) = other else {
                    return atomic_declaration(t, codebase, php_version);
                };
                all.push(plain(other));
            }
            let narrower = |n: &&mago_codex::ttype::atomic::object::named::TNamedObject| {
                !all.iter().any(|m| {
                    !m.name.as_bytes().eq_ignore_ascii_case(n.name.as_bytes())
                        && codebase.is_instance_of(m.name.as_bytes(), n.name.as_bytes())
                })
            };
            let kept: Vec<_> = all.iter().filter(narrower).cloned().collect();
            let (first, rest) = kept.split_first()?;
            let mut first = first.clone();
            first.intersection_types = (!rest.is_empty()).then(|| {
                Box::new(
                    rest.iter()
                        .map(|r| TAtomic::Object(TObject::Named(r.clone())))
                        .collect(),
                )
            });
            atomic_declaration(
                &TAtomic::Object(TObject::Named(first)),
                codebase,
                php_version,
            )
        }
        TAtomic::Object(TObject::Named(named)) if named.is_static => {
            let mut plain = named.clone();
            plain.is_static = false;
            atomic_declaration(
                &TAtomic::Object(TObject::Named(plain)),
                codebase,
                php_version,
            )
        }
        _ => atomic_declaration(t, codebase, php_version),
    }
}

pub(crate) fn own_static(t: &TAtomic, class: Option<&str>) -> TAtomic {
    match t {
        TAtomic::Object(TObject::Named(named)) if named.is_static || named.is_this => {
            let mut this = named.clone();
            this.is_this =
                class.is_some_and(|c| c.as_bytes().eq_ignore_ascii_case(named.name.as_bytes()));
            this.is_static = false;
            TAtomic::Object(TObject::Named(this))
        }
        other => other.clone(),
    }
}

pub(crate) enum ArrayShape {
    Constant(Vec<TUnion>),
    General(Option<TUnion>),
}

pub(crate) fn array_shape_of(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
) -> Option<ArrayShape> {
    if let Some(sliced) = head_slice(expr)
        && matches!(
            array_shape_of(facts, ancestors, sliced),
            Some(ArrayShape::General(None))
        )
    {
        return Some(ArrayShape::General(None));
    }
    let span = crate::syntax::node_span(crate::syntax::unparenthesize(expr));
    let literal = match crate::syntax::unparenthesize(expr) {
        Expression::Array(a) => a.elements.iter().collect(),
        Expression::LegacyArray(a) => a.elements.iter().collect(),
        _ => Vec::new(),
    };
    if facts.is_untyped(span)
        || literal
            .iter()
            .filter_map(|e| e.get_value())
            .any(|v| facts.is_untyped(crate::syntax::node_span(v)))
    {
        return None;
    }
    let t = type_of(facts, expr)?;
    let native_non_empty = facts.native_type(span).is_some_and(|n| {
        n.types
            .iter()
            .any(|a| matches!(a, TAtomic::Array(a) if a.is_non_empty()))
    });
    if cased_explode(ancestors, expr) {
        return Some(ArrayShape::General(Some(TUnion::from_vec(vec![
            TAtomic::Scalar(TScalar::string()),
        ]))));
    }
    if non_empty_call(expr) {
        let item = if let Some(ArrayShape::General(Some(item))) = array_shape(t, false) {
            accessory_string(&item)
        } else {
            None
        };
        return Some(ArrayShape::General(item));
    }
    array_shape(t, !native_non_empty || emptiness_checked(ancestors, expr))
}

fn emptiness_checked(ancestors: &[Node<'_, '_>], expr: &Expression<'_>) -> bool {
    use mago_syntax::cst::{UnaryPrefixOperator, Variable};
    let Expression::Variable(Variable::Direct(variable)) = crate::syntax::unparenthesize(expr)
    else {
        return false;
    };
    let Some(scope) = ancestors.iter().rev().find(|a| {
        matches!(
            a,
            Node::Function(_) | Node::Method(_) | Node::Closure(_) | Node::ArrowFunction(_)
        )
    }) else {
        return false;
    };
    let at = crate::syntax::node_span(expr).start;
    let is_it = |e: &Expression<'_>| matches!(crate::syntax::unparenthesize(e), Expression::Variable(Variable::Direct(v)) if v.name == variable.name);
    let mut checked = false;
    traverse(*scope, &mut |node| {
        checked |= crate::syntax::node_span(&node).end <= at
            && match node {
                Node::EmptyConstruct(e) => is_it(e.value),
                Node::UnaryPrefix(u) => {
                    matches!(u.operator, UnaryPrefixOperator::Not(_)) && is_it(u.operand)
                }
                _ => false,
            };
        Flow::Continue
    });
    checked
}

fn accessory_string(item: &TUnion) -> Option<TUnion> {
    use mago_codex::ttype::atomic::scalar::string::TStringCasing;
    let accessory = |a: &TAtomic| {
        matches!(a, TAtomic::Scalar(TScalar::String(s)) if s.literal.is_none()
            && (s.is_non_empty || s.is_truthy || s.is_numeric || s.casing != TStringCasing::Unspecified))
    };
    (!item.types.is_empty() && item.types.iter().all(accessory))
        .then(|| TUnion::from_vec(vec![TAtomic::Scalar(TScalar::string())]))
}

fn declared_items(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
) -> Option<Vec<String>> {
    use mago_syntax::cst::{Access, ClassLikeMemberSelector, Variable};
    let Expression::Access(Access::Property(fetch)) = crate::syntax::unparenthesize(expr) else {
        return None;
    };
    let (Expression::Variable(Variable::Direct(this)), ClassLikeMemberSelector::Identifier(name)) =
        (crate::syntax::unparenthesize(fetch.object), &fetch.property)
    else {
        return None;
    };
    if this.name != b"$this" {
        return None;
    }
    let class = super::class_scope(ancestors)?.name?;
    let property = facts
        .documented()
        .get_declaring_property(class.as_bytes(), &[b"$", name.value].concat())?;
    let span = property.type_metadata.as_ref()?.span;
    let source = facts.source_of(span.file_id)?;
    let text = source
        .get(span.start.offset as usize..span.end.offset as usize)?
        .trim();
    let item = match text.strip_suffix("[]") {
        Some(item) => item.trim().trim_start_matches('(').trim_end_matches(')'),
        None => {
            let lists = ["array", "list", "non-empty-array", "non-empty-list"];
            let (_, rest) = text
                .split_once('<')
                .filter(|(head, _)| lists.contains(&head.trim().to_ascii_lowercase().as_str()))?;
            top_level(rest.strip_suffix('>')?, ',').pop()?
        }
    };
    top_level(item, '|')
        .into_iter()
        .map(declared_member)
        .collect()
}

fn top_level(text: &str, separator: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (i, c) in text.char_indices() {
        match c {
            '<' | '(' | '{' | '[' => depth += 1,
            '>' | ')' | '}' | ']' => depth -= 1,
            _ if c == separator && depth == 0 => {
                parts.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts
}

fn declared_member(member: &str) -> Option<String> {
    let lower = member.to_ascii_lowercase();
    if lower.ends_with("[]")
        || ["array<", "list<", "non-empty-array<", "non-empty-list<"]
            .iter()
            .any(|p| lower.starts_with(p))
    {
        return Some("array".to_string());
    }
    if member.contains(['<', '(', '{', '&', ' ', '\'', '"']) {
        return None;
    }
    Some(
        match lower.as_str() {
            "integer" => "int",
            "boolean" => "bool",
            "double" => "float",
            "class-string" | "non-empty-string" => "string",
            "array" | "list" => "array",
            other => other.rsplit('\\').next().unwrap_or(other),
        }
        .to_string(),
    )
}

pub(crate) fn item_declaration(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
    item: &TUnion,
    php_version: u32,
) -> Option<String> {
    let declaration = param_declaration(item, &facts.codebase, php_version)?;
    let names: Vec<&str> = declaration.split('|').collect();
    let key = |n: &str| n.rsplit('\\').next().unwrap_or(n).to_ascii_lowercase();
    let Some(order) = declared_items(facts, ancestors, expr) else {
        return Some(declaration);
    };
    if names.len() < 2 {
        return Some(declaration);
    }
    let mut ordered: Vec<&str> = Vec::new();
    let rank = |o: &String| match o.as_str() {
        "array" => 1,
        "null" => 2,
        _ => 0,
    };
    let mut order = order;
    order.sort_by_key(rank);
    for o in &order {
        if let Some(n) = names.iter().find(|n| key(n) == *o)
            && !ordered.contains(n)
        {
            ordered.push(n);
        }
    }
    Some(if ordered.len() == names.len() {
        ordered.join("|")
    } else {
        declaration
    })
}

fn head_slice<'a>(expr: &'a Expression<'a>) -> Option<&'a Expression<'a>> {
    use crate::rules::lint_b_helpers::value;
    use crate::rules::rector_code_quality::named;
    use mago_syntax::cst::{Call, Literal};
    let Expression::Call(Call::Function(call)) = crate::syntax::unparenthesize(expr) else {
        return None;
    };
    let args: Vec<&Expression<'_>> = call.argument_list.arguments.iter().map(value).collect();
    let int = |i: usize| match args.get(i).map(|e| crate::syntax::unparenthesize(e)) {
        Some(Expression::Literal(Literal::Integer(i))) => i.value,
        _ => None,
    };
    (named(call, "array_slice")
        && int(1) == Some(0)
        && (args.len() == 2 || int(2).is_some_and(|n| n > 0)))
    .then(|| args[0])
}

fn code_scope<'ast, 'arena>(ancestors: &[Node<'ast, 'arena>]) -> Option<Node<'ast, 'arena>> {
    ancestors
        .iter()
        .rev()
        .find(|a| {
            matches!(
                a,
                Node::Function(_) | Node::Method(_) | Node::Closure(_) | Node::ArrowFunction(_)
            )
        })
        .or(ancestors.first())
        .copied()
}

fn assigned<'ast, 'arena>(
    ancestors: &[Node<'ast, 'arena>],
    expr: &'ast Expression<'arena>,
) -> &'ast Expression<'arena> {
    use mago_syntax::cst::{AssignmentOperator, Variable};
    let Expression::Variable(Variable::Direct(variable)) = crate::syntax::unparenthesize(expr)
    else {
        return expr;
    };
    let Some(scope) = code_scope(ancestors) else {
        return expr;
    };
    let at = crate::syntax::node_span(expr).start;
    let mut value = expr;
    traverse(scope, &mut |node| {
        if let Node::Assignment(a) = node
            && matches!(a.operator, AssignmentOperator::Assign(_))
            && crate::syntax::node_span(&node).end <= at
            && matches!(crate::syntax::unparenthesize(a.lhs), Expression::Variable(Variable::Direct(v)) if v.name == variable.name)
        {
            value = a.rhs;
        }
        Flow::Continue
    });
    value
}

fn cased_explode<'ast, 'arena>(
    ancestors: &[Node<'ast, 'arena>],
    expr: &'ast Expression<'arena>,
) -> bool {
    use crate::rules::lint_b_helpers::value;
    use crate::rules::rector_code_quality::named;
    use mago_syntax::cst::{Call, FunctionCall, Identifier, Variable};
    fn call_of<'b>(e: &'b Expression<'b>) -> Option<&'b FunctionCall<'b>> {
        match crate::syntax::unparenthesize(e) {
            Expression::Call(Call::Function(call)) => Some(call),
            _ => None,
        }
    }
    let Some(call) = call_of(assigned(ancestors, expr)).filter(|c| named(c, "explode")) else {
        return false;
    };
    let args: Vec<&Expression<'_>> = call.argument_list.arguments.iter().map(value).collect();
    let [_, haystack] = args.as_slice() else {
        return false;
    };
    let cased = call_of(assigned(ancestors, haystack)).is_some_and(|c| {
        ["strtolower", "mb_strtolower", "strtoupper", "mb_strtoupper"]
            .iter()
            .any(|n| named(c, n))
    });
    if !cased {
        return false;
    }
    let Expression::Variable(Variable::Direct(variable)) = crate::syntax::unparenthesize(haystack)
    else {
        return true;
    };
    let Some(scope) = code_scope(ancestors) else {
        return true;
    };
    let global = !ancestors.iter().any(|a| matches!(a, Node::Namespace(_)));
    let mut proven = false;
    traverse(scope, &mut |node| {
        if let Node::FunctionCall(c) = node
            && ["str_contains", "str_starts_with", "str_ends_with"].iter().any(|n| named(c, n))
            && (global || matches!(c.function, Expression::Identifier(Identifier::FullyQualified(_))))
            && c.argument_list.arguments.iter().next().is_some_and(|a| {
                matches!(crate::syntax::unparenthesize(value(a)), Expression::Variable(Variable::Direct(v)) if v.name == variable.name)
            })
        {
            proven = true;
        }
        Flow::Continue
    });
    !proven
}

fn non_empty_call(expr: &Expression<'_>) -> bool {
    use crate::rules::lint_b_helpers::value;
    use crate::rules::rector_code_quality::named;
    use mago_syntax::cst::{Call, Literal, UnaryPrefixOperator};
    let Expression::Call(Call::Function(call)) = crate::syntax::unparenthesize(expr) else {
        return false;
    };
    let args: Vec<&Expression<'_>> = call.argument_list.arguments.iter().map(value).collect();
    let int = |e: &&Expression<'_>| match crate::syntax::unparenthesize(e) {
        Expression::Literal(Literal::Integer(i)) => i.value,
        _ => None,
    };
    if named(call, "explode") {
        return !args.get(2).is_some_and(|limit| {
            matches!(
                crate::syntax::unparenthesize(limit),
                Expression::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Negation(_))
            )
        });
    }
    if named(call, "range") {
        let step = match args.get(2) {
            Some(step) => int(step),
            None => Some(1),
        };
        return match (args.first().and_then(int), args.get(1).and_then(int), step) {
            (Some(from), Some(to), Some(step)) if step > 0 => from.abs_diff(to) / step + 1 > 50,
            _ => true,
        };
    }
    false
}

fn array_shape(t: &TUnion, non_empty: bool) -> Option<ArrayShape> {
    let arrays: Vec<&TArray> = t
        .types
        .iter()
        .map(|a| match a {
            TAtomic::Array(array) => Some(array),
            _ => None,
        })
        .collect::<Option<_>>()?;
    if let [one] = arrays.as_slice() {
        return one_array_shape(one, non_empty);
    }
    if non_empty
        && arrays
            .iter()
            .all(|a| matches!(one_array_shape(a, true), Some(ArrayShape::General(None))))
    {
        return Some(ArrayShape::General(None));
    }
    let shapes: Vec<ArrayShape> = arrays
        .iter()
        .map(|a| one_array_shape(a, false))
        .collect::<Option<_>>()?;
    if shapes.iter().all(|s| matches!(s, ArrayShape::Constant(_))) {
        return None;
    }
    let mut items: Vec<TAtomic> = Vec::new();
    for shape in &shapes {
        let Some(item) = item_type(shape) else {
            continue;
        };
        for a in item.types.iter() {
            if !items.contains(a) {
                items.push(a.clone());
            }
        }
    }
    Some(ArrayShape::General(
        (!items.is_empty()).then(|| TUnion::from_vec(items)),
    ))
}

fn one_array_shape(array: &TArray, non_empty: bool) -> Option<ArrayShape> {
    Some(match array {
        TArray::List(list) => match &list.known_elements {
            Some(known) if list.element_type.is_never() => {
                ArrayShape::Constant(known.values().map(|(_, v)| v.clone()).collect())
            }
            Some(_) => return None,
            None if list.non_empty && non_empty => {
                ArrayShape::General(accessory_string(&list.element_type))
            }
            None => ArrayShape::General(Some((*list.element_type).clone())),
        },
        TArray::Keyed(keyed) => match (&keyed.known_items, &keyed.parameters) {
            (known, None) => ArrayShape::Constant(
                known
                    .iter()
                    .flat_map(|k| k.values())
                    .map(|(_, v)| v.clone())
                    .collect(),
            ),
            (Some(_), Some(_)) => return None,
            (None, Some((_, value))) if keyed.non_empty && non_empty => {
                ArrayShape::General(accessory_string(value))
            }
            (None, Some((_, value))) => ArrayShape::General(Some((**value).clone())),
        },
    })
}

pub(crate) fn item_type(shape: &ArrayShape) -> Option<TUnion> {
    match shape {
        ArrayShape::Constant(values) => {
            let mut types: Vec<TAtomic> = Vec::new();
            for v in values {
                for t in v.types.iter() {
                    if !types.contains(t) {
                        types.push(t.clone());
                    }
                }
            }
            (!types.is_empty()).then(|| TUnion::from_vec(types))
        }
        ArrayShape::General(value) => value.clone(),
    }
}

pub(crate) fn constant_values_union(types: &[TAtomic]) -> Vec<TAtomic> {
    let kind = |t: &TAtomic| match t {
        TAtomic::Null => Some(0),
        TAtomic::Scalar(s) if s.is_literal_value() => Some(match s {
            TScalar::Integer(_) => 1,
            TScalar::String(_) => 2,
            TScalar::Float(_) => 3,
            _ => 4,
        }),
        _ => None,
    };
    let mut out: Vec<TAtomic> = types
        .iter()
        .filter(|t| kind(t).is_none() && !matches!(t, TAtomic::Array(_)))
        .cloned()
        .collect();
    out.extend(
        types
            .iter()
            .filter(|t| matches!(t, TAtomic::Array(_)))
            .cloned(),
    );
    let mut kinds: Vec<u8> = Vec::new();
    for k in types.iter().filter_map(kind) {
        if !kinds.contains(&k) {
            kinds.push(k);
        }
    }
    for k in kinds {
        out.extend(types.iter().filter(|t| kind(t) == Some(k)).cloned());
    }
    out
}

pub(crate) enum Flow {
    Continue,
    Stop,
}

pub(crate) fn traverse<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    f: &mut dyn FnMut(Node<'ast, 'arena>) -> Flow,
) -> bool {
    let mut stopped = false;
    node.visit_children(|child| {
        if stopped {
            return;
        }
        match child {
            Node::Function(_)
            | Node::Closure(_)
            | Node::ArrowFunction(_)
            | Node::Method(_)
            | Node::Class(_)
            | Node::Interface(_)
            | Node::Trait(_)
            | Node::Enum(_) => {}
            Node::AnonymousClass(class) => {
                if let Some(args) = &class.argument_list {
                    stopped = traverse(Node::PartialArgumentList(args), f);
                }
            }
            _ => {
                stopped = matches!(f(child), Flow::Stop) || traverse(child, f);
            }
        }
    });
    stopped
}

pub(crate) fn phpstan_union(codebase: &CodebaseMetadata, args: Vec<Vec<TAtomic>>) -> Vec<TAtomic> {
    if let [one] = args.as_slice()
        && let [single] = one.as_slice()
        && !matches!(single, TAtomic::Array(_))
    {
        return one.clone();
    }
    let mut types: Vec<(usize, TAtomic)> = Vec::new();
    let mut array: Option<TAtomic> = None;
    let mut cases: Vec<TAtomic> = Vec::new();
    let mut null = false;
    for (group, arg) in args.into_iter().enumerate() {
        for atomic in arg {
            match atomic {
                TAtomic::Null => null = true,
                TAtomic::Array(_) => {
                    array.get_or_insert(atomic);
                }
                TAtomic::Object(TObject::Enum(ref e)) if e.case.is_some() => {
                    if !cases.contains(&atomic) {
                        cases.push(atomic);
                    }
                }
                other => types.push((group, other)),
            }
        }
    }
    let mut i = 0;
    while i < types.len() {
        let mut j = i + 1;
        let mut removed = false;
        while j < types.len() {
            if types[i].0 == types[j].0 {
                j += 1;
                continue;
            }
            if is_supertype(codebase, &types[j].1, &types[i].1) {
                types.remove(i);
                removed = true;
                break;
            }
            if is_supertype(codebase, &types[i].1, &types[j].1) {
                types.remove(j);
                continue;
            }
            j += 1;
        }
        if !removed {
            i += 1;
        }
    }
    let mut out: Vec<TAtomic> = types.into_iter().map(|(_, t)| t).collect();
    out.extend(array);
    out.extend(cases);
    if null {
        out.push(TAtomic::Null);
    }
    out
}

fn is_supertype(codebase: &CodebaseMetadata, a: &TAtomic, b: &TAtomic) -> bool {
    if a == b {
        return true;
    }
    let (TAtomic::Object(TObject::Named(a)), TAtomic::Object(TObject::Named(b))) = (a, b) else {
        return false;
    };
    a.name.as_bytes().eq_ignore_ascii_case(b.name.as_bytes())
        || codebase.get_class_like(b.name.as_bytes()).is_some_and(|c| {
            c.all_parent_classes
                .iter()
                .chain(c.all_parent_interfaces.iter())
                .any(|p| p.as_bytes().eq_ignore_ascii_case(a.name.as_bytes()))
        })
}
