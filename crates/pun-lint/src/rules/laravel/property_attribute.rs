use mago_syntax::cst::*;
use pun_problem::{Edit, Safety, Span};
use std::fmt::Write as _;

use crate::rules::support::class_members::member_from;
use crate::rules::support::conditions::{indentation, reprint};
use crate::rules::support::expressions::is_constant;
use crate::rules::support::names::has_attribute;
use crate::rules::support::rewrites::remove_items;
use crate::rules::support::statement_edits::{first_comment, line_indent};
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report};

pub(crate) const MODEL: &str = "Illuminate\\Database\\Eloquent\\Model";
pub(crate) const COMMAND: &str = "Illuminate\\Console\\Command";
pub(crate) const SHOULD_QUEUE: &str = "Illuminate\\Contracts\\Queue\\ShouldQueue";
pub(crate) const JSON_RESOURCE: &str = "Illuminate\\Http\\Resources\\Json\\JsonResource";
pub(crate) const FORM_REQUEST: &str = "Illuminate\\Foundation\\Http\\FormRequest";

#[derive(Clone, Copy)]
pub(crate) enum Visibility {
    Protected,
    Public,
}

#[derive(Clone, Copy)]
pub(crate) enum Check {
    Any,
    String,
    ClassConstOrString,
    StringArray,
    MaybeStringArray,
    TypedStringArray,
    Integer,
    True,
    TrueOrConstant,
    FalseConstant,
}

pub(crate) struct Config {
    pub id: &'static str,
    pub message: &'static str,
    pub class: &'static str,
    pub final_only: bool,
    pub property: &'static str,
    pub visibility: Visibility,
    pub check: Check,
    pub attribute: &'static str,
    pub skip_existing: bool,
}

pub(crate) struct ClassView<'ast, 'arena> {
    pub node: Node<'ast, 'arena>,
    pub attribute_lists: &'ast Sequence<'arena, AttributeList<'arena>>,
    pub modifiers: &'ast Sequence<'arena, Modifier<'arena>>,
    pub members: &'ast Sequence<'arena, ClassLikeMember<'arena>>,
    pub extends: &'ast Option<Extends<'arena>>,
    pub implements: &'ast Option<Implements<'arena>>,
    pub left_brace: mago_span::Span,
    pub right_brace: mago_span::Span,
    pub kind: Kind<'ast, 'arena>,
}

#[derive(Clone, Copy)]
pub(crate) enum Kind<'ast, 'arena> {
    Named(&'ast Class<'arena>),
    Anonymous(&'ast AnonymousClass<'arena>),
}

impl<'ast, 'arena> ClassView<'ast, 'arena> {
    pub(crate) fn of(node: Node<'ast, 'arena>) -> Option<Self> {
        if let Node::Class(c) = node {
            return Some(ClassView {
                node,
                attribute_lists: &c.attribute_lists,
                modifiers: &c.modifiers,
                members: &c.members,
                extends: &c.extends,
                implements: &c.implements,
                left_brace: c.left_brace,
                right_brace: c.right_brace,
                kind: Kind::Named(c),
            });
        }
        let Node::AnonymousClass(c) = node else {
            return None;
        };
        Some(ClassView {
            node,
            attribute_lists: &c.attribute_lists,
            modifiers: &c.modifiers,
            members: &c.members,
            extends: &c.extends,
            implements: &c.implements,
            left_brace: c.left_brace,
            right_brace: c.right_brace,
            kind: Kind::Anonymous(c),
        })
    }

    pub(crate) fn is_final(&self) -> bool {
        self.modifiers
            .iter()
            .any(|m| matches!(m, Modifier::Final(_)))
    }

    pub(crate) fn name(&self, ctx: &Context<'_>, ancestors: &[Node<'_, '_>]) -> String {
        match self.kind {
            Kind::Named(c) => {
                let name = String::from_utf8_lossy(c.name.value);
                match crate::rules::types::namespace_of(ancestors) {
                    Some(ns) => format!("{ns}\\{name}"),
                    None => name.into_owned(),
                }
            }
            Kind::Anonymous(c) => facts(ctx).anonymous_class_name(mago_span::HasSpan::span(c)),
        }
    }

    pub(crate) fn is_object_type(
        &self,
        ctx: &Context<'_>,
        ancestors: &[Node<'_, '_>],
        target: &str,
    ) -> bool {
        facts(ctx)
            .codebase
            .get_class_like(target.as_bytes())
            .is_some()
            && super::is_a(ctx, &self.name(ctx, ancestors), &[target])
    }

