use std::collections::HashSet;

use mago_codex::metadata::CodebaseMetadata;
use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_codex::ttype::TType;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_span::HasSpan;
use mago_syntax::cst::{
    Access, Argument, ClassLikeMemberSelector, Closure, Expression, Hint, Node, NodeKind,
};
use pun_problem::{Edit, Safety, Span};

use super::names::resolve_class_name;
use super::object_type::{is_object_type, object_class};
use super::print::set_param_type;
use super::{is_name, member_name};
use crate::rules::support::expressions::named;
use crate::rules::types::class_scope;
use crate::rules::types::declare::{expression_order, return_declaration};
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const TAPPABLE: &str = "Illuminate\\Support\\Traits\\Tappable";

pub struct AddTapClosureType;

impl Rule for AddTapClosureType {
    fn id(&self) -> &'static str {
        "lint/laravel/add-tap-closure-type"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall, NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (closure, value) = match node {
            Node::FunctionCall(call) => {
                let args: Vec<&Expression<'_>> = call
                    .argument_list
                    .arguments
                    .iter()
                    .map(Argument::value)
                    .collect();
                if !named(call, "tap") || args.len() < 2 {
                    return;
                }
                (args[1], args[0])
            }
            _ => {
                let Node::MethodCall(call) = node else { return };
                let args: Vec<&Expression<'_>> = call
                    .argument_list
                    .arguments
                    .iter()
                    .map(Argument::value)
                    .collect();
                if !member_name(&call.method).is_some_and(|n| is_name(n, "tap"))
                    || args.is_empty()
                    || !is_object_type(ctx, ancestors, call.object, TAPPABLE)
                {
                    return;
                }
                (args[0], call.object)
            }
        };
        let Expression::Closure(closure) = closure else {
            return;
        };
        if let Some(fix) = refactor_parameter(ctx, ancestors, closure, value) {
            out.push(Report::fixed(
                node_span(closure.parameter_list.parameters.first().unwrap()),
                "Type the tap closure's parameter with the tapped value's type.",
                Safety::Unsafe,
                vec![fix],
            ));
        }
    }
}

fn refactor_parameter(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    closure: &Closure<'_>,
    value: &Expression<'_>,
) -> Option<Edit> {
    let param = closure.parameter_list.parameters.first()?;
    let facts = facts(ctx);
    let own = class_scope(ancestors).and_then(|s| s.name);
    if let Some((declared, values)) = new_type(ancestors, &facts, value) {
        return parameter_edit(ctx, ancestors, param, own.as_deref(), declared, &values);
    }
    if undeclared_property(&facts, value) {
        return parameter_edit(
            ctx,
            ancestors,
            param,
            own.as_deref(),
            None,
            &[Member::Other],
        );
    }
    let value = unparenthesize(value);
    let t = facts.full_type(node_span(value))?;
    let unknown = |a: &TAtomic| match a {
        TAtomic::Object(TObject::Named(n)) => {
            facts.codebase.get_class_like(n.name.as_bytes()).is_none()
        }
        TAtomic::Mixed(_)
        | TAtomic::Reference(_)
        | TAtomic::Object(TObject::Any)
        | TAtomic::Never => true,
        _ => false,
    };
    if t.types.iter().any(unknown) {
        return None;
    }
    let ordered = expression_order(value, |e| facts.full_type(node_span(e)))?;
    let declared = declaration(&facts, ordered, ctx.php_version.id())?;
    let values = value_members(&facts, &t.types, own.as_deref());
    parameter_edit(ctx, ancestors, param, own.as_deref(), declared, &values)
}

fn undeclared_property(facts: &Facts, value: &Expression<'_>) -> bool {
    let Expression::Access(Access::Property(access)) = unparenthesize(value) else {
        return false;
    };
    let name = match &access.property {
        ClassLikeMemberSelector::Identifier(name) => name.value,
        ClassLikeMemberSelector::Expression(e) => return !constant_string(facts, e.expression),
        selector => return !constant_string(facts, selector),
    };
    let property = [b"$", name].concat();
    facts.full_type(node_span(access.object)).is_some_and(|t| {
        t.types.iter().all(|a| match a {
            TAtomic::Object(TObject::Named(n)) => {
                let codebase = facts.documented();
                declared_property(codebase, n.name.as_bytes(), &property, &mut HashSet::new())
                    == Some(false)
            }
            _ => false,
        })
    })
}

fn constant_string(facts: &Facts, name: &impl HasSpan) -> bool {
    facts.full_type(node_span(name)).is_some_and(|t| {
        t.types.iter().all(|a| matches!(a, TAtomic::Scalar(TScalar::String(s)) if s.get_known_literal_value().is_some()))
    })
}

