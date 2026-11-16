use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_codex::symbol::SymbolKind;
use mago_codex::visibility::Visibility;
use mago_word::word;
use pun_phpstan::symbols::{self, PhpVersion, Symbols};
use pun_phpstan::types::{Level, Type};

use super::{Checker, anonymous, params};

pub(crate) fn builtins(php_version: u32) -> &'static Symbols {
    symbols::symbols(PhpVersion::from_id(php_version))
}

impl Checker<'_, '_> {
    pub(super) fn display(&self, k: &ClassLikeMetadata) -> String {
        let name = k.original_name.as_bytes();
        if let Some(display) = self.anonymous.get(name) {
            return display.clone();
        }
        if name.starts_with(b"{anonymous-class:")
            && let Some(display) = self
                .anonymous_names
                .get(
                    k.span.file_id,
                    || {
                        let path = self.files.get(&k.span.file_id)?;
                        self.sources.get(path).cloned()
                    },
                    self.relative_path,
                )
                .and_then(|names| names.get(name).cloned())
        {
            return display;
        }
        String::from_utf8_lossy(name).into_owned()
    }

    pub(super) fn class_name(&self, k: &ClassLikeMetadata) -> String {
        let display = self.display(k);
        if !k.original_name.as_bytes().starts_with(b"{anonymous-class:") {
            return display;
        }
        let file = self
            .files
            .get(&k.span.file_id)
            .map(|p| crate::relative_path::absolute(p))
            .unwrap_or_default();
        anonymous::internal_name(&display, &file, self.relative_path)
    }

    pub(super) fn describe_object(&self, k: &ClassLikeMetadata) -> String {
        let display = self.display(k);
        if k.template_types.is_empty() {
            return display;
        }
        let args = super::declared::class_templates(self, k)
            .into_iter()
            .map(|t| Type::Template(Box::new(t)))
            .collect();
        Type::Object(display, args, false).describe(Level::TypeOnly)
    }
}

#[derive(Clone, Copy)]
pub(super) enum Known<'a> {
    User(&'a ClassLikeMetadata),
    Builtin(&'static symbols::Class),
}

pub(super) struct ClassConstant {
    pub visibility: Option<Visibility>,
    pub declaring: String,
    pub display: String,
}

impl Known<'_> {
    pub(super) fn constant(&self, c: &Checker, name: &str) -> Option<ClassConstant> {
        match self {
            Known::User(k) => {
                let visibility = if let Some(m) = k.constants.get(&word(name.as_bytes())) {
                    let declaring = std::iter::once(&k.name)
                        .chain(k.all_parent_classes.iter())
                        .filter_map(|n| c.codebase.get_class_like(n.as_bytes()))
                        .filter(|a| {
                            a.constants
                                .get(&word(name.as_bytes()))
                                .is_some_and(|v| v.span == m.span)
                        })
                        .last()
                        .unwrap_or(k);
                    return Some(ClassConstant {
                        visibility: Some(m.visibility),
                        declaring: declaring.original_name.to_string(),
                        display: c.display(declaring),
                    });
                } else if k.enum_cases.contains_key(&word(name.as_bytes())) {
                    None
                } else {
                    return None;
                };
                Some(ClassConstant {
                    visibility,
                    declaring: k.original_name.to_string(),
                    display: c.display(k),
                })
            }
            Known::Builtin(k) => {
                let has = |k: &symbols::Class| {
                    k.constants.iter().any(|(n, _)| n == name) || k.cases.iter().any(|n| n == name)
                };
                let declaring = std::iter::once(*k)
                    .chain(
                        k.ancestors
                            .iter()
                            .filter_map(|a| c.builtins.classes.get(&a.to_ascii_lowercase())),
                    )
                    .find(|k| has(k))?;
                Some(ClassConstant {
                    visibility: None,
                    declaring: declaring.name.clone(),
                    display: declaring.name.clone(),
                })
            }
        }
    }

    pub(super) fn is_trait(&self) -> bool {
        matches!(self, Known::User(k) if k.kind == SymbolKind::Trait)
    }

    pub(super) fn is_interface(&self) -> bool {
        match self {
            Known::User(k) => k.kind == SymbolKind::Interface,
            Known::Builtin(k) => k.is_interface,
        }
    }

    pub(super) fn is_enum(&self) -> bool {
        match self {
            Known::User(k) => k.kind == SymbolKind::Enum,
            Known::Builtin(k) => !k.cases.is_empty(),
        }
    }

    pub(super) fn is_final(&self) -> bool {
        match self {
            Known::User(k) => k.flags.is_final(),
            Known::Builtin(k) => k.is_final,
        }
    }

    pub(super) fn is_class(&self) -> bool {
        !self.is_trait() && !self.is_interface() && !self.is_enum()
    }

    pub(super) fn type_description(&self) -> &'static str {
        if self.is_interface() {
            "Interface"
        } else if self.is_trait() {
            "Trait"
        } else if self.is_enum() {
            "Enum"
        } else {
            "Class"
        }
    }

    pub(super) fn display(&self) -> String {
        match self {
            Known::User(k) => String::from_utf8_lossy(k.original_name.as_bytes()).into_owned(),
            Known::Builtin(k) => k.name.clone(),
        }
    }
}

pub(super) fn known<'a>(c: &Checker<'a, '_>, name: &str) -> Option<Known<'a>> {
    let name = name.trim_start_matches('\\');
    if let Some(k) = c
        .codebase
        .get_class_like(name.as_bytes())
        .filter(|k| !k.flags.is_built_in())
    {
        return Some(Known::User(k));
    }
    c.builtins
        .classes
        .get(&name.to_ascii_lowercase())
        .map(Known::Builtin)
}

impl<'a> Checker<'a, '_> {
    pub(super) fn function(&self, name: &[u8]) -> Option<(String, params::Variants)> {
        if let Some(f) = self
            .codebase
            .get_function(name)
            .filter(|f| !f.flags.is_built_in())
        {
            return Some((
                String::from_utf8_lossy(f.original_name.as_bytes()).into_owned(),
                params::Variants::of(f),
            ));
        }
        let lower = String::from_utf8_lossy(name).to_ascii_lowercase();
        let f = self.builtins.functions.get(&lower)?;
        Some((f.name.clone(), params::Variants::of_builtin(f)))
    }

    pub(super) fn class_like(&self, name: &str) -> Lookup<'a> {
        match known(self, name) {
            None => Lookup::NotFound,
            Some(Known::User(k)) => Lookup::Found(k),
            Some(Known::Builtin(k)) => self
                .codebase
                .get_class_like(k.name.as_bytes())
                .map_or(Lookup::Opaque, Lookup::Found),
        }
    }
}

pub(super) enum Lookup<'a> {
    NotFound,
    Opaque,
    Found(&'a ClassLikeMetadata),
}

impl<'a> Lookup<'a> {
    pub(super) fn found(self) -> Option<&'a ClassLikeMetadata> {
        match self {
            Lookup::Found(k) => Some(k),
            Lookup::NotFound | Lookup::Opaque => None,
        }
    }
}
