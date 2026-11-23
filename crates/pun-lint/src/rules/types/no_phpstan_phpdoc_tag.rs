use crate::problem::{Edit, Safety};
use mago_syntax::cst::{Node, NodeKind};

use crate::rules::support::docblocks::Doc;
use crate::rules::support::phpdoc_writer::doc_info::{PhpDocInfo, tag_name};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const PHPSTAN_TAG_TO_NATIVE_TAG: [(&str, &str); 3] = [
    ("@phpstan-var", "@var"),
    ("@phpstan-param", "@param"),
    ("@phpstan-return", "@return"),
];

pub struct NoPhpstanPhpdocTag;

impl Rule for NoPhpstanPhpdocTag {
    fn id(&self) -> &'static str {
        "lint/types/no-phpstan-phpdoc-tag"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::Property,
            NodeKind::FunctionLikeParameter,
            NodeKind::ClassLikeConstant,
            NodeKind::Method,
            NodeKind::Function,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        // @phpstan-* tags can only live in a docblock; skip nodes without one
        let Some(doc) = Doc::of(ctx, ancestors, node_span(&node).start) else {
            return;
        };
        let tags: Vec<Tag<'_>> = (0..doc.len()).map(|i| Tag::of(&doc, i)).collect();
        let mut param_names = Vec::new();
        let mut var_names = Vec::new();
        let mut has_return = false;
        let mut native_descriptions: Vec<(String, &str)> = Vec::new();
        for tag in &tags {
            let Some(name) = tag.name else { continue };
            // remember native tag descriptions, so they survive when the native tag is overruled
            if PHPSTAN_TAG_TO_NATIVE_TAG.iter().any(|(_, n)| *n == name) {
                if let Some(value) = &tag.value
                    && !value.description.is_empty()
                {
                    native_descriptions.push((value.key(name), value.description));
                }
                continue;
            }
            match (name, &tag.value) {
                ("@phpstan-param", Some(v)) => param_names.push(v.variable.clone()),
                ("@phpstan-var", Some(v)) => var_names.push(v.variable.clone()),
                ("@phpstan-return", Some(_)) => has_return = true,
                _ => {}
            }
        }
        if param_names.is_empty() && var_names.is_empty() && !has_return {
            return;
        }
        let text = doc.text_all();
        let mut renamed = text.to_string();
        let mut removed = vec![false; tags.len()];
        for (i, tag) in tags.iter().enumerate().rev() {
            let Some(name) = tag.name else { continue };
            // drop the weaker native tag that the @phpstan-* variant overrules
            let overruled = match (name, &tag.value) {
                ("@param", Some(v)) => param_names.contains(&v.variable),
                ("@var", Some(v)) => var_names.contains(&v.variable),
                ("@return", Some(_)) => has_return,
                _ => false,
            };
            if overruled {
                removed[i] = true;
                continue;
            }
            // promote the @phpstan-* tag to its native counterpart
            let Some((_, native)) = PHPSTAN_TAG_TO_NATIVE_TAG.iter().find(|(p, _)| *p == name)
            else {
                continue;
            };
            let child = doc.text(i);
            let from = child.as_ptr() as usize - text.as_ptr() as usize;
            let mut new = format!("{native}{}", &child[name.len()..]);
            // keep the description of the overruled native tag
            if let Some(value) = tag.value.as_ref().filter(|v| v.description.is_empty()) {
                let key = value.key(native);
                if let Some((_, description)) =
                    native_descriptions.iter().rev().find(|(k, _)| *k == key)
                {
                    new = format!("{} {description}", new.trim_end());
                }
            }
            renamed.replace_range(from..from + child.len(), &new);
        }
        let renamed_doc = Doc::parse(&renamed).unwrap();
        // drop a leading empty line left behind by a removed native tag
        for (i, removed) in removed.iter_mut().enumerate() {
            if *removed {
                continue;
            }
            if !renamed_doc.text(i).trim().is_empty() {
                break;
            }
            *removed = true;
        }
        out.push(Report::fixed(
            node_span(&node),
            "Merge the `@phpstan-*` tags into their native tags.",
            Safety::Unsafe,
            vec![Edit::new(doc.span, renamed_doc.print(&removed))],
        ));
    }
}

struct Tag<'s> {
    name: Option<&'s str>,
    value: Option<Value<'s>>,
}

/// The parts of a `@param`, `@var` or `@return` tag (or its `@phpstan-` form) the rule reads.
struct Value<'s> {
    variable: String,
    /// The description as written, its later lines included.
    description: &'s str,
}

impl Value<'_> {
    fn key(&self, native: &str) -> String {
        match native {
            "@return" => native.to_string(),
            _ => format!("{native} {}", self.variable),
        }
    }
}

impl<'s> Tag<'s> {
    fn of(doc: &Doc<'s>, i: usize) -> Tag<'s> {
        let words = doc.words(i);
        let Some(name) = words.first().and_then(|w| tag_name(w)) else {
            return Tag {
                name: None,
                value: None,
            };
        };
        let native = PHPSTAN_TAG_TO_NATIVE_TAG
            .iter()
            .find(|(p, n)| *p == name || *n == name)
            .map(|(_, n)| *n);
        let child = doc.text(i);
        let value = native.and_then(|native| {
            let as_native = format!("{native}{}", &child[name.len()..]);
            let info = PhpDocInfo::parse(&format!("/**\n * {as_native}\n */"));
            let typed = match native {
                "@param" => info.typed("@param").next(),
                "@var" => info.var_tag(),
                _ => info.return_tag(),
            }?;
            Some(Value {
                variable: typed.variable.clone(),
                description: raw_description(child, &typed.description),
            })
        });
        Tag {
            name: Some(name),
            value,
        }
    }
}

/// The source text of a tag's description, which starts where its first line ends with the parsed one.
fn raw_description<'s>(child: &'s str, description: &str) -> &'s str {
    if description.is_empty() {
        return "";
    }
    let first = description.lines().next().unwrap_or_default();
    let first_line = child.lines().next().unwrap_or_default();
    match first_line.rfind(first).filter(|_| !first.is_empty()) {
        Some(at) => &child[at..],
        None => child[first_line.len()..].trim_start(),
    }
}
