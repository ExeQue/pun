use crate::problem::{Edit, Safety, Span};
use mago_codex::metadata::function_like::FunctionLikeMetadata;
use mago_syntax::cst::{
    Block, Call, ClassLikeMemberSelector, Construct, Expression, FunctionLikeReturnTypeHint, Hint,
    Identifier, Method, Modifier, Node, Statement,
};

use super::facts::{Facts, facts, is_builtin_class, is_vendor_file};
use super::{ClassScope, MAGIC, class_scope, namespace_of};
use crate::rules::types::names::resolve_class_name;
use crate::syntax::node_span;
use crate::{Context, Report};

pub(crate) const NEVER_TYPE: u32 = 80100;

pub(crate) struct FunctionLike<'a, 'arena> {
    pub span: Span,
    pub at: usize,
    pub return_type: Option<&'a FunctionLikeReturnTypeHint<'arena>>,
    pub body: &'a Block<'arena>,
    pub method: Option<&'a Method<'arena>>,
}

pub(crate) fn add_never(
    f: FunctionLike<'_, '_>,
    ancestors: &[Node<'_, '_>],
    ctx: &Context<'_>,
) -> Option<Report> {
    let void = match f.return_type {
        None => false,
        Some(hint) if matches!(hint.hint, Hint::Void(_)) => true,
        Some(_) => return None,
    };
    if has_return_or_yields(f.body) {
        return None;
    }
    let scope = class_scope(ancestors);
    let facts = facts(ctx);
    let throws = super::any_scoped(
        Node::Block(f.body),
        &mut |n| matches!(n, Node::ExpressionStatement(s) if matches!(s.expression, Expression::Throw(_))),
    );
    if !throws
        && !f.body.statements.iter().any(|s| {
            matches!(s, Statement::Expression(e) if is_native_never(&facts, ancestors, scope.as_ref(), e.expression))
        })
    {
        return None;
    }
    if let Some(method) = f.method
        && should_skip_class_method(&facts, scope.as_ref(), method)
    {
        return None;
    }
    if void && !scope.as_ref().is_some_and(|s| s.is_final) {
        return None;
    }
    let edit = match f.return_type {
        Some(hint) => Edit::new(node_span(&hint.hint), "never".to_string()),
        None => Edit::new(Span::new(f.at, f.at), ": never".to_string()),
    };
    Some(Report::fixed(
        f.span,
        "Add the `never` return type.",
        Safety::Unsafe,
        vec![edit],
    ))
}

fn has_return_or_yields(body: &Block<'_>) -> bool {
    super::any_scoped(Node::Block(body), &mut |n| {
        matches!(
            n,
            Node::Return(_)
                | Node::Yield(_)
                | Node::If(_)
                | Node::While(_)
                | Node::DoWhile(_)
                | Node::TryCatchClause(_)
                | Node::Switch(_)
                | Node::Match(_)
                | Node::Foreach(_)
        )
    })
}

fn returns_never(f: Option<&FunctionLikeMetadata>) -> bool {
    f.and_then(|f| f.return_type_declaration_metadata.as_ref())
        .is_some_and(|t| t.type_union.is_never())
}

pub(crate) fn is_native_never(
    facts: &Facts,
    ancestors: &[Node<'_, '_>],
    scope: Option<&ClassScope>,
    expr: &Expression<'_>,
) -> bool {
    let codebase = &facts.codebase;
    let method_never = |class: Option<String>, selector: &ClassLikeMemberSelector<'_>| {
        let ClassLikeMemberSelector::Identifier(name) = selector else {
            return false;
        };
        class.is_some_and(|class| {
            returns_never(codebase.get_declaring_method(class.as_bytes(), name.value))
        })
    };
    match expr {
        Expression::Construct(Construct::Exit(_) | Construct::Die(_)) => true,
        Expression::Call(Call::Function(call)) => {
            let Expression::Identifier(name) = call.function else {
                return false;
            };
            function_names(ancestors, name)
                .into_iter()
                .find_map(|n| codebase.get_function(n.as_bytes()))
                .is_some_and(|f| returns_never(Some(f)))
        }
        Expression::Call(Call::Method(call)) => {
            matches!(call.object, Expression::Variable(v) if node_text_is_this(v))
                && method_never(scope.and_then(|s| s.name.clone()), &call.method)
        }
        Expression::Call(Call::StaticMethod(call)) => {
            let class = match call.class {
                Expression::Identifier(id) => Some(resolve_class_name(ancestors, id)),
                Expression::Self_(_) | Expression::Static(_) => scope.and_then(|s| s.name.clone()),
                Expression::Parent(_) => scope.and_then(|s| s.parent_class.clone()),
                _ => None,
            };
            method_never(class, &call.method)
        }
        _ => false,
    }
}

fn node_text_is_this(variable: &mago_syntax::cst::Variable<'_>) -> bool {
    matches!(variable, mago_syntax::cst::Variable::Direct(d) if d.name == b"$this")
}

