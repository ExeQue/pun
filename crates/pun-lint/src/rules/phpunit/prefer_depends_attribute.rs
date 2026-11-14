use mago_syntax::cst::{ClassLikeMember, Method, Node, NodeKind};
use pun_problem::Safety;

use super::annotations::{Annotated, TagValue, map_item, named_tags};
use super::{class_body, is_in_test_class};
use crate::rules::types::facts::facts;
use crate::{Context, Report, Rule};

const DEPENDS_ATTRIBUTE: &str = "PHPUnit\\Framework\\Attributes\\Depends";

pub struct PreferDependsAttribute;

impl Rule for PreferDependsAttribute {
    fn id(&self) -> &'static str {
        "lint/phpunit/prefer-depends-attribute"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (members, _, _) = class_body(node).unwrap();
        let methods: Vec<&Method<'_>> = members
            .iter()
            .filter_map(|m| match m {
                ClassLikeMember::Method(method) => Some(method),
                _ => None,
            })
            .collect();
        if !ctx.source.contains("@depends") {
            return;
        }
        let mut scope = ancestors.to_vec();
        scope.push(node);
        if !is_in_test_class(ctx, &scope, None)
            || facts(ctx)
                .codebase
                .get_class_like(DEPENDS_ATTRIBUTE.as_bytes())
                .is_none()
        {
            return;
        }
        let mut edits = Vec::new();
        let mut span = None;
        for method in &methods {
            let annotated = Annotated::of(ctx, Node::Method(method), &scope).unwrap();
            let Some(doc) = annotated.doc.as_ref() else {
                continue;
            };
            let Some(tags) = named_tags(ctx, &scope, doc, "depends") else {
                continue;
            };
            let current = String::from_utf8_lossy(method.name.value);
            let mut removed = vec![false; doc.len()];
            let mut groups = Vec::new();
            for (i, value) in tags {
                let TagValue::Generic(value) = value else {
                    continue;
                };
                let Some((class, items)) = resolve(&methods, &current, &value) else {
                    continue;
                };
                let items: Vec<String> = items.iter().map(|i| map_item(i)).collect();
                groups.push(format!("#[\\{class}({})]", items.join(", ")));
                removed[i] = true;
            }
            if groups.is_empty() {
                continue;
            }
            span.get_or_insert(doc.span);
            let new_doc = doc.print(&removed);
            let Some(method_edits) = annotated.edits(ctx.source, Some(&new_doc), &groups) else {
                return;
            };
            edits.extend(method_edits);
        }
        let Some(span) = span else {
            return;
        };
        out.push(Report::fixed(
            span,
            "Use the `#[Depends*]` attributes instead of the `@depends` annotation.",
            Safety::Safe,
            edits,
        ));
    }
}

fn resolve(methods: &[&Method<'_>], current: &str, value: &str) -> Option<(String, Vec<String>)> {
    let has_method = |name: &str| {
        methods
            .iter()
            .any(|m| m.name.value.eq_ignore_ascii_case(name.as_bytes()))
    };
    if current != value && has_method(value) {
        return Some((DEPENDS_ATTRIBUTE.to_string(), vec![value.to_string()]));
    }
    if let Some(class) = value.strip_suffix("::class") {
        return Some((
            "PHPUnit\\Framework\\Attributes\\DependsOnClass".to_string(),
            vec![format!("{class}::class")],
        ));
    }
    if value.starts_with("clone ") {
        let cloned = value.split("clone ").nth(1).unwrap_or_default();
        if current != cloned && has_method(cloned) {
            return Some((
                "PHPUnit\\Framework\\Attributes\\DependsUsingDeepClone".to_string(),
                vec![cloned.to_string()],
            ));
        }
        return None;
    }
    let (class, method) = value.split_once("::")?;
    if class.is_empty() || method.is_empty() || method == "class" {
        return None;
    }
    Some((
        "PHPUnit\\Framework\\Attributes\\DependsExternal".to_string(),
        vec![format!("{class}::class"), method.to_string()],
    ))
}
