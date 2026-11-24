//! Rector 2.8.4's configured rules in no set that edit a method's arguments, a property's name or an attribute's
//! value: `ArgumentRemoverRector`, `RenamePropertyRector` and `AttributeKeyToClassConstFetchRector`.

use crate::problem::{Edit, Safety, Span};
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_syntax::cst::{
    Access, Argument, ClassLikeConstantSelector, ClassLikeMember, ClassLikeMemberSelector,
    Expression, Node, NodeKind, PartialArgument, Property, Variable,
};
use serde_json::Value as Json;

use super::{assert_class, assert_method, int, is_class, is_object_type, items, object, string};
use crate::rules::support::class_members::qualified;
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::rewrites::list_removal;
use crate::rules::support::values::{Value, value};
use crate::rules::types::class_scope;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

/// `ValueResolver::getValue()` on what these rules compare: a literal, a constant (its name, but `true`, `false` and
/// `null`), `X::class`, and a class constant (its value when Pun's codebase knows it, else `X::NAME`).
fn resolved_value(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
) -> Option<Value> {
    let Expression::Access(Access::ClassConstant(access)) = unparenthesize(expr) else {
        return value(expr).map(|v| match v {
            Value::Constant(name) => Value::Str(name),
            v => v,
        });
    };
    let Expression::Identifier(class) = access.class else {
        return None;
    };
    let class = resolve_class_name(ancestors, class);
    let class = class.trim_start_matches('\\');
    match &access.constant {
        ClassLikeConstantSelector::Identifier(name)
            if name.value.eq_ignore_ascii_case(b"class") =>
        {
            Some(Value::Str(class.to_string()))
        }
        ClassLikeConstantSelector::Identifier(name) => {
            let codebase = &facts(ctx).codebase;
            let known = codebase
                .get_class_constant(class.as_bytes(), name.value)
                .and_then(|c| c.inferred_type.as_ref())
                .filter(|t| t.is_single())
                .and_then(|t| match t.get_single() {
                    TAtomic::Scalar(TScalar::Integer(i)) => i.get_literal_value().map(Value::Int),
                    TAtomic::Scalar(TScalar::Float(f)) => f.get_literal_value().map(Value::Float),
                    TAtomic::Scalar(TScalar::String(s)) => s
                        .get_known_literal_value()
                        .map(|s| Value::Str(String::from_utf8_lossy(s).into_owned())),
                    TAtomic::Scalar(TScalar::Bool(b)) if b.is_true() => Some(Value::Bool(true)),
                    TAtomic::Scalar(TScalar::Bool(b)) if b.is_false() => Some(Value::Bool(false)),
                    TAtomic::Null => Some(Value::Null),
                    _ => None,
                });
            Some(known.unwrap_or_else(|| {
                Value::Str(format!("{class}::{}", String::from_utf8_lossy(name.value)))
            }))
        }
        _ => None,
    }
}

/// PHP's `in_array($value, $values, true)` on a configured JSON value.
fn same(value: &Value, json: &Json) -> bool {
    match (value, json) {
        (Value::Null, Json::Null) => true,
        (Value::Bool(a), Json::Bool(b)) => a == b,
        (Value::Int(a), Json::Number(b)) => b.as_i64() == Some(*a) && !b.is_f64(),
        (Value::Float(a), Json::Number(b)) => b.is_f64() && b.as_f64() == Some(*a),
        (Value::Str(a), Json::String(b)) => a == b,
        _ => false,
    }
}

/// An `ArgumentRemover`'s value: none (remove whatever is there), a `name` (only the argument or parameter so named),
/// or the values the argument must hold.
enum Match {
    Any,
    Name(String),
    Values(Vec<Json>),
}

/// `ArgumentRemoverRector`.
#[derive(Default)]
pub struct RemoveMethodArgument {
    /// (class, method, position, match)
    removals: Vec<(String, String, usize, Match)>,
}