fn function_names(ancestors: &[Node<'_, '_>], name: &Identifier<'_>) -> Vec<String> {
    let written = String::from_utf8_lossy(name.value()).into_owned();
    if let Some(fq) = written.strip_prefix('\\') {
        return vec![fq.to_string()];
    }
    match namespace_of(ancestors) {
        Some(ns) if written.contains('\\') => vec![format!("{ns}\\{written}")],
        Some(ns) => vec![format!("{ns}\\{written}"), written],
        None => vec![written],
    }
}

pub(crate) fn should_skip_class_method(
    facts: &Facts,
    scope: Option<&ClassScope>,
    method: &Method<'_>,
) -> bool {
    let name = method.name.value.to_ascii_lowercase();
    if name != b"__invoke" && MAGIC.iter().any(|m| m.as_bytes() == name) {
        return true;
    }
    if method
        .modifiers
        .iter()
        .any(|m| matches!(m, Modifier::Private(_)))
    {
        return false;
    }
    let Some(scope) = scope else {
        return true;
    };
    if scope
        .name
        .as_ref()
        .is_some_and(|n| facts.codebase.get_class_like(n.as_bytes()).is_none())
    {
        return true;
    }
    if scope.is_abstract || scope.is_interface {
        return true;
    }
    !is_return_type_change_allowed(facts, scope, &name)
}

fn is_return_type_change_allowed(facts: &Facts, scope: &ClassScope, method: &[u8]) -> bool {
    let Ok(parent) = parent_method(facts, scope, method) else {
        return false;
    };
    let Some((class, m)) = parent else {
        return true;
    };
    let declared = m
        .return_type_declaration_metadata
        .as_ref()
        .is_some_and(|t| !t.type_union.is_mixed());
    if declared || is_builtin_class(class.as_bytes()) {
        return false;
    }
    let vendor = |name: &str| {
        (facts.codebase.get_class_like(name.as_bytes()))
            .is_some_and(|c| is_vendor_file(c.span.file_id))
    };
    scope.name.as_deref().is_some_and(vendor) == vendor(&class)
}

fn parent_method<'f>(
    facts: &'f Facts,
    scope: &ClassScope,
    method: &[u8],
) -> Result<Option<(String, &'f FunctionLikeMetadata)>, ()> {
    let codebase = &facts.codebase;
    let class = scope
        .name
        .as_deref()
        .and_then(|n| codebase.get_class_like(n.as_bytes()));
    let mut parent = match class {
        Some(class) => class.direct_parent_class.as_ref().map(text),
        None => scope.parent_class.clone(),
    };
    while let Some(name) = parent {
        let Some(parent_class) = codebase.get_class_like(name.as_bytes()) else {
            return Err(());
        };
        let declaring = parent_class
            .declaring_method_ids
            .iter()
            .find(|(m, _)| m.as_bytes().eq_ignore_ascii_case(method))
            .map(|(_, id)| text(&id.get_class_name()));
        if let Some(m) =
            declaring.and_then(|c| codebase.get_method(c.as_bytes(), method).map(|m| (c, m)))
        {
            return Ok(Some(m));
        }
        parent = parent_class.direct_parent_class.as_ref().map(text);
    }
    let (interfaces, traits): (Vec<String>, Vec<String>) = match class {
        Some(class) => (
            class.all_parent_interfaces.iter().map(text).collect(),
            class.used_traits.iter().map(text).collect(),
        ),
        None => {
            let mut interfaces = Vec::new();
            for name in scope
                .parents
                .iter()
                .filter(|p| Some(*p) != scope.parent_class.as_ref())
            {
                let Some(interface) = codebase.get_class_like(name.as_bytes()) else {
                    return Err(());
                };
                interfaces.push(name.clone());
                interfaces.extend(interface.all_parent_interfaces.iter().map(text));
            }
            (interfaces, Vec::new())
        }
    };
    for name in interfaces {
        if let Some(m) = codebase.get_method(name.as_bytes(), method) {
            return Ok(Some((name, m)));
        }
    }
    for name in traits {
        if let Some(m) = codebase.get_method(name.as_bytes(), method) {
            let is_abstract = m.method_metadata.as_ref().is_some_and(|m| m.is_abstract);
            return Ok(is_abstract.then_some((name, m)));
        }
    }
    Ok(None)
}

pub(crate) fn is_test_class(facts: &Facts, scope: &ClassScope) -> bool {
    super::ancestor_names(facts, scope).iter().any(|a| {
        a.eq_ignore_ascii_case("PHPUnit\\Framework\\TestCase")
            || a.eq_ignore_ascii_case("PHPUnit_Framework_TestCase")
    })
}

fn text(word: &impl AsRef<[u8]>) -> String {
    String::from_utf8_lossy(word.as_ref()).into_owned()
}

pub(crate) fn has_parent_class_method(facts: &Facts, scope: &ClassScope, method: &[u8]) -> bool {
    parent_method(facts, scope, method).map_or(true, |m| m.is_some())
}
