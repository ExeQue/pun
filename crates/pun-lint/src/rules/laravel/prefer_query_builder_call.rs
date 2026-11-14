use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_syntax::cst::{ClassLikeMemberSelector, Expression, Node, NodeKind, StaticMethodCall};
use pun_problem::{Edit, Safety};
use serde_json::Value;

use super::is_a;
use super::object_type::{name_class, object_class, special_class};
use super::print::kept_arguments;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

const MODEL: &str = "Illuminate\\Database\\Eloquent\\Model";
const BUILDERS: [&str; 2] = [
    "Illuminate\\Database\\Eloquent\\Builder",
    "Illuminate\\Database\\Query\\Builder",
];

#[derive(Default)]
pub struct PreferQueryBuilderCall {
    pub exclude_methods: Vec<String>,
}

impl Rule for PreferQueryBuilderCall {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-query-builder-call"
    }

    fn configure(&mut self, options: &Value) -> Result<(), OptionsError> {
        let map = match options {
            Value::Null => return Ok(()),
            Value::Object(map) => map,
            _ => return Err("options must be a table".into()),
        };
        let mut keys = map.keys();
        let (Some(key), None) = (keys.next(), keys.next()) else {
            return Err("takes `exclude-methods` or `items`".into());
        };
        if key != "exclude-methods" && key != "items" {
            return Err(format!("unknown option `{key}`").into());
        }
        let names = (map[key].as_array()).and_then(|a| {
            a.iter()
                .map(|v| v.as_str().map(str::to_string))
                .collect::<Option<Vec<_>>>()
        });
        self.exclude_methods = names.ok_or(format!("`{key}` must be a list of method names"))?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::StaticMethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::StaticMethodCall(
            call @ StaticMethodCall {
                method: ClassLikeMemberSelector::Identifier(method),
                ..
            },
        ) = node
        else {
            return;
        };
        let name = method.value;
        let method = String::from_utf8_lossy(name);
        if method == "query" || self.exclude_methods.iter().any(|m| *m == method) {
            return;
        }
        let Some(model) = classes(ctx, ancestors, call.class)
            .into_iter()
            .find(|c| facts(ctx).codebase.get_class_like(c.as_bytes()).is_some())
            .filter(|c| is_a(ctx, c, &[MODEL]))
        else {
            return;
        };
        if !is_magic_method(ctx, &model, &method) {
            return;
        }
        let source = ctx.source;
        let class = &source[node_span(call.class).start..node_span(call.class).end];
        let start = node_span(call).start;
        let args = kept_arguments(source, &call.argument_list, start).join(", ");
        out.push(Report::fixed(
            node_span(call),
            format!(
                "Call `{method}()` on `query()` instead of through the model's magic static call."
            ),
            Safety::Unsafe,
            vec![Edit::new(
                node_span(call),
                format!("{class}::query()->{method}({args})"),
            )],
        ));
    }
}

fn classes(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], class: &Expression<'_>) -> Vec<String> {
    match unparenthesize(class) {
        Expression::Identifier(id) => return name_class(ancestors, id).into_iter().collect(),
        e @ (Expression::Self_(_) | Expression::Static(_) | Expression::Parent(_)) => {
            return special_class(ancestors, e).into_iter().collect();
        }
        _ => {}
    }
    let facts = facts(ctx);
    let types = facts
        .full_type(node_span(class))
        .map_or(&[][..], |t| &t.types[..]);
    let class_strings = types.iter().all(|a| {
        matches!(a, TAtomic::Scalar(TScalar::ClassLikeString(_)))
            || matches!(a, TAtomic::Scalar(s) if s.get_known_literal_string_value().is_some_and(|v| facts.codebase.get_class_like(v).is_some()))
    });
    let mut names = Vec::new();
    for atomic in types {
        if class_strings {
            if let TAtomic::Scalar(TScalar::ClassLikeString(c)) = atomic {
                match c.literal_value() {
                    Some(v) => names.push(v.to_string()),
                    None => names.extend(c.constraint().map(object_names).unwrap_or_default()),
                }
            } else if let TAtomic::Scalar(s) = atomic
                && let Some(v) = s.get_known_literal_string_value()
            {
                names.push(String::from_utf8_lossy(v).into_owned());
            }
        } else {
            names.extend(object_names(atomic));
        }
    }
    names
}

fn object_names(atomic: &TAtomic) -> Vec<String> {
    let mut names: Vec<String> = object_class(atomic).into_iter().collect();
    if let TAtomic::Object(TObject::Named(n)) = atomic {
        names.extend((n.intersection_types.iter().flat_map(|i| i.iter())).flat_map(object_names));
    }
    names
}

fn is_magic_method(ctx: &Context<'_>, model: &str, method: &str) -> bool {
    let facts = facts(ctx);
    let codebase = &facts.codebase;
    let public_instance = |class: &str| {
        codebase
            .get_declaring_method(class.as_bytes(), method.as_bytes())
            .and_then(|m| m.method_metadata.as_ref())
            .is_some_and(|m| m.visibility.is_public() && !m.is_static)
    };
    if !codebase.method_exists(model.as_bytes(), method.as_bytes()) {
        let mut scope = String::from("scope");
        let mut chars = method.chars();
        if let Some(first) = chars.next() {
            scope.push(first.to_ascii_uppercase());
            scope.push_str(chars.as_str());
        }
        return codebase.method_exists(model.as_bytes(), scope.as_bytes())
            || BUILDERS.iter().any(|b| {
                codebase.method_exists(b.as_bytes(), method.as_bytes()) && public_instance(b)
            });
    }
    (codebase.get_declaring_method_class(model.as_bytes(), method.as_bytes())).is_some_and(|d| {
        let declaring = d.to_string();
        public_instance(&declaring) && is_a(ctx, &declaring, &BUILDERS)
    })
}
