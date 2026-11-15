use mago_codex::ttype::atomic::TAtomic;
use mago_syntax::cst::{Node, NodeKind, Program};
use pun_problem::{Edit, Safety, Span};
use serde_json::Value;

use super::{items, object};
use crate::rules::phpunit::annotations::Annotated;
use crate::rules::support::rector_names::attribute::{self, Attribute, AttributeConfig};
use crate::rules::support::rector_names::doc::{Doc, Value as DocValue};
use crate::rules::support::rector_names::resolve::{self, Scope, has_class};
use crate::rules::support::rector_names::view::{FileView, UseView};
use crate::rules::support::statement_edits::line_indent;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct AnnotationToAttribute {
    configs: Vec<AttributeConfig>,
}

impl Rule for AnnotationToAttribute {
    fn id(&self) -> &'static str {
        "lint/refactor/prefer-attribute-over-annotation"
    }
    fn once(&self) -> bool {
        true
    }

    fn configure(&mut self, options: &Value) -> Result<(), OptionsError> {
        self.configs.clear();
        for item in items(options)? {
            let map = object(
                item,
                &[
                    "tag",
                    "attribute-class",
                    "class-reference-fields",
                    "use-value-as-attribute-argument",
                ],
                &["tag"],
            )?;
            let class_name = |key: &str| -> Result<Option<String>, OptionsError> {
                match map.get(key) {
                    None | Some(Value::Null) => Ok(None),
                    Some(Value::String(name)) if is_class_name(name) => Ok(Some(name.clone())),
                    Some(Value::String(name)) => {
                        Err(format!("\"{name}\" is not a valid class name").into())
                    }
                    Some(_) => Err(format!("`{key}` must be a string").into()),
                }
            };
            let tag = class_name("tag")?.unwrap_or_default();
            let attribute_class = class_name("attribute-class")?.unwrap_or_else(|| tag.clone());
            let class_reference_fields = match map.get("class-reference-fields") {
                None => Vec::new(),
                Some(Value::Array(fields)) => fields
                    .iter()
                    .map(|f| f.as_str().map(str::to_string))
                    .collect::<Option<_>>()
                    .ok_or("`class-reference-fields` must be a list of strings")?,
                Some(_) => return Err("`class-reference-fields` must be a list of strings".into()),
            };
            let use_value_as_attribute_argument = match map.get("use-value-as-attribute-argument") {
                None => false,
                Some(value) => value
                    .as_bool()
                    .ok_or("`use-value-as-attribute-argument` must be a boolean")?,
            };
            self.configs.push(AttributeConfig {
                tag,
                attribute_class,
                class_reference_fields,
                use_value_as_attribute_argument,
            });
        }
        for config in self.configs.clone() {
            if config.attribute_class != config.tag {
                self.configs.push(AttributeConfig {
                    tag: config.attribute_class.clone(),
                    ..config
                });
            }
        }
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Program])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Program(program) = node else {
            return;
        };
        if self.configs.is_empty() || ctx.php_version.id() < 80000 {
            return;
        }
        let Some(edits) = self.fix(ctx, program, node) else {
            return;
        };
        for edit in edits {
            out.push(Report::fixed(
                edit.span,
                "Replace the annotation by an attribute.",
                Safety::Unsafe,
                vec![edit],
            ));
        }
    }
}

fn bare_uses(view: &FileView) -> Vec<(String, Option<String>, Span)> {
    if view.namespaces > 1 {
        return Vec::new();
    }
    view.uses_root()
        .uses
        .iter()
        .filter(|u| u.prefix.is_none())
        .flat_map(|u| {
            u.items
                .iter()
                .map(|i| (i.name.clone(), i.alias.clone(), i.name_span))
        })
        .collect()
}