fn declared_property(
    codebase: &CodebaseMetadata,
    class: &[u8],
    property: &[u8],
    seen: &mut HashSet<Vec<u8>>,
) -> Option<bool> {
    let metadata = codebase.get_class_like(class)?;
    (!metadata.kind.is_enum()).then_some(())?;
    if codebase.property_exists(class, property) {
        return Some(true);
    }
    let lineage = (metadata.all_parent_classes.iter())
        .map(|p| codebase.get_class_like(p.as_bytes()))
        .collect::<Option<Vec<_>>>()?;
    let lineage: Vec<_> = std::iter::once(metadata).chain(lineage).collect();
    if allows_dynamic_properties(codebase, &lineage)? {
        if codebase.magic_property_exists(class, property) {
            return Some(true);
        }
        let traits = lineage.iter().flat_map(|c| &c.used_traits);
        let mixins = (lineage.iter().map(|c| &c.mixins)).chain(
            traits
                .filter_map(|t| codebase.get_class_like(t.as_bytes()))
                .map(|t| &t.mixins),
        );
        for mixin in mixins.flatten() {
            if !seen.insert(mixin.type_union.get_id().as_bytes().to_vec()) {
                continue;
            }
            let mut every = true;
            for atomic in mixin.type_union.types.iter() {
                let class = object_class(atomic)?;
                every &= declared_property(codebase, class.as_bytes(), property, seen)?;
            }
            if every {
                return Some(true);
            }
        }
        if lineage.iter().any(|c| c.name.as_bytes() == b"stdclass") {
            return Some(false);
        }
    }
    for required in &metadata.require_extends {
        if declared_property(codebase, required.as_bytes(), property, seen)? {
            return Some(true);
        }
    }
    Some(false)
}

fn allows_dynamic_properties(
    codebase: &CodebaseMetadata,
    lineage: &[&ClassLikeMetadata],
) -> Option<bool> {
    let class = lineage[0].name.as_bytes();
    if ["__get", "__set", "__isset"]
        .iter()
        .any(|m| codebase.method_exists(class, m.as_bytes()))
    {
        return Some(true);
    }
    for required in &lineage[0].require_extends {
        let required = codebase.get_class_like(required.as_bytes())?;
        let parents = (required.all_parent_classes.iter())
            .map(|p| codebase.get_class_like(p.as_bytes()))
            .collect::<Option<Vec<_>>>()?;
        if allows_dynamic_properties(codebase, &[&[required][..], &parents].concat())? {
            return Some(true);
        }
    }
    Some(lineage.iter().any(|c| {
        c.name.as_bytes() == b"stdclass"
            || (c.attributes.iter()).any(|a| {
                a.name
                    .as_bytes()
                    .eq_ignore_ascii_case(b"AllowDynamicProperties")
            })
    }))
}

fn new_type(
    ancestors: &[Node<'_, '_>],
    facts: &Facts,
    value: &Expression<'_>,
) -> Option<(Option<String>, Vec<Member>)> {
    let Expression::Instantiation(new) = unparenthesize(value) else {
        return None;
    };
    let name = match new.class {
        Expression::Static(_) => {
            let values = vec![Member::Class {
                name: "static".to_string(),
                generic: false,
            }];
            return Some((None, values));
        }
        Expression::Identifier(id) => resolve_class_name(ancestors, id),
        Expression::Self_(_) => class_scope(ancestors)?.name?,
        Expression::Parent(_) => class_scope(ancestors)?.parent_class?,
        _ => return None,
    };
    facts.codebase.get_class_like(name.as_bytes())?;
    Some((
        Some(format!("\\{name}")),
        vec![Member::Class {
            name,
            generic: false,
        }],
    ))
}

fn parameter_edit(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    param: &mago_syntax::cst::FunctionLikeParameter<'_>,
    own: Option<&str>,
    declared: Option<String>,
    values: &[Member],
) -> Option<Edit> {
    if let Some(hint) = &param.hint {
        let members = hint_members(ancestors, hint, own)?;
        if equal(&members, values)? {
            return None;
        }
        let text = &ctx.source[node_span(hint).start..node_span(hint).end];
        if let Some(declared) = &declared
            && (text == declared
                || (text.eq_ignore_ascii_case(declared) && members.contains(&Member::Other)))
        {
            return None;
        }
    }
    match (declared, &param.hint) {
        (Some(declared), _) => Some(set_param_type(param, &declared)),
        (None, Some(hint)) => {
            let span = node_span(hint);
            let rest = &ctx.source[span.end..];
            let end = span.end + rest.len() - rest.trim_start().len();
            Some(Edit::new(Span::new(span.start, end), ""))
        }
        (None, None) => None,
    }
}

