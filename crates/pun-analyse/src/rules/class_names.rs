use std::collections::HashMap;

use mago_span::Span;
use mago_syntax::cst::*;

use mago_codex::metadata::class_like::ClassLikeMetadata;

use super::Checker;
use super::reflection::{Known, known};

pub(super) struct Pair {
    name: String,
    at: Span,
    written: bool,
}

impl Pair {
    pub(super) fn name(name: String, at: Span) -> Pair {
        Pair {
            name,
            at,
            written: true,
        }
    }

    pub(super) fn node(name: String, at: Span) -> Pair {
        Pair {
            name,
            at,
            written: false,
        }
    }
}

#[derive(Default)]
pub(super) struct State {
    explicit_aliases: HashMap<String, String>,
}

pub(super) fn enter_namespace(c: &mut Checker) {
    c.class_names.explicit_aliases.clear();
}

pub(super) fn use_statement(c: &mut Checker, u: &Use) {
    let items: Vec<&UseItem> = match &u.items {
        UseItems::Sequence(s) => s.items.iter().collect(),
        UseItems::MixedList(l) => l
            .items
            .iter()
            .filter(|i| i.r#type.is_none())
            .map(|i| &i.item)
            .collect(),
        UseItems::TypedSequence(_) | UseItems::TypedList(_) => Vec::new(),
    };
    for alias in items.iter().filter_map(|i| i.alias.as_ref()) {
        let alias = String::from_utf8_lossy(alias.identifier.value).into_owned();
        c.class_names
            .explicit_aliases
            .insert(alias.to_ascii_lowercase(), alias);
    }
}

pub(super) fn check(c: &mut Checker, pairs: &[Pair], case: bool) {
    check_at(c, pairs, None, case);
}

pub(super) fn check_at(c: &mut Checker, pairs: &[Pair], location: Option<&Location>, case: bool) {
    if case {
        case_sensitivity(c, pairs);
    }
    forbidden(c, pairs);
    let Some(location) = location else {
        return;
    };
    for pair in pairs {
        let Some(Known::User(k)) = known(c, &pair.name) else {
            continue;
        };
        for (message, identifier) in restricted(c, k, location) {
            c.report(pair.at, &identifier, message, None);
        }
    }
}

pub(super) struct Member {
    pub name: String,
    pub internal: bool,
    pub deprecated: bool,
}

pub(super) enum Location {
    TraitUse {
        class: Option<String>,
    },
    StaticPropertyAccess(Option<Member>),
    Attribute,
    Catch,
    ClassConstantAccess(Option<Member>),
    Extends {
        subject: String,
        class: Option<String>,
    },
    Implements {
        subject: String,
        class: Option<String>,
    },
    Instanceof,
    PropertyType {
        property: String,
    },
    ParameterType {
        parameter: String,
        function: Function,
    },
    ReturnType(Function),
    Instantiation,
    StaticMethodCall(Option<Member>),
}

pub(super) enum Function {
    Anonymous,
    Method { name: String, class: Option<String> },
    Named(String),
}

impl Location {
    fn message(&self, part: &str) -> String {
        match self {
            Location::TraitUse { class: Some(class) } => {
                format!("Usage of {part} in class {class}.")
            }
            Location::TraitUse { class: None } => format!("Usage of {part}."),
            Location::StaticPropertyAccess(Some(p)) => {
                format!("Access to static property ${} on {part}.", p.name)
            }
            Location::StaticPropertyAccess(None) => format!("Access to static property on {part}."),
            Location::Attribute => format!("Attribute references {part}."),
            Location::Catch => format!("Catching {part}."),
            Location::ClassConstantAccess(Some(k)) => {
                format!("Access to constant {} on {part}.", k.name)
            }
            Location::ClassConstantAccess(None) => format!("Access to constant on {part}."),
            Location::Extends { subject, .. } => format!("{subject} extends {part}."),
            Location::Implements { subject, .. } => format!("{subject} implements {part}."),
            Location::Instanceof => format!("Instanceof references {part}."),
            Location::PropertyType { property } => {
                format!("Property ${property} references {part} in its type.")
            }
            Location::ParameterType {
                parameter,
                function,
            } => match function {
                Function::Anonymous => format!(
                    "Parameter ${parameter} of anonymous function has typehint with {part}."
                ),
                Function::Method {
                    name,
                    class: Some(class),
                } => {
                    format!(
                        "Parameter ${parameter} of method {class}::{name}() has typehint with {part}."
                    )
                }
                Function::Method { name, class: None } => {
                    format!(
                        "Parameter ${parameter} of method {name}() in anonymous class has typehint with {part}."
                    )
                }
                Function::Named(name) => {
                    format!("Parameter ${parameter} of function {name}() has typehint with {part}.")
                }
            },
            Location::ReturnType(function) => match function {
                Function::Anonymous => {
                    format!("Return type of anonymous function has typehint with {part}.")
                }
                Function::Method {
                    name,
                    class: Some(class),
                } => {
                    format!("Return type of method {class}::{name}() has typehint with {part}.")
                }
                Function::Method { name, class: None } => {
                    format!(
                        "Return type of method {name}() in anonymous class has typehint with {part}."
                    )
                }
                Function::Named(name) => {
                    format!("Return type of function {name}() has typehint with {part}.")
                }
            },
            Location::Instantiation => format!("Instantiation of {part}."),
            Location::StaticMethodCall(Some(m)) => {
                format!("Call to static method {}() on {part}.", m.name)
            }
            Location::StaticMethodCall(None) => format!("Call to static method on {part}."),
        }
    }

    fn identifier(&self, second: &str) -> String {
        let ucfirst = || format!("{}{}", second[..1].to_ascii_uppercase(), &second[1..]);
        let second = match self {
            Location::Extends { .. } => format!("extends{}", ucfirst()),
            Location::Implements { .. } => format!("implements{}", ucfirst()),
            _ => second.to_string(),
        };
        format!("{}.{second}", self.value())
    }

    fn value(&self) -> &'static str {
        match self {
            Location::TraitUse { .. } => "traitUse",
            Location::StaticPropertyAccess(_) => "staticProperty",
            Location::Attribute => "attribute",
            Location::Catch => "catch",
            Location::ClassConstantAccess(_) => "classConstant",
            Location::Extends { subject, .. } if subject.starts_with("Interface") => "interface",
            Location::Implements { subject, .. } if subject.starts_with("Enum") => "enum",
            Location::Extends { .. } | Location::Implements { .. } => "class",
            Location::Instanceof => "instanceof",
            Location::PropertyType { .. } => "property",
            Location::ParameterType { .. } => "parameter",
            Location::ReturnType(_) => "return",
            Location::Instantiation => "new",
            Location::StaticMethodCall(_) => "staticMethod",
        }
    }

    fn current_class(&self) -> Option<&str> {
        match self {
            Location::TraitUse { class }
            | Location::Extends { class, .. }
            | Location::Implements { class, .. }
            | Location::ParameterType {
                function: Function::Method { class, .. },
                ..
            }
            | Location::ReturnType(Function::Method { class, .. }) => class.as_deref(),
            _ => None,
        }
    }

    fn member(&self) -> Option<&Member> {
        match self {
            Location::StaticPropertyAccess(m)
            | Location::ClassConstantAccess(m)
            | Location::StaticMethodCall(m) => m.as_ref(),
            _ => None,
        }
    }
}

