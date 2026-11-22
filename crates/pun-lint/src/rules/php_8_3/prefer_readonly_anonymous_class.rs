use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{ClassLikeMember, Modifier, Node, NodeKind, Sequence};

use crate::rules::support::class_members::method;
use crate::rules::support::names::resolve_class_name;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

/// Rector's `ReadOnlyAnonymousClassRector` with `ReadonlyClassManipulator`: an anonymous class whose properties
/// and promoted parameters are all typed and readonly becomes a readonly class, and they lose their own `readonly`.
pub struct PreferReadonlyAnonymousClass;

impl Rule for PreferReadonlyAnonymousClass {
    fn id(&self) -> &'static str {
        "lint/php-8.3/prefer-readonly-anonymous-class"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80300)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::AnonymousClass(class) = node else {
            return;
        };
        // An attribute between `new` and `class` cannot be printed with an added readonly modifier.
        if !class.attribute_lists.is_empty() || is_readonly(&class.modifiers) {
            return;
        }
        let codebase = &facts(ctx).codebase;
        if let Some(extends) = &class.extends {
            let name = resolve_class_name(ancestors, extends.types.first().expect("a parent"));
            let Some(parent) = codebase.get_class_like(name.as_bytes()) else {
                return;
            };
            let grandparents = (parent.all_parent_classes.iter())
                .filter_map(|name| codebase.get_class_like(name.as_bytes()));
            if std::iter::once(parent)
                .chain(grandparents)
                .any(|p| !p.flags.is_readonly())
            {
                return;
            }
        }
        let properties: Vec<_> = (class.members.iter())
            .filter_map(|member| match member {
                ClassLikeMember::Property(property) => Some(property),
                _ => None,
            })
            .collect();
        if properties
            .iter()
            .any(|p| !is_readonly(p.modifiers()) || p.hint().is_none())
        {
            return;
        }
        for member in &class.members {
            let ClassLikeMember::TraitUse(trait_use) = member else {
                continue;
            };
            for name in &trait_use.trait_names {
                let name = resolve_class_name(ancestors, name);
                let Some(t) = codebase.get_class_like(name.as_bytes()) else {
                    return;
                };
                if t.properties.values().any(|p| !p.flags.is_readonly()) {
                    return;
                }
            }
        }
        let params: Vec<_> = method(&class.members, b"__construct")
            .map_or_default(|constructor| constructor.parameter_list.parameters.iter().collect());
        if params.is_empty() && properties.is_empty() {
            return;
        }
        if params
            .iter()
            .any(|p| (p.is_promoted_property() && !is_readonly(&p.modifiers)) || p.hint.is_none())
        {
            return;
        }
        let at = node_span(&class.class).start;
        let mut edits = vec![Edit::new(Span::new(at, at), "readonly ")];
        for param in &params {
            // A parameter whose only modifier is `readonly` would stop being promoted.
            let only = param.modifiers.len() == 1 && param.hooks.is_none();
            edits.extend(without_readonly(&param.modifiers, only));
        }
        for property in &properties {
            let modifiers = property.modifiers();
            edits.extend(without_readonly(modifiers, !has_visibility(modifiers)));
        }
        out.push(Report::fixed(
            node_span(&class.class),
            "Make the anonymous class readonly instead of each of its properties.",
            Safety::Safe,
            edits,
        ));
    }
}

fn is_readonly(modifiers: &Sequence<'_, Modifier<'_>>) -> bool {
    modifiers.iter().any(|m| matches!(m, Modifier::Readonly(_)))
}

fn has_visibility(modifiers: &Sequence<'_, Modifier<'_>>) -> bool {
    modifiers.iter().any(|m| {
        matches!(
            m,
            Modifier::Public(_) | Modifier::Protected(_) | Modifier::Private(_)
        )
    })
}

/// `VisibilityManipulator::removeReadonly()`, printed as PHP-Parser prints changed modifiers: in its fixed order, with
/// `public` added (`make_public`) to a property without a visibility and to a parameter that would stop being
/// promoted.
fn without_readonly(modifiers: &Sequence<'_, Modifier<'_>>, make_public: bool) -> Option<Edit> {
    if !is_readonly(modifiers) {
        return None;
    }
    let has = |f: fn(&Modifier<'_>) -> bool| modifiers.iter().any(f);
    let order: [(fn(&Modifier<'_>) -> bool, &str); 9] = [
        (|m| matches!(m, Modifier::Final(_)), "final"),
        (|m| matches!(m, Modifier::Abstract(_)), "abstract"),
        (|m| matches!(m, Modifier::Public(_)), "public"),
        (|m| matches!(m, Modifier::Protected(_)), "protected"),
        (|m| matches!(m, Modifier::Private(_)), "private"),
        (|m| matches!(m, Modifier::PublicSet(_)), "public(set)"),
        (|m| matches!(m, Modifier::ProtectedSet(_)), "protected(set)"),
        (|m| matches!(m, Modifier::PrivateSet(_)), "private(set)"),
        (|m| matches!(m, Modifier::Static(_)), "static"),
    ];
    let mut words: Vec<&str> = order
        .iter()
        .filter(|(pick, _)| has(*pick))
        .map(|(_, word)| *word)
        .collect();
    if make_public {
        let before = has(|m| matches!(m, Modifier::Final(_)));
        words.insert(usize::from(before), "public");
    }
    let first = node_span(modifiers.first().expect("a readonly modifier")).start;
    let last = node_span(modifiers.last().expect("a readonly modifier")).end;
    Some(Edit::new(Span::new(first, last), words.join(" ")))
}
