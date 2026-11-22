//! Rector 2.8.4's `ReadOnlyClassRector` with `Php82\NodeManipulator\ReadonlyClassManipulator`.

use std::collections::HashSet;

use crate::problem::{Edit, Safety, Span};
use mago_codex::metadata::CodebaseMetadata;
use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_syntax::cst::{
    Class, ClassLikeMember, FunctionLikeParameter, Method, Modifier, Node, NodeKind, Property,
    Sequence,
};

use super::super::types::facts::facts;
use crate::rules::support::names::{has_attribute, resolve_class_name};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

pub struct PreferReadonlyClass;

impl Rule for PreferReadonlyClass {
    fn id(&self) -> &'static str {
        "lint/php-8.2/prefer-readonly-class"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80200)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Class(class) = node else { return };
        let facts = facts(ctx);
        let codebase = &facts.codebase;
        let parents = parent_class_reflections(codebase, ancestors, class);
        if should_skip(codebase, ancestors, class, &parents) {
            return;
        }
        let mut edits = vec![make_readonly(class)];
        if let Some(constructor) = construct(class) {
            for param in &constructor.parameter_list.parameters {
                edits.extend(remove_readonly_param(param));
            }
        }
        for property in properties(class) {
            edits.extend(remove_readonly_property(property));
        }
        out.push(Report::fixed(
            node_span(&class.name),
            "Make the class `readonly`: every property it declares is readonly.",
            Safety::Unsafe,
            edits,
        ));
    }
}

/// `resolveParentClassReflections()`: the parent and its parents, or none when there is no known parent.
fn parent_class_reflections<'c>(
    codebase: &'c CodebaseMetadata,
    ancestors: &[Node<'_, '_>],
    class: &Class<'_>,
) -> Vec<&'c ClassLikeMetadata> {
    let Some(parent) =
        extends(ancestors, class).and_then(|n| codebase.get_class_like(n.as_bytes()))
    else {
        return Vec::new();
    };
    std::iter::once(parent)
        .chain(
            (parent.all_parent_classes.iter())
                .filter_map(|c| codebase.get_class_like(c.as_bytes())),
        )
        .collect()
}

fn extends(ancestors: &[Node<'_, '_>], class: &Class<'_>) -> Option<String> {
    let name = class.extends.as_ref()?.types.first()?;
    Some(resolve_class_name(ancestors, name))
}

fn should_skip(
    codebase: &CodebaseMetadata,
    ancestors: &[Node<'_, '_>],
    class: &Class<'_>,
    parents: &[&ClassLikeMetadata],
) -> bool {
    if should_skip_class(codebase, ancestors, class) {
        return true;
    }
    if !has(&class.modifiers, |m| matches!(m, Modifier::Final(_))) {
        return !parents.iter().any(|p| p.flags.is_readonly());
    }
    if parents.iter().any(|p| !p.flags.is_readonly()) {
        return true;
    }
    let properties = properties(class);
    if properties
        .iter()
        .any(|p| !has(p.modifiers(), |m| matches!(m, Modifier::Readonly(_))))
    {
        return true;
    }
    if properties.iter().any(|p| p.hint().is_none()) {
        return true;
    }
    if should_skip_consume_trait_property(codebase, ancestors, class) {
        return true;
    }
    match construct(class) {
        Some(constructor) if !constructor.parameter_list.parameters.is_empty() => {
            should_skip_params(constructor)
        }
        _ => properties.is_empty(),
    }
}

fn should_skip_class(
    codebase: &CodebaseMetadata,
    ancestors: &[Node<'_, '_>],
    class: &Class<'_>,
) -> bool {
    has(&class.modifiers, |m| matches!(m, Modifier::Readonly(_)))
        || has_attribute(ancestors, &class.attribute_lists, "AllowDynamicProperties")
        || extends(ancestors, class).is_some_and(|n| !codebase.class_like_exists(n.as_bytes()))
}

/// A used trait Rector can't load, or one with a property that isn't readonly (its own traits' included).
fn should_skip_consume_trait_property(
    codebase: &CodebaseMetadata,
    ancestors: &[Node<'_, '_>],
    class: &Class<'_>,
) -> bool {
    class.members.iter().any(|member| {
        let ClassLikeMember::TraitUse(used) = member else {
            return false;
        };
        used.trait_names.iter().any(|name| {
            let name = resolve_class_name(ancestors, name);
            match codebase.get_class_like(name.as_bytes()) {
                None => true,
                Some(t) => has_writable_trait_property(codebase, t, &mut HashSet::new()),
            }
        })
    })
}

fn has_writable_trait_property(
    codebase: &CodebaseMetadata,
    t: &ClassLikeMetadata,
    seen: &mut HashSet<String>,
) -> bool {
    if !seen.insert(t.name.to_string()) {
        return false;
    }
    t.properties.values().any(|p| !p.flags.is_readonly())
        || (t.used_traits.iter())
            .filter_map(|n| codebase.get_class_like(n.as_bytes()))
            .any(|u| has_writable_trait_property(codebase, u, seen))
}

fn should_skip_params(constructor: &Method<'_>) -> bool {
    constructor.parameter_list.parameters.iter().any(|param| {
        (!has(&param.modifiers, |m| matches!(m, Modifier::Readonly(_))) && promoted(param))
            || param.hint.is_none()
    })
}

