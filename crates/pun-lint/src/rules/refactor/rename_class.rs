use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{Node, NodeKind};
use serde_json::Value;

use crate::rules::support::rector_names::phpdoc_parser::Val;
use crate::rules::support::rector_names::post::{Carry, Options, Session, doc_type};
use crate::rules::support::rector_names::resolve::{has_class, resolve_tag};
use crate::rules::types::facts::facts;
use crate::{Context, OptionsError, Report, Rule};

const TYPE_AWARE_ANNOTATIONS: &[&str] = &[
    JMS_TYPE,
    "Doctrine\\ORM\\Mapping\\OneToMany",
    "Symfony\\Component\\Validator\\Constraints\\Choice",
    "Symfony\\Component\\Validator\\Constraints\\Email",
    "Symfony\\Component\\Validator\\Constraints\\Range",
];
const JMS_TYPE: &str = "JMS\\Serializer\\Annotation\\Type";

#[derive(Default)]
pub struct RenameClass {
    renames: Vec<(String, String)>,
    options: Options,
}

impl Rule for RenameClass {
    fn id(&self) -> &'static str {
        "lint/refactor/rename-class"
    }

    fn once(&self) -> bool {
        true
    }

    fn configure(&mut self, options: &Value) -> Result<(), OptionsError> {
        self.renames.clear();
        self.options = Options {
            import_short_classes: true,
            ..Options::default()
        };
        let map = match options {
            Value::Null => return Ok(()),
            Value::Object(map) => map,
            _ => return Err("options must be a table".into()),
        };
        for (key, value) in map {
            let flag = match key.as_str() {
                "import-names" => &mut self.options.import_names,
                "import-short-classes" => &mut self.options.import_short_classes,
                "remove-unused-imports" => &mut self.options.remove_unused_imports,
                _ => {
                    let Some(new) = value.as_str() else {
                        return Err(format!("`{key}` must be a string").into());
                    };
                    self.renames.push((key.clone(), new.to_string()));
                    continue;
                }
            };
            *flag = value
                .as_bool()
                .ok_or_else(|| format!("`{key}` must be a boolean"))?;
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
        if self.renames.is_empty() {
            return;
        }
        let Some(fixed) = self.fix(ctx, program) else {
            return;
        };
        let source = ctx.source;
        let prefix: usize = source
            .chars()
            .zip(fixed.chars())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len_utf8())
            .sum();
        let suffix: usize = source[prefix..]
            .chars()
            .rev()
            .zip(fixed[prefix..].chars().rev())
            .take_while(|(a, b)| a == b)
            .map(|(a, _)| a.len_utf8())
            .sum();
        let (start, end, new_end) = (prefix, source.len() - suffix, fixed.len() - suffix);
        out.push(Report::fixed(
            Span::new(start, end),
            "Replace the renamed classes by their new names.",
            Safety::Unsafe,
            vec![Edit::new(
                Span::new(start, end),
                fixed[start..new_end].to_string(),
            )],
        ));
    }
}

impl RenameClass {
    fn fix(&self, ctx: &Context<'_>, program: &mago_syntax::cst::Program<'_>) -> Option<String> {
        let mut session = Session::new(ctx, program);
        self.rename_names(ctx, &mut session);
        self.rename_docs(ctx, &mut session);
        dedupe_implements(&mut session);
        let mut fixed = session.finish(self.options, true)?;
        let mut carry = session.carry();
        while fixed != ctx.source {
            let next = post_again(ctx, &fixed, self.options, &mut carry)?;
            if next == fixed {
                break;
            }
            fixed = next;
        }
        (fixed != ctx.source).then_some(fixed)
    }

    fn rename_names(&self, ctx: &Context<'_>, session: &mut Session<'_>) {
        let codebase = &facts(ctx).codebase;
        for i in 0..session.name_count() {
            let name = session.view.names[i].clone();
            if name.kind == crate::rules::support::rector_names::view::Kind::Function {
                continue;
            }
            let Some(old) = session.fully_qualified(i).map(str::to_string) else {
                continue;
            };
            if has_class(ctx, &old)
                && let Some(constant) = &name.class_const_fetch
            {
                let missing = self.renames.iter().any(|(from, to)| {
                    old.eq_ignore_ascii_case(from)
                        && has_class(ctx, to)
                        && codebase.class_constant_exists(from.as_bytes(), constant.as_bytes())
                        && !codebase.class_constant_exists(to.as_bytes(), constant.as_bytes())
                });
                if missing {
                    continue;
                }
            }
            let Some((_, new)) = self.renames.iter().find(|(from, _)| *from == old) else {
                continue;
            };
            if let Some(class) = codebase.get_class_like(new.as_bytes()) {
                let is_interface = class.kind.is_interface();
                if name.new && is_interface {
                    continue;
                }
                if !name.new && name.extends && name.class.as_deref() == Some(new.as_str()) {
                    continue;
                }
                if name.extends && (is_interface || class.flags.is_final()) {
                    continue;
                }
                if name.implements && class.kind.is_class() {
                    continue;
                }
                if name.static_call && is_interface {
                    continue;
                }
            }
            session.collect_renamed(&old);
            session.rename(i, new);
        }
    }