    pub(crate) fn has_attribute(&self, ancestors: &[Node<'_, '_>], attribute: &str) -> bool {
        has_attribute(ancestors, self.attribute_lists, attribute)
    }

    pub(crate) fn property(&self, name: &str) -> Option<(usize, PropertyView<'ast, 'arena>)> {
        self.members.iter().enumerate().find_map(|(i, m)| {
            let ClassLikeMember::Property(p) = m else {
                return None;
            };
            let view = PropertyView::of(p);
            view.items
                .iter()
                .any(|item| &item.variable().name[1..] == name.as_bytes())
                .then_some((i, view))
        })
    }

    pub(crate) fn method(&self, name: &str) -> Option<(usize, &'ast Method<'arena>)> {
        self.members.iter().enumerate().find_map(|(i, m)| match m {
            ClassLikeMember::Method(method)
                if method.name.value.eq_ignore_ascii_case(name.as_bytes()) =>
            {
                Some((i, method))
            }
            _ => None,
        })
    }
}

pub(crate) struct PropertyView<'ast, 'arena> {
    pub modifiers: &'ast Sequence<'arena, Modifier<'arena>>,
    pub items: Vec<&'ast PropertyItem<'arena>>,
}

impl<'ast, 'arena> PropertyView<'ast, 'arena> {
    fn of(property: &'ast Property<'arena>) -> Self {
        match property {
            Property::Plain(p) => PropertyView {
                modifiers: &p.modifiers,
                items: p.items.iter().collect(),
            },
            Property::Hooked(p) => PropertyView {
                modifiers: &p.modifiers,
                items: vec![&p.item],
            },
        }
    }

    pub(crate) fn is_protected(&self) -> bool {
        self.modifiers
            .iter()
            .any(|m| matches!(m, Modifier::Protected(_)))
    }

    pub(crate) fn is_public(&self) -> bool {
        !(self.modifiers.iter()).any(|m| matches!(m, Modifier::Protected(_) | Modifier::Private(_)))
    }

    pub(crate) fn has(&self, visibility: Visibility) -> bool {
        match visibility {
            Visibility::Protected => self.is_protected(),
            Visibility::Public => self.is_public(),
        }
    }

    pub(crate) fn default(&self) -> Option<&'ast Expression<'arena>> {
        match self.items[0] {
            PropertyItem::Concrete(c) => Some(unparenthesize(c.value)),
            PropertyItem::Abstract(_) => None,
        }
    }
}

pub(crate) fn is_string_node(expr: &Expression<'_>) -> bool {
    match unparenthesize(expr) {
        Expression::Literal(Literal::String(_)) => true,
        Expression::CompositeString(CompositeString::Document(d)) => {
            d.parts.iter().all(|p| matches!(p, StringPart::Literal(_)))
        }
        _ => false,
    }
}

pub(crate) fn array_elements<'ast, 'arena>(
    expr: &'ast Expression<'arena>,
) -> Option<&'ast TokenSeparatedSequence<'arena, ArrayElement<'arena>>> {
    match expr {
        Expression::Array(a) => Some(&a.elements),
        Expression::LegacyArray(a) => Some(&a.elements),
        _ => None,
    }
}

fn element_value<'ast, 'arena>(
    element: &'ast ArrayElement<'arena>,
) -> Option<&'ast Expression<'arena>> {
    match element {
        ArrayElement::KeyValue(e) => Some(e.value),
        ArrayElement::Value(e) => Some(e.value),
        ArrayElement::Variadic(e) => Some(e.value),
        ArrayElement::Missing(_) => None,
    }
}

fn type_of<'f>(
    facts: &'f crate::rules::types::facts::Facts,
    expr: &Expression<'_>,
) -> Option<&'f mago_codex::ttype::union::TUnion> {
    facts.full_type(node_span(unparenthesize(expr)))
}

fn never_string(t: &mago_codex::ttype::union::TUnion) -> bool {
    use mago_codex::ttype::atomic::TAtomic;
    use mago_codex::ttype::atomic::scalar::TScalar;
    !t.types.is_empty()
        && t.types.iter().all(|a| {
            matches!(
                a,
                TAtomic::Scalar(TScalar::Bool(_) | TScalar::Integer(_) | TScalar::Float(_))
                    | TAtomic::Null
                    | TAtomic::Array(_)
                    | TAtomic::Object(_)
            )
        })
}