fn declaration(facts: &Facts, types: Vec<&TAtomic>, php_version: u32) -> Option<Option<String>> {
    if !types.is_empty() && types.iter().all(|a| matches!(a, TAtomic::Null)) && php_version >= 80200
    {
        return Some(Some("null".to_string()));
    }
    if types
        .iter()
        .all(|a| matches!(a, TAtomic::Null | TAtomic::Never | TAtomic::Void))
    {
        return Some(None);
    }
    let declared = return_declaration(types, &facts.codebase, php_version, &[], true)?;
    let self_named = |m: &str| match m {
        "static" => "self".to_string(),
        "?static" => "?self".to_string(),
        m => m.to_string(),
    };
    Some(Some(
        declared
            .split('|')
            .map(self_named)
            .collect::<Vec<_>>()
            .join("|"),
    ))
}

#[derive(Debug, PartialEq)]
enum Member {
    Class { name: String, generic: bool },
    Null,
    Scalar(&'static str),
    Bool(bool),
    Float,
    Other,
}

fn hint_members(
    ancestors: &[Node<'_, '_>],
    hint: &Hint<'_>,
    own: Option<&str>,
) -> Option<Vec<Member>> {
    let mut out = Vec::new();
    let mut stack = vec![hint];
    while let Some(h) = stack.pop() {
        let class = |name: String| Member::Class {
            name,
            generic: false,
        };
        out.push(match h {
            Hint::Parenthesized(p) => {
                stack.push(p.hint);
                continue;
            }
            Hint::Nullable(n) => {
                stack.push(n.hint);
                Member::Null
            }
            Hint::Union(u) => {
                stack.push(u.right);
                stack.push(u.left);
                continue;
            }
            Hint::Intersection(_) => return None,
            Hint::Identifier(id) => class(resolve_class_name(ancestors, id)),
            Hint::Self_(_) | Hint::Static(_) => class(own?.to_string()),
            Hint::Parent(_) => class(class_scope(ancestors)?.parent_class?),
            Hint::Null(_) => Member::Null,
            Hint::True(_) => Member::Bool(true),
            Hint::False(_) => Member::Bool(false),
            Hint::Integer(_) => Member::Scalar("int"),
            Hint::Float(_) => Member::Scalar("float"),
            Hint::String(_) => Member::Scalar("string"),
            Hint::Bool(_) => Member::Scalar("bool"),
            _ => Member::Other,
        });
    }
    Some(out)
}

fn value_members(facts: &Facts, types: &[TAtomic], own: Option<&str>) -> Vec<Member> {
    let class = |name: &str| {
        let known = facts.codebase.get_class_like(name.as_bytes());
        Member::Class {
            name: known.map_or_else(|| name.to_string(), |c| c.original_name.to_string()),
            generic: known.is_some_and(|c| !c.template_types.is_empty()),
        }
    };
    types
        .iter()
        .map(|a| match a {
            TAtomic::Null => Member::Null,
            TAtomic::Object(TObject::Named(n)) if n.intersection_types.is_none() => {
                match (n.is_this || n.is_static, own) {
                    (true, Some(own)) => Member::Class {
                        name: own.to_string(),
                        generic: false,
                    },
                    _ => class(&n.name.to_string()),
                }
            }
            TAtomic::Scalar(TScalar::Integer(i)) if i.is_unspecified() => Member::Scalar("int"),
            TAtomic::Scalar(TScalar::String(s)) if s.is_boring() => Member::Scalar("string"),
            TAtomic::Scalar(TScalar::Float(f)) if !f.is_literal() => Member::Scalar("float"),
            TAtomic::Scalar(TScalar::Float(_)) => Member::Float,
            TAtomic::Scalar(TScalar::Bool(b)) if b.is_true() => Member::Bool(true),
            TAtomic::Scalar(TScalar::Bool(b)) if b.is_false() => Member::Bool(false),
            TAtomic::Scalar(TScalar::Bool(_)) => Member::Scalar("bool"),
            _ => Member::Other,
        })
        .collect()
}

fn equal(hint: &[Member], value: &[Member]) -> Option<bool> {
    if let ([h], [v]) = (hint, value) {
        let boolean = |m: &Member| matches!(m, Member::Scalar("bool") | Member::Bool(_));
        let float = |m: &Member| matches!(m, Member::Scalar("float") | Member::Float);
        if (boolean(h) && boolean(v)) || (float(h) && float(v)) {
            return Some(true);
        }
    }
    let same = |h: &Member, v: &Member| match (h, v) {
        (Member::Class { name: a, .. }, Member::Class { name: b, generic }) if a == b => {
            if *generic { None } else { Some(true) }
        }
        (Member::Other, _) | (_, Member::Other) => Some(false),
        _ => Some(h == v),
    };
    if hint.len() != value.len() {
        return Some(false);
    }
    for h in hint {
        let mut found = false;
        for v in value {
            if same(h, v)? {
                found = true;
            }
        }
        if !found {
            return Some(false);
        }
    }
    Some(true)
}