impl AnnotationToAttribute {
    fn fix(
        &self,
        ctx: &Context<'_>,
        program: &Program<'_>,
        root: Node<'_, '_>,
    ) -> Option<Vec<Edit>> {
        let view = FileView::of(ctx.source, program);
        let uses = bare_uses(&view);
        let original: Vec<(String, Option<String>)> = uses
            .iter()
            .map(|(n, a, _)| (n.clone(), a.clone()))
            .collect();
        let mut renamed: Vec<Option<String>> = vec![None; uses.len()];
        let mut edits = Vec::new();
        let mut stack = vec![(root, Vec::new())];
        while let Some((node, ancestors)) = stack.pop() {
            let mut path = ancestors.clone();
            path.push(node);
            for child in node.children().into_iter().rev() {
                stack.push((child, path.clone()));
            }
            let lists = match node {
                Node::Closure(closure) => {
                    Some(closure.attribute_lists.last().map(|l| node_span(l).end))
                }
                Node::ArrowFunction(arrow) => {
                    Some(arrow.attribute_lists.last().map(|l| node_span(l).end))
                }
                _ => None,
            };
            if let Some(lists) = lists {
                let start = node_span(&node).start;
                if ancestors
                    .iter()
                    .rev()
                    .take_while(|a| node_span(*a).start == start)
                    .any(|a| !matches!(a, Node::Expression(_)))
                {
                    continue;
                }
                let Some(doc_comment) =
                    crate::rules::support::docblocks::Doc::of(ctx, &ancestors, start)
                else {
                    continue;
                };
                let Some((_, groups)) =
                    self.convert(ctx, &view, doc_comment.span, &original, &mut renamed)?
                else {
                    continue;
                };
                let printed = groups.iter().map(ToString::to_string);
                edits.push(match lists {
                    Some(end) => Edit::new(
                        Span::new(end, end),
                        printed.map(|g| format!(" {g}")).collect::<String>(),
                    ),
                    None => Edit::new(
                        Span::new(start, start),
                        printed.map(|g| format!("{g} ")).collect::<String>(),
                    ),
                });
                continue;
            }
            let Some(annotated) = Annotated::of_last(ctx, node, &ancestors) else {
                continue;
            };
            let Some(doc_comment) = &annotated.doc else {
                continue;
            };
            let Some((doc, groups)) =
                self.convert(ctx, &view, doc_comment.span, &original, &mut renamed)?
            else {
                continue;
            };
            let printed: Vec<String> = groups.iter().map(ToString::to_string).collect();
            let new_doc = doc.print();
            let width: usize = line_indent(ctx.source, annotated.start)
                .chars()
                .map(|c| if c == '\t' { 4 } else { 1 })
                .sum();
            let nl = format!("\n{}", " ".repeat(width));
            match annotated.edit_with_comments(ctx.source, &new_doc) {
                Some(mut edit) => match annotated.last_attribute_end {
                    Some(end) => {
                        edits.push(edit);
                        edits.push(Edit::new(
                            Span::new(end, end),
                            printed
                                .iter()
                                .map(|g| format!("{nl}{g}"))
                                .collect::<String>(),
                        ));
                    }
                    None => {
                        edit.text.extend(printed.iter().map(|g| format!("{g}{nl}")));
                        edits.push(edit);
                    }
                },
                None => {
                    edits.extend(annotated.edits_at(ctx.source, Some(&new_doc), &printed, &nl)?);
                }
            }
        }
        for ((name, _, span), new) in uses.iter().zip(renamed) {
            if let Some(new) = new.filter(|new| new != name) {
                edits.push(Edit::new(*span, new));
            }
        }
        Some(edits)
    }

    fn convert(
        &self,
        ctx: &Context<'_>,
        view: &FileView,
        span: Span,
        original: &[(String, Option<String>)],
        renamed: &mut [Option<String>],
    ) -> Option<Option<(Doc, Vec<Attribute>)>> {
        let doc_view = view.docs.iter().find(|d| d.span == span);
        let scope_uses: &[UseView] = &view.uses_root().uses;
        let scope = Scope {
            namespace: doc_view.and_then(|d| d.namespace.as_deref()),
            class: doc_view.and_then(|d| d.class.as_deref()),
            uses: scope_uses,
        };
        let mut doc = Doc::parse(
            &ctx.source[span.start..span.end],
            &|tag| resolve::resolve_tag(ctx, scope, tag),
            &|name| {
                resolve::narrow(ctx, scope, name)
                    .class_name()
                    .map(str::to_string)
            },
        )?;
        let mut groups = self.doctrine_annotations(ctx, &mut doc, original, renamed);
        groups.extend(self.simple_tags(ctx, &mut doc));
        Some((!groups.is_empty()).then_some((doc, groups)))
    }