fn promoted(param: &FunctionLikeParameter<'_>) -> bool {
    !param.modifiers.is_empty() || param.hooks.is_some()
}

fn properties<'a>(class: &'a Class<'a>) -> Vec<&'a Property<'a>> {
    (class.members.iter())
        .filter_map(|m| match m {
            ClassLikeMember::Property(p) => Some(p),
            _ => None,
        })
        .collect()
}

fn construct<'a>(class: &'a Class<'a>) -> Option<&'a Method<'a>> {
    class.members.iter().find_map(|m| match m {
        ClassLikeMember::Method(method)
            if method.name.value.eq_ignore_ascii_case(b"__construct") =>
        {
            Some(method)
        }
        _ => None,
    })
}

fn has(modifiers: &Sequence<'_, Modifier<'_>>, pick: fn(&Modifier<'_>) -> bool) -> bool {
    modifiers.iter().any(pick)
}

/// The modifier keywords that matter here, as PHP-Parser's `Modifiers` flags.
#[derive(Clone, Copy, PartialEq)]
struct Flags(u16);

const FINAL: u16 = 1;
const ABSTRACT: u16 = 2;
const PUBLIC: u16 = 4;
const PROTECTED: u16 = 8;
const PRIVATE: u16 = 16;
const PUBLIC_SET: u16 = 32;
const PROTECTED_SET: u16 = 64;
const PRIVATE_SET: u16 = 128;
const STATIC: u16 = 256;
const READONLY: u16 = 512;
const VISIBILITY: u16 = PUBLIC | PROTECTED | PRIVATE;

impl Flags {
    fn of(modifiers: &Sequence<'_, Modifier<'_>>) -> Self {
        Flags(modifiers.iter().fold(0, |flags, m| {
            flags
                | match m {
                    Modifier::Final(_) => FINAL,
                    Modifier::Abstract(_) => ABSTRACT,
                    Modifier::Public(_) => PUBLIC,
                    Modifier::Protected(_) => PROTECTED,
                    Modifier::Private(_) => PRIVATE,
                    Modifier::PublicSet(_) => PUBLIC_SET,
                    Modifier::ProtectedSet(_) => PROTECTED_SET,
                    Modifier::PrivateSet(_) => PRIVATE_SET,
                    Modifier::Static(_) => STATIC,
                    Modifier::Readonly(_) => READONLY,
                }
        }))
    }

    /// PHP-Parser's `PrettyPrinterAbstract::pModifiers()`.
    fn print(self) -> String {
        [
            (FINAL, "final "),
            (ABSTRACT, "abstract "),
            (PUBLIC, "public "),
            (PROTECTED, "protected "),
            (PRIVATE, "private "),
            (PUBLIC_SET, "public(set) "),
            (PROTECTED_SET, "protected(set) "),
            (PRIVATE_SET, "private(set) "),
            (STATIC, "static "),
            (READONLY, "readonly "),
        ]
        .iter()
        .filter(|(flag, _)| self.0 & flag != 0)
        .map(|(_, text)| *text)
        .collect()
    }
}

/// Reprints the modifiers from the first one (or `var`) up to what follows them, as PHP-Parser does when the flags
/// change.
fn reprint(
    modifiers: &Sequence<'_, Modifier<'_>>,
    start: Option<usize>,
    next: usize,
    flags: Flags,
) -> Edit {
    let start = (modifiers.first().map(|m| node_span(m).start))
        .or(start)
        .unwrap_or(next);
    Edit::new(Span::new(start, next), flags.print())
}

fn make_readonly(class: &Class<'_>) -> Edit {
    let flags = Flags(Flags::of(&class.modifiers).0 | READONLY);
    reprint(&class.modifiers, None, node_span(&class.class).start, flags)
}

/// `VisibilityManipulator::removeReadonly()` on a promoted parameter: one left with no modifier becomes public.
fn remove_readonly_param(param: &FunctionLikeParameter<'_>) -> Option<Edit> {
    let old = Flags::of(&param.modifiers);
    if old.0 & READONLY == 0 {
        return None;
    }
    let mut flags = old.0 & !READONLY;
    if flags == 0 && param.hooks.is_none() {
        flags = PUBLIC;
    }
    let next = (param.hint.as_ref().map(|h| node_span(h).start))
        .or(param.ampersand.map(|s| s.start.offset as usize))
        .unwrap_or(node_span(&param.variable).start);
    Some(reprint(&param.modifiers, None, next, Flags(flags)))
}

/// `removeReadonly()` on a property, then `publicize()`: a property without a visibility keyword gets `public`.
fn remove_readonly_property(property: &Property<'_>) -> Option<Edit> {
    let old = Flags::of(property.modifiers());
    let mut flags = old.0 & !READONLY;
    if flags & VISIBILITY == 0 {
        flags |= PUBLIC;
    }
    if flags == old.0 {
        return None;
    }
    let next = property
        .hint()
        .map_or(node_span(property.first_variable()).start, |h| {
            node_span(h).start
        });
    let var = property.var().map(|v| node_span(v).start);
    Some(reprint(property.modifiers(), var, next, Flags(flags)))
}