impl Config {
    fn passes(&self, ctx: &Context<'_>, value: &Expression<'_>) -> bool {
        let facts = || facts(ctx);
        match self.check {
            Check::Any => true,
            Check::String => is_string_node(value),
            Check::ClassConstOrString => {
                matches!(value, Expression::Access(Access::ClassConstant(_)))
                    || is_string_node(value)
            }
            Check::StringArray | Check::MaybeStringArray | Check::TypedStringArray => {
                let Some(elements) = array_elements(value) else {
                    return false;
                };
                if elements.is_empty() {
                    return false;
                }
                let facts = facts();
                elements.iter().all(|e| {
                    let Some(v) = element_value(e) else {
                        return false;
                    };
                    match self.check {
                        Check::StringArray => is_string_node(v),
                        Check::MaybeStringArray => !type_of(&facts, v).is_some_and(never_string),
                        _ => type_of(&facts, v)
                            .is_some_and(|t| t.is_any_string() && !t.types.is_empty()),
                    }
                })
            }
            Check::Integer => {
                type_of(&facts(), value).is_some_and(|t| t.is_int() && !t.types.is_empty())
            }
            Check::True => {
                type_of(&facts(), value).is_some_and(mago_codex::ttype::union::TUnion::is_true)
            }
            Check::TrueOrConstant => {
                type_of(&facts(), value).is_some_and(mago_codex::ttype::union::TUnion::is_true)
                    || is_constant(value, "true")
            }
            Check::FalseConstant => is_constant(value, "false"),
        }
    }

    fn with_argument(&self) -> bool {
        !matches!(
            self.check,
            Check::True | Check::TrueOrConstant | Check::FalseConstant
        )
    }

