use mago_codex::ttype::atomic::TAtomic;
use mago_syntax::cst::{
    Access, ClassLikeMemberSelector, Construct, Expression, Node, NodeKind, UnaryPrefixOperator,
    Variable,
};
use pun_problem::{Edit, Fix, Safety};

use crate::rules::support::class_members::{object_class_names, qualified, type_of};
use crate::rules::support::expressions::{Prec, in_parent, text, wrap};
use crate::rules::types::facts::{Facts, facts, is_builtin_class};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferPropertyExists;

impl Rule for PreferPropertyExists {
    fn id(&self) -> &'static str {
        "lint/complexity/prefer-property-exists"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::IssetConstruct, NodeKind::UnaryPrefix])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (isset, negated) = match node {
            Node::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Not(_)) => {
                match unparenthesize(u.operand) {
                    Expression::Construct(Construct::Isset(isset)) => (isset, true),
                    _ => return,
                }
            }
            Node::IssetConstruct(isset) => {
                let above = ancestors.iter().rev().find(|a| {
                    !matches!(
                        a,
                        Node::Expression(_) | Node::Construct(_) | Node::Parenthesized(_)
                    )
                });
                if matches!(above, Some(Node::UnaryPrefix(u)) if matches!(u.operator, UnaryPrefixOperator::Not(_)))
                {
                    return;
                }
                (isset, false)
            }
            _ => return,
        };
        let facts = facts(ctx);
        let this_class = this_class(&facts, ancestors);
        let checks: Vec<(String, Prec)> = isset
            .values
            .iter()
            .filter_map(|value| check(ctx, &facts, value, negated, this_class.as_deref()))
            .collect();
        let Some((first, rest)) = checks.split_first() else {
            return;
        };
        let and = Prec::Infix(120, 121, 120);
        let (mut code, mut prec) = first.clone();
        for (next, next_prec) in rest {
            code = format!(
                "{} && {}",
                wrap(&code, prec, 121, Some(121)),
                wrap(next, *next_prec, 120, None)
            );
            prec = and;
        }
        let span = node_span(&node);
        out.push(Report {
            span,
            message:
                "Check the property against `null`, and its existence with `property_exists()`."
                    .to_string(),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits: vec![Edit::new(span, in_parent(span, ancestors, code, prec))],
            }),
        });
    }
}

fn check(
    ctx: &Context<'_>,
    facts: &Facts,
    value: &Expression<'_>,
    negated: bool,
    this_class: Option<&str>,
) -> Option<(String, Prec)> {
    let Expression::Access(Access::Property(fetch)) = value else {
        return None;
    };
    let ClassLikeMemberSelector::Identifier(name) = &fetch.property else {
        return None;
    };
    let name = String::from_utf8_lossy(name.value).into_owned();
    let object = unparenthesize(fetch.object);
    let this = matches!(object, Expression::Variable(Variable::Direct(v)) if v.name == b"$this");
    if matches!(object, Expression::ArrayAccess(_)) {
        return None;
    }
    let classes = if this {
        this_class.map(|c| vec![c.to_string()]).unwrap_or_default()
    } else {
        let names = type_of(facts, fetch.object)
            .map(object_class_names)
            .unwrap_or_default();
        names
            .iter()
            .filter(|n| {
                !names
                    .iter()
                    .any(|m| m != *n && facts.codebase.is_instance_of(n.as_bytes(), m.as_bytes()))
            })
            .cloned()
            .collect()
    };
    let [class] = &classes[..] else {
        return None;
    };
    if class == "stdClass" {
        return None;
    }
    let metadata = facts.codebase.get_class_like(class.as_bytes())?;
    if class.eq_ignore_ascii_case("SimpleXMLElement")
        || facts
            .codebase
            .is_instance_of(class.as_bytes(), b"SimpleXMLElement")
    {
        return None;
    }
    let property = format!("${name}");
    if skips_for_native_type(facts, class, &property, metadata.kind.is_enum()) {
        return None;
    }
    if facts.codebase.method_exists(class.as_bytes(), b"__isset") {
        return None;
    }
    let fetch_text = text(ctx, value);
    let null_check = |op: &str| format!("{fetch_text} {op} null");
    let has_instance_property = if metadata.kind.is_enum() {
        name == "name" || (name == "value" && metadata.enum_type.is_some())
    } else {
        facts
            .codebase
            .get_declaring_property(class.as_bytes(), property.as_bytes())
            .is_some_and(|p| !p.flags.is_static())
    };
    let compare = Prec::Infix(80, 80, 80);
    Some(
        if !has_instance_property || is_builtin_class(class.as_bytes()) {
            let exists = format!("property_exists({}, '{name}')", text(ctx, object));
            if negated {
                (
                    format!("!{exists} || {}", null_check("===")),
                    Prec::Infix(130, 131, 130),
                )
            } else {
                (
                    format!("{exists} && {}", null_check("!==")),
                    Prec::Infix(120, 121, 120),
                )
            }
        } else if negated {
            (null_check("==="), compare)
        } else {
            (null_check("!=="), compare)
        },
    )
}

fn skips_for_native_type(facts: &Facts, class: &str, property: &str, enum_: bool) -> bool {
    if enum_ {
        return false;
    }
    let Some(property) = facts
        .codebase
        .get_declaring_property(class.as_bytes(), property.as_bytes())
    else {
        return false;
    };
    let Some(native) = property.type_declaration_metadata.as_ref() else {
        return false;
    };
    let native = &native.type_union;
    if native.is_mixed() {
        return false;
    }
    if !native.types.iter().any(|t| matches!(t, TAtomic::Null)) {
        return true;
    }
    !(property.flags.has_default()
        && property
            .default_type_metadata
            .as_ref()
            .is_some_and(|d| d.type_union.is_null()))
}

fn this_class(facts: &Facts, ancestors: &[Node<'_, '_>]) -> Option<String> {
    for (i, a) in ancestors.iter().enumerate().rev() {
        match a {
            Node::Closure(c) if c.r#static.is_some() => return None,
            Node::ArrowFunction(f) if f.r#static.is_some() => return None,
            Node::Class(c) => return Some(qualified(&ancestors[..i], c.name.value)),
            Node::Enum(e) => return Some(qualified(&ancestors[..i], e.name.value)),
            Node::AnonymousClass(c) => {
                return Some(facts.anonymous_class_name(mago_span::HasSpan::span(c)));
            }
            Node::Trait(_) | Node::Interface(_) => return None,
            _ => {}
        }
    }
    None
}