pub(super) fn restricted(
    c: &Checker,
    k: &ClassLikeMetadata,
    location: &Location,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let description = super::deprecations::type_description(k);
    let name = String::from_utf8_lossy(k.original_name.as_bytes()).into_owned();
    if c.on(crate::rules::Switches::INTERNAL_TAG)
        && k.flags.is_internal()
        && super::internal::should_be_reported(c, &name)
        && !location.member().is_some_and(|m| m.internal)
    {
        let part = format!(
            "internal {} {}",
            description.to_ascii_lowercase(),
            c.display(k)
        );
        out.push((
            location.message(&part),
            location.identifier(&format!("internal{description}")),
        ));
    }
    if c.on(crate::rules::Switches::DEPRECATIONS)
        && let Some(deprecation) = c.class_deprecation(k)
        && !c.scope_deprecated()
        && location
            .current_class()
            .and_then(|class| c.codebase.get_class_like(class.as_bytes()))
            .is_none_or(|current| c.class_deprecation(current).is_none())
        && !location.member().is_some_and(|m| m.deprecated)
        && (c.on(crate::rules::Switches::BLEEDING_EDGE)
            || matches!(
                location,
                Location::Implements { .. }
                    | Location::Extends { .. }
                    | Location::Instantiation
                    | Location::TraitUse { .. }
                    | Location::StaticMethodCall(_)
                    | Location::StaticPropertyAccess(_)
                    | Location::ClassConstantAccess(_)
                    | Location::ParameterType { .. }
                    | Location::ReturnType(_)
            ))
    {
        let part = format!(
            "deprecated {} {}",
            description.to_ascii_lowercase(),
            c.display(k)
        );
        let message = location.message(&part);
        let message = match deprecation {
            None => message,
            Some(d) => format!("{}:\n{d}", message.trim_end_matches('.')),
        };
        out.push((
            message,
            location.identifier(&format!("deprecated{description}")),
        ));
    }
    out
}