impl Rule for RemoveMethodArgument {
    fn id(&self) -> &'static str {
        "lint/refactor/remove-method-argument"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut removals = Vec::new();
        for item in items(options)? {
            let map = object(
                item,
                &["class", "method", "position", "value"],
                &["class", "method", "position"],
            )?;
            let (class, method) = (string(map, "class")?, string(map, "method")?);
            assert_class(&class)?;
            assert_method(&method)?;
            let position = usize::try_from(int(map, "position")?)
                .map_err(|_| "`position` must not be negative")?;
            let matches = match map.get("value") {
                None | Some(Json::Null) => Match::Any,
                Some(Json::Object(m)) => match m.get("name").and_then(Json::as_str) {
                    Some(name) if m.len() == 1 => Match::Name(name.to_string()),
                    _ => return Err("a table `value` holds one key, `name`, a string".into()),
                },
                Some(Json::Array(values))
                    if values.iter().all(|v| !v.is_array() && !v.is_object()) =>
                {
                    Match::Values(values.clone())
                }
                Some(_) => {
                    return Err("`value` must be a list of values or a table with `name`".into());
                }
            };
            removals.push((class, method, position, matches));
        }
        self.removals = removals;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::MethodCall,
            NodeKind::StaticMethodCall,
            NodeKind::Method,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if self.removals.is_empty() {
            return;
        }
        let (name, list): (&[u8], Option<_>) = match node {
            Node::MethodCall(call) => match &call.method {
                ClassLikeMemberSelector::Identifier(m) => (m.value, Some(&call.argument_list)),
                _ => return,
            },
            Node::StaticMethodCall(call) => match &call.method {
                ClassLikeMemberSelector::Identifier(m) => (m.value, Some(&call.argument_list)),
                _ => return,
            },
            Node::Method(method) => (method.name.value, None),
            _ => return,
        };
        let is_type = |class: &str| match node {
            Node::MethodCall(call) => is_object_type(ctx, ancestors, call.object, class),
            Node::StaticMethodCall(call) => is_object_type(ctx, ancestors, call.class, class),
            _ => class_scope(ancestors)
                .and_then(|s| s.name)
                .is_some_and(|n| n.eq_ignore_ascii_case(class) || is_class(ctx, &n, class)),
        };
        let spans: Vec<Span> = match (node, list) {
            (_, Some(list)) => list.arguments.iter().map(node_span).collect(),
            (Node::Method(method), None) => method
                .parameter_list
                .parameters
                .iter()
                .map(node_span)
                .collect(),
            _ => return,
        };
        let mut removed = vec![false; spans.len()];
        for (class, method, position, matches) in &self.removals {
            if !method.as_bytes().eq_ignore_ascii_case(name)
                || *position >= spans.len()
                || !is_type(class.trim_start_matches('\\'))
            {
                continue;
            }
            let hit = match (matches, list, node) {
                (Match::Any, ..) => true,
                (Match::Name(wanted), Some(list), _) => matches!(
                    list.arguments.get(*position),
                    Some(Argument::Named(a)) if wanted.as_bytes().eq_ignore_ascii_case(a.name.value)
                ),
                (Match::Name(wanted), None, Node::Method(method)) => method
                    .parameter_list
                    .parameters
                    .get(*position)
                    .is_some_and(|p| {
                        p.variable
                            .name
                            .get(1..)
                            .is_some_and(|n| wanted.as_bytes().eq_ignore_ascii_case(n))
                    }),
                // only an argument's value can be matched
                (Match::Values(values), Some(list), _) => {
                    // Rector reads an expression it cannot evaluate as `null`; Pun matches only values it knows
                    list.arguments.get(*position).is_some_and(|arg| {
                        !matches!(arg, Argument::Positional(p) if p.ellipsis.is_some())
                            && resolved_value(ctx, ancestors, arg.value())
                                .is_some_and(|v| values.iter().any(|j| same(&v, j)))
                    })
                }
                _ => false,
            };
            if hit {
                removed[*position] = true;
            }
        }
        if !removed.contains(&true) {
            return;
        }
        let edits = list_removal(&spans, &removed).unwrap_or_else(|| {
            vec![Edit::new(
                Span::new(spans[0].start, spans[spans.len() - 1].end),
                "",
            )]
        });
        out.push(Report::fixed(
            node_span(&node),
            "Remove the configured argument.",
            Safety::Unsafe,
            edits,
        ));
    }
}

/// `RenamePropertyRector`.
#[derive(Default)]
pub struct RenameProperty {
    /// (type, old property, new property)
    renames: Vec<(String, String, String)>,
}