    fn doctrine_annotations(
        &self,
        ctx: &Context<'_>,
        doc: &mut Doc,
        uses: &[(String, Option<String>)],
        renamed: &mut [Option<String>],
    ) -> Vec<Attribute> {
        let mut matched = Vec::new();
        for (i, child) in doc.children.iter().enumerate() {
            let DocValue::Annotation(annotation) = &child.value else {
                continue;
            };
            let identifier = annotation.identifier.trim_matches('@');
            let Some(config) = self
                .configs
                .iter()
                .find(|c| identifier == c.tag || annotation.class == c.tag)
            else {
                continue;
            };
            if config.use_value_as_attribute_argument
                || !is_attribute_class(ctx, &config.attribute_class)
            {
                continue;
            }
            matched.push((i, config, annotation.clone()));
        }
        let mut attributes = Vec::new();
        let mut renames = Vec::new();
        for (_, config, annotation) in &matched {
            let (name, rename) = attribute::attribute_name(config, &annotation.identifier, uses);
            renames.extend(rename);
            let constructor = constructor_parameters(ctx, &config.attribute_class);
            attributes.push(attribute::create(
                annotation,
                config,
                name,
                &constructor,
                foldable_parameters(ctx, &config.attribute_class).as_deref(),
                ctx.php_version.id() >= 80100,
            ));
        }
        for (i, new) in renames {
            renamed[i] = Some(new);
        }
        if attributes
            .iter()
            .any(|a| attribute::has_remove_array_state(a) || attribute::is_invalid(a))
        {
            return Vec::new();
        }
        for (i, _, annotation) in matched.iter().rev() {
            match attribute::trailing_comment(annotation.original()) {
                Some(comment) => doc.replace_with_text(*i, comment),
                None => doc.remove(*i),
            }
        }
        attributes
    }

    fn simple_tags(&self, ctx: &Context<'_>, doc: &mut Doc) -> Vec<Attribute> {
        let mut attributes = Vec::new();
        let mut i = 0;
        while i < doc.children.len() {
            let child = &doc.children[i];
            let (value, original) = match &child.value {
                DocValue::Generic(value) => (value.clone(), None),
                DocValue::Annotation(annotation) => (
                    annotation.values_string(),
                    Some(annotation.original().to_string()),
                ),
                _ => {
                    i += 1;
                    continue;
                }
            };
            let tag = child
                .tag
                .as_deref()
                .unwrap_or_default()
                .trim_matches('@')
                .to_string();
            let config = self
                .configs
                .iter()
                .filter(|c| c.tag.eq_ignore_ascii_case(&tag))
                .find(|c| has_class(ctx, &c.attribute_class));
            let Some(config) = config.filter(|_| !tag.contains('\\')) else {
                i += 1;
                continue;
            };
            let resolved = config
                .use_value_as_attribute_argument
                .then(|| attribute::resolve_value(&value, original.as_deref()));
            attributes.push(attribute::from_simple_tag(
                config,
                resolved.as_ref().map(|(v, _)| v.as_str()),
            ));
            match resolved {
                Some((_, comment)) if !comment.is_empty() => {
                    doc.replace_with_text(i, comment);
                    i += 1;
                }
                _ => doc.remove(i),
            }
        }
        attributes
    }
}

fn is_class_name(name: &str) -> bool {
    crate::rules::support::rector_names::doc::is_class_name(name)
}

fn is_attribute_class(ctx: &Context<'_>, class: &str) -> bool {
    has_class(ctx, class)
        && facts(ctx)
            .codebase
            .get_class_like(class.as_bytes())
            .is_some_and(|c| c.attribute_flags.is_some())
}

fn constructor_parameters(ctx: &Context<'_>, class: &str) -> Vec<(String, bool)> {
    let codebase = &facts(ctx).codebase;
    let Some(constructor) = codebase.get_method(class.as_bytes(), b"__construct") else {
        return Vec::new();
    };
    constructor
        .parameters
        .iter()
        .map(|p| {
            let name = p
                .name
                .0
                .as_str()
                .unwrap_or_default()
                .trim_start_matches('$')
                .to_string();
            let has_int = p.type_metadata.as_ref().is_some_and(|t| {
                t.type_union
                    .types
                    .iter()
                    .any(mago_codex::ttype::atomic::TAtomic::is_int)
            });
            (name, has_int)
        })
        .collect()
}

fn foldable_parameters(ctx: &Context<'_>, class: &str) -> Option<Vec<String>> {
    let facts = facts(ctx);
    let constructor = facts
        .codebase
        .get_method(class.as_bytes(), b"__construct")?;
    let mut names = Vec::new();
    let mut has_options = false;
    for p in &constructor.parameters {
        if p.flags.is_variadic() {
            return None;
        }
        let name = p
            .name
            .0
            .as_str()
            .unwrap_or_default()
            .trim_start_matches('$')
            .to_string();
        if name == "options" {
            has_options = p.type_metadata.as_ref().is_none_or(|t| {
                t.type_union.types.iter().any(|a| {
                    matches!(
                        a,
                        TAtomic::Array(_) | TAtomic::Iterable(_) | TAtomic::Mixed(_)
                    )
                })
            });
        }
        names.push(name);
    }
    has_options.then_some(names)
}