    fn rename_docs(&self, ctx: &Context<'_>, session: &mut Session<'_>) {
        for d in session.doc_indexes() {
            let Some(view) = session.doc_view(d) else {
                continue;
            };
            if !view.owners.renamed || !session.doc_is_last(d) {
                continue;
            }
            let Some(doc) = session.doc(d) else {
                session.unsupported();
                continue;
            };
            if !doc.has_type_aware() && doc.find_annotation(TYPE_AWARE_ANNOTATIONS).is_none() {
                continue;
            }
            let mut doc = doc.clone();
            let mut collected = Vec::new();
            let scope = session.doc_scope(d);
            for ident in doc.idents() {
                let Some(class) = doc_type(ctx, scope, &ident.name)
                    .class_name()
                    .map(str::to_string)
                else {
                    continue;
                };
                if let Some((old, new)) = self.renames.iter().find(|(old, _)| *old == class) {
                    doc.set_ident(&ident, &format!("\\{new}"));
                    collected.push(old.clone());
                }
            }
            for class in [
                "Symfony\\Component\\Validator\\Constraints\\Choice",
                "Doctrine\\ORM\\Mapping\\OneToMany",
                JMS_TYPE,
            ] {
                let classes: &[&str] = match class {
                    "Doctrine\\ORM\\Mapping\\OneToMany" => &[
                        "Doctrine\\ORM\\Mapping\\OneToMany",
                        "Doctrine\\ORM\\Mapping\\ManyToMany",
                        "Doctrine\\ORM\\Mapping\\Embedded",
                    ],
                    _ => &[class],
                };
                let Some((child, resolved, items)) = doc.annotation_mut(classes) else {
                    continue;
                };
                let embedded = resolved == "Doctrine\\ORM\\Mapping\\Embedded";
                let mut changed = false;
                match class {
                    "Symfony\\Component\\Validator\\Constraints\\Choice" => {
                        if let Some(item) = items.iter_mut().find(|i| i.key_is("callback"))
                            && let Val::Curly(values) = &mut item.value
                            && let Some(Val::StringNode(callback_class)) =
                                values.first_mut().map(|v| &mut v.value)
                            && let Some((old, new)) =
                                self.renames.iter().find(|(old, _)| old == callback_class)
                        {
                            collected.push(old.clone());
                            *callback_class = new.clone();
                            changed = true;
                        }
                    }
                    "Doctrine\\ORM\\Mapping\\OneToMany" => {
                        let key = if embedded { "class" } else { "targetEntity" };
                        if let Some(item) = items.iter_mut().find(|i| i.key_is(key))
                            && let Val::StringNode(target) = &item.value
                        {
                            let resolved = resolve_tag(ctx, scope, target);
                            for (old, new) in &self.renames {
                                if resolved == *old {
                                    collected.push(old.clone());
                                    item.value = Val::StringNode(new.clone());
                                    changed = true;
                                }
                            }
                        }
                    }
                    _ => {
                        for (old, new) in &self.renames {
                            if let Some(item) = items.iter_mut().find(|i| i.key.is_none())
                                && let Val::StringNode(value) = &item.value
                            {
                                if value == old {
                                    item.value = Val::StringNode(new.clone());
                                    continue;
                                }
                                collected.push(old.clone());
                                let replaced = replace_followed_by_b(value, old, new);
                                item.value = Val::StringNode(replaced);
                                changed = true;
                            }
                            if let Some(item) = items.iter_mut().find(|i| i.key_is("type"))
                                && let Val::StringNode(value) = &item.value
                                && value == old
                            {
                                item.value = Val::StringNode(new.clone());
                            }
                        }
                    }
                }
                if changed {
                    doc.set_changed(child);
                }
            }
            for old in collected {
                session.collect_renamed(&old);
            }
            if doc.is_changed() {
                *session.doc_mut(d).unwrap() = doc;
            }
        }
    }
}

fn post_again(
    ctx: &Context<'_>,
    source: &str,
    options: Options,
    carry: &mut Carry,
) -> Option<String> {
    let arena = mago_allocator::LocalArena::new();
    let file = mago_database::file::File::ephemeral(
        std::borrow::Cow::Borrowed(b"rename-class.php"),
        std::borrow::Cow::Owned(source.as_bytes().to_vec()),
    );
    let program = mago_syntax::parser::parse_file(&arena, &file);
    let next = Context::new(source, ctx.path, ctx.php_version);
    program.errors.is_empty().then_some(())?;
    let mut session = Session::new(&next, program);
    session.resume(std::mem::replace(carry, session.carry()));
    let fixed = session.finish(options, true);
    *carry = session.carry();
    fixed
}

fn replace_followed_by_b(value: &str, old: &str, new: &str) -> String {
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let pattern = format!("{old}b");
    let mut out = String::new();
    let mut at = 0;
    while let Some(found) = value[at..].find(&pattern) {
        let start = at + found;
        let first = old.as_bytes().first().copied().unwrap_or(b' ');
        let before = start.checked_sub(1).map(|p| value.as_bytes()[p]);
        let boundary = word(first) != before.is_some_and(word);
        out.push_str(&value[at..start]);
        if boundary {
            out.push_str(new);
        } else {
            out.push_str(&pattern);
        }
        at = start + pattern.len();
    }
    out.push_str(&value[at..]);
    out
}

fn dedupe_implements(session: &mut Session<'_>) {
    let mut removed = Vec::new();
    for class in session.view.class_likes.iter().filter(|c| c.is_class) {
        let mut seen: Vec<String> = Vec::new();
        for (span, i) in class.implements.iter().filter_map(|(span, _)| {
            Some((
                span,
                session.view.names.iter().position(|n| n.span == *span)?,
            ))
        }) {
            let name = session.name_string(i);
            if seen.contains(&name) {
                removed.push(*span);
            } else {
                seen.push(name);
            }
        }
    }
    for span in removed {
        session.remove_implements(span);
    }
}