    pub(crate) fn refactor(
        &self,
        node: Node<'_, '_>,
        ancestors: &[Node<'_, '_>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(class) = ClassView::of(node) else {
            return;
        };
        if (self.final_only && !class.is_final())
            || !class.is_object_type(ctx, ancestors, self.class)
            || (self.skip_existing && class.has_attribute(ancestors, self.attribute))
        {
            return;
        }
        let Some((index, property)) = class.property(self.property) else {
            return;
        };
        if !property.has(self.visibility) {
            return;
        }
        let Some(value) = property.default() else {
            return;
        };
        if !self.passes(ctx, value) {
            return;
        }
        let args = if self.with_argument() {
            format!("({})", moved_value(ctx.source, &class, value))
        } else {
            String::new()
        };
        let group = format!("#[\\{}{args}]", self.attribute);
        let Some(edits) = rewrite(ctx.source, &class, &[group], &[], &[index]) else {
            return;
        };
        out.push(report(&class, self.message, edits));
    }
}

pub(crate) fn report(class: &ClassView<'_, '_>, message: &str, edits: Vec<Edit>) -> Report {
    Report::fixed(
        node_span(&class.node),
        message.to_string(),
        Safety::Unsafe,
        edits,
    )
}

pub(crate) fn moved_value(
    source: &str,
    class: &ClassView<'_, '_>,
    expr: &Expression<'_>,
) -> String {
    let span = node_span(unparenthesize(expr));
    let adjust =
        indentation(source, class_start(class)) as isize - indentation(source, span.start) as isize;
    reprint(
        source,
        &crate::rules::support::statements::lex(source),
        span,
        adjust,
    )
}

fn class_start(class: &ClassView<'_, '_>) -> usize {
    node_span(&class.node).start
}

pub(crate) fn rewrite(
    source: &str,
    class: &ClassView<'_, '_>,
    groups: &[String],
    replaced: &[usize],
    removed: &[usize],
) -> Option<Vec<Edit>> {
    let start = class_start(class);
    let indent = line_indent(source, start);
    if indent.contains('\t') {
        return None;
    }
    let nl = format!("\n{indent}");
    let (open, close) = (
        class.left_brace.end.offset as usize,
        class.right_brace.start.offset as usize,
    );
    let flags: Vec<bool> = (0..class.members.len())
        .map(|i| removed.contains(&i))
        .collect();
    let mut spans: Vec<Span> = class.members.iter().map(node_span).collect();
    let mut froms: Vec<usize> = (0..spans.len())
        .map(|i| member_from(class.members, class.left_brace, i))
        .collect();
    let mut kept_flags = flags;
    let after = spans.last().map_or(open, |s| s.end);
    if let Some(comment) = first_comment(source, after, close) {
        let end = after + source[after..close].trim_end().len();
        froms.push(after);
        spans.push(Span::new(comment, end));
        kept_flags.push(false);
    }
    let Some(mut edits) = remove_items(source, &spans, &froms, &kept_flags) else {
        return Some(vec![Edit::new(
            Span::new(start, close + 1),
            reprinted(source, class, groups, replaced, &nl),
        )]);
    };
    let lists: Vec<Span> = class.attribute_lists.iter().map(node_span).collect();
    let mut new = groups.iter();
    for &i in replaced {
        edits.push(Edit::new(lists[i], new.next().unwrap().clone()));
    }
    let rest: Vec<&String> = new.collect();
    if !rest.is_empty() {
        let sep = if matches!(class.kind, Kind::Anonymous(_)) {
            " ".to_string()
        } else {
            nl
        };
        match lists.last() {
            Some(last) => {
                let text: String = rest.iter().map(|g| format!("{sep}{g}")).collect();
                edits.push(Edit::new(Span::new(last.end, last.end), text));
            }
            None => {
                let at = match class.kind {
                    Kind::Anonymous(c) => {
                        let new_end = c.new.span.end.offset as usize;
                        new_end + source[new_end..].len() - source[new_end..].trim_start().len()
                    }
                    Kind::Named(_) => start,
                };
                let text: String = rest.iter().map(|g| format!("{g}{sep}")).collect();
                edits.push(Edit::new(Span::new(at, at), text));
            }
        }
    }
    edits.sort_by_key(|e| e.span.start);
    Some(edits)
}

fn reprinted(
    source: &str,
    class: &ClassView<'_, '_>,
    groups: &[String],
    replaced: &[usize],
    nl: &str,
) -> String {
    let text = |s: Span| &source[s.start..s.end];
    let mut out = String::new();
    if matches!(class.kind, Kind::Anonymous(_)) {
        out.push_str("new ");
    }
    let mut new = groups.iter();
    for (i, list) in class.attribute_lists.iter().enumerate() {
        match replaced.contains(&i) {
            true => out.push_str(new.next().unwrap()),
            false => out.push_str(text(node_span(list))),
        }
        out.push_str(nl);
    }
    for g in new {
        out.push_str(g);
        out.push_str(nl);
    }
    let has = |f: fn(&Modifier<'_>) -> bool| class.modifiers.iter().any(f);
    if has(|m| matches!(m, Modifier::Final(_))) {
        out.push_str("final ");
    }
    if has(|m| matches!(m, Modifier::Abstract(_))) {
        out.push_str("abstract ");
    }
    if has(|m| matches!(m, Modifier::Readonly(_))) {
        out.push_str("readonly ");
    }
    out.push_str("class");
    match class.kind {
        Kind::Named(c) => {
            out.push(' ');
            out.push_str(&String::from_utf8_lossy(c.name.value));
        }
        Kind::Anonymous(c) => {
            if let Some(args) = c.argument_list.as_ref().filter(|a| !a.arguments.is_empty()) {
                let args: Vec<&str> = args.arguments.iter().map(|a| text(node_span(a))).collect();
                let _ = write!(out, "({})", args.join(", "));
            }
        }
    }
    let names = |types: &TokenSeparatedSequence<'_, Identifier<'_>>| {
        types
            .iter()
            .map(|t| text(node_span(t)).to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    if let Some(e) = class.extends {
        out.push_str(" extends ");
        out.push_str(&names(&e.types));
    }
    if let Some(i) = class.implements {
        out.push_str(" implements ");
        out.push_str(&names(&i.types));
    }
    out.push_str(nl);
    out.push('{');
    out.push_str(nl);
    out.push('}');
    out
}

macro_rules! property_rules {
    ($($name:ident = $config:expr;)*) => {$(
        pub struct $name;

        impl $name {
            const CONFIG: $crate::rules::laravel::property_attribute::Config = $config;
        }

        impl $crate::Rule for $name {
            fn id(&self) -> &'static str {
                Self::CONFIG.id
            }

            fn targets(&self) -> Option<&'static [mago_syntax::cst::NodeKind]> {
                Some(&[mago_syntax::cst::NodeKind::Class, mago_syntax::cst::NodeKind::AnonymousClass])
            }

            fn visit<'ast, 'arena>(
                &self,
                node: mago_syntax::cst::Node<'ast, 'arena>,
                ancestors: &[mago_syntax::cst::Node<'ast, 'arena>],
                ctx: &$crate::Context<'_>,
                out: &mut Vec<$crate::Report>,
            ) {
                Self::CONFIG.refactor(node, ancestors, ctx, out);
            }
        }
    )*};
}
pub(crate) use property_rules;
