use mago_syntax::cst::{
    Access, CompositeString, Expression, FunctionCall, Identifier, Literal, Node, NodeKind,
};
use pun_problem::{Edit, Safety};
use serde_json::Value;

use super::document_string::document_value;
use super::names::resolve_class_name;
use super::object_type::is_object_type;
use super::{is_name, member_name};
use crate::rules::support::expressions::named;
use crate::rules::types::facts::facts;
use crate::rules::types::namespace_of;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Clone, Debug, PartialEq)]
pub enum ContainerService {
    Name(String),
    Constant(String, String),
}

#[derive(Default)]
pub struct PreferClassContainerKey {
    pub items: Vec<(ContainerService, ContainerService)>,
}

impl Rule for PreferClassContainerKey {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-class-container-key"
    }

    fn configure(&mut self, options: &Value) -> Result<(), OptionsError> {
        let map = match options {
            Value::Null => return Ok(()),
            Value::Object(map) => map,
            _ => return Err("options must be a table".into()),
        };
        if let Some(key) = map.keys().find(|k| *k != "items") {
            return Err(format!("unknown option `{key}`").into());
        }
        let error = "`items` must be a list of tables with `old-service` and `new-service`";
        let items = map.get("items").and_then(Value::as_array).ok_or(error)?;
        let mut parsed = Vec::new();
        for item in items {
            let item = item.as_object().ok_or(error)?;
            if item
                .keys()
                .any(|k| k != "old-service" && k != "new-service")
            {
                return Err(error.into());
            }
            let service = |key: &str| item.get(key).and_then(service).ok_or(error);
            parsed.push((service("old-service")?, service("new-service")?));
        }
        self.items = parsed;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::MethodCall,
            NodeKind::StaticMethodCall,
            NodeKind::FunctionCall,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (valid, list) = match node {
            Node::MethodCall(c) => (
                is_container_method(&c.method)
                    && is_object_type(
                        ctx,
                        ancestors,
                        c.object,
                        "Illuminate\\Contracts\\Container\\Container",
                    ),
                &c.argument_list,
            ),
            Node::StaticMethodCall(c) => (
                is_container_method(&c.method)
                    && is_object_type(
                        ctx,
                        ancestors,
                        c.class,
                        "Illuminate\\Support\\Facades\\Application",
                    ),
                &c.argument_list,
            ),
            _ => {
                let Node::FunctionCall(f) = node else { return };
                (is_helper(ctx, ancestors, f), &f.argument_list)
            }
        };
        let Some(first) = list.arguments.first().filter(|_| valid) else {
            return;
        };
        let value = first.value();
        let Some(mut current) = written(ctx, ancestors, value) else {
            return;
        };
        let mut changed = false;
        for (old, new) in &self.items {
            let matches = match (&current, old) {
                (ContainerService::Constant(a, _), ContainerService::Constant(b, _)) => a == b,
                (ContainerService::Name(a), ContainerService::Name(b)) => a == b,
                _ => false,
            };
            if matches {
                current = new.clone();
                changed = true;
            }
        }
        if !changed {
            return;
        }
        let code = match &current {
            ContainerService::Name(s) => {
                format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'"))
            }
            ContainerService::Constant(class, constant) => format!("\\{class}::{constant}"),
        };
        out.push(Report::fixed(
            node_span(value),
            "Use the configured service container key.",
            Safety::Unsafe,
            vec![Edit::new(node_span(unparenthesize(value)), code)],
        ));
    }
}

fn service(v: &Value) -> Option<ContainerService> {
    if let Some(s) = v.as_str() {
        return Some(ContainerService::Name(s.to_string()));
    }
    let o = v.as_object()?;
    if o.keys()
        .any(|k| !matches!(k.as_str(), "class" | "name" | "attributes"))
    {
        return None;
    }
    let name = |key: &str| o.get(key)?.get("name")?.as_str().map(str::to_string);
    Some(ContainerService::Constant(name("class")?, name("name")?))
}

fn is_container_method(method: &mago_syntax::cst::ClassLikeMemberSelector<'_>) -> bool {
    member_name(method).is_some_and(|n| is_name(n, "make") || is_name(n, "get"))
}

fn is_helper(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], f: &FunctionCall<'_>) -> bool {
    ["app", "resolve"].iter().any(|name| {
        named(f, name)
            && !(matches!(f.function, Expression::Identifier(Identifier::Local(_)))
                && namespace_of(ancestors).is_some_and(|ns| {
                    facts(ctx)
                        .codebase
                        .function_exists(format!("{ns}\\{name}").as_bytes())
                }))
    })
}

fn written(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    value: &Expression<'_>,
) -> Option<ContainerService> {
    let bytes = match unparenthesize(value) {
        Expression::Literal(Literal::String(s)) => s.value?.to_vec(),
        Expression::CompositeString(CompositeString::Document(d)) => document_value(d)?,
        Expression::Access(Access::ClassConstant(c)) => {
            let class = match unparenthesize(c.class) {
                Expression::Identifier(id) => resolve_class_name(ancestors, id),
                e @ (Expression::Self_(_) | Expression::Static(_) | Expression::Parent(_)) => {
                    let span = node_span(e);
                    ctx.source[span.start..span.end].to_string()
                }
                _ => return None,
            };
            return Some(ContainerService::Constant(
                class,
                ctx.source[node_span(&c.constant).start..node_span(&c.constant).end].to_string(),
            ));
        }
        _ => return None,
    };
    String::from_utf8(bytes).ok().map(ContainerService::Name)
}