impl Rule for RenameProperty {
    fn id(&self) -> &'static str {
        "lint/refactor/rename-property"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut renames = Vec::new();
        for item in items(options)? {
            let keys = ["type", "old-property", "new-property"];
            let map = object(item, &keys, &keys)?;
            let (class, old, new) = (
                string(map, keys[0])?,
                string(map, keys[1])?,
                string(map, keys[2])?,
            );
            assert_class(&class)?;
            renames.push((class, old, new));
        }
        self.renames = renames;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::Class,
            NodeKind::Trait,
            NodeKind::Interface,
            NodeKind::Enum,
            NodeKind::PropertyAccess,
            NodeKind::StaticPropertyAccess,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if self.renames.is_empty() {
            return;
        }
        match node {
            Node::PropertyAccess(access) => {
                let ClassLikeMemberSelector::Identifier(name) = &access.property else {
                    return;
                };
                self.rename_fetch(
                    ctx,
                    ancestors,
                    access.object,
                    node_span(name),
                    name.value,
                    "",
                    out,
                );
            }
            Node::StaticPropertyAccess(access) => {
                let Variable::Direct(variable) = &access.property else {
                    return;
                };
                let Some(name) = variable.name.get(1..) else {
                    return;
                };
                self.rename_fetch(
                    ctx,
                    ancestors,
                    access.class,
                    node_span(variable),
                    name,
                    "$",
                    out,
                );
            }
            _ => self.rename_declaration(ctx, node, ancestors, out),
        }
    }
}

impl RenameProperty {
    /// `refactorPropertyFetch()`.
    #[expect(clippy::too_many_arguments, reason = "the two fetch kinds share it")]
    fn rename_fetch(
        &self,
        ctx: &Context<'_>,
        ancestors: &[Node<'_, '_>],
        object: &Expression<'_>,
        span: Span,
        name: &[u8],
        sigil: &str,
        out: &mut Vec<Report>,
    ) {
        let Some((_, _, new)) = self.renames.iter().find(|(class, old, _)| {
            old.as_bytes().eq_ignore_ascii_case(name)
                && is_object_type(ctx, ancestors, object, class.trim_start_matches('\\'))
        }) else {
            return;
        };
        out.push(Report::fixed(
            span,
            format!("Rename the property to `{new}`."),
            Safety::Unsafe,
            vec![Edit::new(span, format!("{sigil}{new}"))],
        ));
    }

    /// `renameProperty()`: the class-like's own declaration of the property, when the class-like is the configured
    /// type, a subtype of it or uses it as a trait, and declares no property of the new name.
    fn rename_declaration(
        &self,
        ctx: &Context<'_>,
        node: Node<'_, '_>,
        ancestors: &[Node<'_, '_>],
        out: &mut Vec<Report>,
    ) {
        let (name, members) = match node {
            Node::Class(c) => (c.name.value, &c.members),
            Node::Trait(t) => (t.name.value, &t.members),
            Node::Interface(i) => (i.name.value, &i.members),
            Node::Enum(e) => (e.name.value, &e.members),
            _ => return,
        };
        let class_name = qualified(ancestors, name);
        let codebase = &facts(ctx).codebase;
        // ClassLike::getProperty(): the first property statement that declares the name, case-sensitively
        let property = |wanted: &str| {
            members.iter().find_map(|m| match m {
                ClassLikeMember::Property(Property::Plain(p)) => p
                    .items
                    .iter()
                    .any(|i| i.variable().name.get(1..) == Some(wanted.as_bytes()))
                    .then_some(p),
                _ => None,
            })
        };
        for (class, old, new) in &self.renames {
            let class = class.trim_start_matches('\\');
            let Some(declaration) = property(old) else {
                continue;
            };
            let uses_trait = codebase
                .get_class_like(class_name.as_bytes())
                .is_some_and(|c| {
                    c.used_traits
                        .iter()
                        .any(|t| t.as_bytes().eq_ignore_ascii_case(class.as_bytes()))
                });
            if class_name != class && !is_class(ctx, &class_name, class) && !uses_trait {
                continue;
            }
            if property(new).is_some() {
                continue;
            }
            // Rector renames the statement's first property, whichever declares the name; Pun only when it is that one
            let Some(first) = declaration.items.first() else {
                continue;
            };
            let variable = first.variable();
            if variable.name.get(1..) != Some(old.as_bytes()) {
                continue;
            }
            out.push(Report::fixed(
                node_span(variable),
                format!("Rename the property to `{new}`."),
                Safety::Unsafe,
                vec![Edit::new(node_span(variable), format!("${new}"))],
            ));
        }
    }
}