const STUBBED: [&str; 4] = [
    "allowdynamicproperties",
    "attribute",
    "returntypewillchange",
    "sensitiveparameter",
];

fn case_sensitivity(c: &mut Checker, pairs: &[Pair]) {
    for pair in pairs {
        let name = if c.on(crate::rules::Switches::CHECK_IMPORTED_CLASS_NAME_CASE)
            && pair.written
            && c.names.resolve(&pair.at) == Some(pair.name.as_bytes())
            && !c.written(pair.at).contains('\\')
        {
            original_case(c, pair.at)
        } else {
            pair.name.clone()
        };
        let Some(k) = known(c, &name) else {
            continue;
        };
        if let Known::Builtin(b) = k
            && !c.on(crate::rules::Switches::CHECK_INTERNAL_CLASS_CASE_SENSITIVITY)
            && !STUBBED.contains(&b.name.to_ascii_lowercase().as_str())
        {
            continue;
        }
        let real = k.display();
        if !real.eq_ignore_ascii_case(&name)
            || (pair.written && explicit_alias(c, pair.at))
            || real == name
        {
            continue;
        }
        let kind = k.type_description();
        c.report(
            pair.at,
            &format!("{}.nameCase", kind.to_ascii_lowercase()),
            format!("{kind} {real} referenced with incorrect case: {name}."),
            None,
        );
    }
}

fn explicit_alias(c: &Checker, at: Span) -> bool {
    let written = c.written(at);
    c.class_names
        .explicit_aliases
        .get(&written.to_ascii_lowercase())
        .is_some_and(|alias| *alias == written)
}

const INTERNAL_CLASS_PREFIXES: [(&str, &str); 5] = [
    ("PHPStan", "_PHPStan_"),
    ("Rector", "RectorPrefix"),
    ("PHP-Scoper", "_PhpScoper"),
    ("PHPUnit", "PHPUnitPHAR"),
    ("Box", "_HumbugBox"),
];

fn forbidden(c: &mut Checker, pairs: &[Pair]) {
    for pair in pairs {
        let name = pair.name.as_str();
        let mut found = None;
        for (project, prefix) in INTERNAL_CLASS_PREFIXES {
            let Some(rest) = name.strip_prefix(prefix) else {
                continue;
            };
            found = Some((project, rest.find('\\').map_or(rest, |pos| &rest[pos..])));
        }
        let Some((project, without_prefix)) = found else {
            continue;
        };
        c.report_non_ignorable(
            pair.at,
            "class.prefixed",
            format!("Referencing prefixed {project} class: {name}."),
            Some(&format!(
                "This is most likely unintentional. Did you mean to type {without_prefix}?"
            )),
        );
    }
}

pub(super) fn original_case(c: &Checker, at: Span) -> String {
    let resolved = c.resolved_span(at);
    let written = c.written(at);
    let written = written.trim_start_matches('\\');
    let written = match written.split_once('\\') {
        Some((first, rest)) if first.eq_ignore_ascii_case("namespace") => rest,
        _ => written,
    };
    let original: Vec<&str> = written.split('\\').collect();
    let parts: Vec<&str> = resolved.split('\\').collect();
    parts[..parts.len().saturating_sub(original.len())]
        .iter()
        .chain(&original)
        .copied()
        .collect::<Vec<_>>()
        .join("\\")
}