/// `AttributeKeyToClassConstFetchRector`.
#[derive(Default)]
pub struct ReplaceAttributeValueWithConstant {
    /// (attribute class, attribute key, constant class, value => constant)
    replacements: Vec<(String, String, String, Vec<(String, String)>)>,
}

impl Rule for ReplaceAttributeValueWithConstant {
    fn id(&self) -> &'static str {
        "lint/refactor/replace-attribute-value-with-constant"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut replacements = Vec::new();
        for item in items(options)? {
            let keys = [
                "attribute-class",
                "attribute-key",
                "constant-class",
                "values-to-constants-map",
            ];
            let map = object(item, &keys, &keys)?;
            let (attribute, key, constant) = (
                string(map, keys[0])?,
                string(map, keys[1])?,
                string(map, keys[2])?,
            );
            assert_class(&attribute)?;
            assert_class(&constant)?;
            let Some(values) = map[keys[3]].as_object() else {
                return Err("`values-to-constants-map` must be a table".into());
            };
            let values = values
                .iter()
                .map(|(k, v)| match v.as_str() {
                    Some(c) => Ok((k.clone(), c.to_string())),
                    None => Err(format!("`values-to-constants-map.{k}` must be a string")),
                })
                .collect::<Result<_, _>>()?;
            replacements.push((attribute, key, constant, values));
        }
        self.replacements = replacements;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Attribute])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Attribute(attribute) = node else {
            return;
        };
        // the attributes of a class, interface, property, parameter, method, function, closure or arrow function
        let owner = ancestors.iter().rev().nth(1);
        if !owner.is_some_and(|o| {
            matches!(
                o,
                Node::Class(_)
                    | Node::AnonymousClass(_)
                    | Node::Interface(_)
                    | Node::PlainProperty(_)
                    | Node::HookedProperty(_)
                    | Node::FunctionLikeParameter(_)
                    | Node::Method(_)
                    | Node::Function(_)
                    | Node::Closure(_)
                    | Node::ArrowFunction(_)
            )
        }) {
            return;
        }
        let Some(list) = &attribute.argument_list else {
            return;
        };
        let name = resolve_class_name(ancestors, &attribute.name);
        let name = name.trim_start_matches('\\');
        for (class, key, constant_class, values) in &self.replacements {
            if !class.trim_start_matches('\\').eq_ignore_ascii_case(name) {
                continue;
            }
            let constant_class = constant_class.trim_start_matches('\\');
            for arg in &list.arguments {
                let PartialArgument::Named(arg) = arg else {
                    continue;
                };
                if !key.as_bytes().eq_ignore_ascii_case(arg.name.value) {
                    continue;
                }
                let value = match resolved_value(ctx, ancestors, arg.value) {
                    Some(Value::Str(s)) => s,
                    Some(Value::Int(i)) => i.to_string(),
                    _ => continue,
                };
                let Some((_, constant)) = values.iter().find(|(v, _)| *v == value) else {
                    continue;
                };
                if let Expression::Access(Access::ClassConstant(access)) = arg.value
                    && let Expression::Identifier(written) = access.class
                    && let ClassLikeConstantSelector::Identifier(written_constant) =
                        &access.constant
                    && resolve_class_name(ancestors, written).trim_start_matches('\\')
                        == constant_class
                    && written_constant.value == constant.as_bytes()
                {
                    continue;
                }
                let span = node_span(arg.value);
                out.push(Report::fixed(
                    span,
                    format!(
                        "Use `{constant_class}::{constant}` for the attribute's `{key}` value."
                    ),
                    Safety::Unsafe,
                    vec![Edit::new(span, format!("\\{constant_class}::{constant}"))],
                ));
            }
        }
    }
}
