use crate::problem::{Edit, Safety, Span};
use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::union::TUnion;
use mago_span::HasSpan;
use mago_syntax::cst::{
    Access, ClassLikeConstant, ClassLikeMember, CompositeString, Expression, Literal, Modifier,
    Node, NodeKind, StringPart, UnaryPrefixOperator,
};

use crate::rules::phpdoc::no_useless_var_tag::{Target, is_dead};
use crate::rules::support::class_members::qualified;
use crate::rules::support::docblocks::Doc;
use crate::rules::support::options::{boolean, configure};
use crate::rules::types::compare::classes::with_classes;
use crate::rules::types::compare::mapper::TypeScope;
use crate::rules::types::compare::tags::{Kind, first_named, is_allowed, removed_by_type};
use crate::rules::types::facts::{Facts, facts};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

/// Rector's `AddTypeToConstRector`: types the constants of a class that no child can redeclare (a final class, a
/// private or final constant, or every class under `treat-classes-as-final`) and no parent, interface or trait
/// declares, from their value, and drops the `@var` tag the type makes useless.
#[derive(Default)]
pub struct AddConstType {
    /// Rector's `treatClassesAsFinal()` feature flag: a class that is not abstract counts as final.
    pub treat_classes_as_final: bool,
}

impl Rule for AddConstType {
    fn id(&self) -> &'static str {
        "lint/php-8.3/add-const-type"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80300)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        configure(options, |key, value| {
            match key {
                "treat-classes-as-final" => self.treat_classes_as_final = boolean(value)?,
                _ => return None,
            }
            Some(())
        })
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let facts = facts(ctx);
        let (name, modifiers, members, span) = match node {
            Node::Class(class) => (
                qualified(ancestors, class.name.value),
                &class.modifiers,
                &class.members,
                node_span(&class.name),
            ),
            Node::AnonymousClass(class) => (
                facts.anonymous_class_name(class.span()),
                &class.modifiers,
                &class.members,
                node_span(&class.class),
            ),
            _ => return,
        };
        let has = |f: fn(&Modifier<'_>) -> bool| modifiers.iter().any(f);
        let is_abstract = has(|m| matches!(m, Modifier::Abstract(_)));
        let is_final = has(|m| matches!(m, Modifier::Final(_)));
        let parents = parent_class_likes(&facts, &name);
        let mut path = ancestors.to_vec();
        path.push(node);
        let mut edits = Vec::new();
        for member in members {
            let ClassLikeMember::Constant(constant) = member else {
                continue;
            };
            if constant.hint.is_some() {
                continue;
            }
            let private = constant
                .modifiers
                .iter()
                .any(|m| matches!(m, Modifier::Private(_)));
            let mut value_types: Vec<Option<&str>> = Vec::new();
            for item in &constant.items {
                if is_abstract && !private {
                    continue;
                }
                if self.can_be_inherited(constant, is_abstract, is_final, private) {
                    continue;
                }
                if parents.iter().any(|parent| {
                    parent
                        .constants
                        .keys()
                        .any(|constant| constant.as_bytes() == item.name.value)
                }) {
                    continue;
                }
                let found = find_value_type(&facts, item.value);
                if !value_types.contains(&found) {
                    value_types.push(found);
                }
            }
            let [Some(value_type)] = value_types[..] else {
                continue;
            };
            let first = constant.items.first().expect("a constant declares an item");
            let at = node_span(&first.name).start;
            edits.push(Edit::new(Span::new(at, at), format!("{value_type} ")));
            edits.extend(useless_var_tag(ctx, &facts, &path, constant, value_type));
        }
        if edits.is_empty() {
            return;
        }
        out.push(Report::fixed(
            span,
            "Type the class constants from their values.",
            Safety::Safe,
            edits,
        ));
    }
}

impl AddConstType {
    fn can_be_inherited(
        &self,
        constant: &ClassLikeConstant<'_>,
        is_abstract: bool,
        is_final: bool,
        private: bool,
    ) -> bool {
        if self.treat_classes_as_final && !is_abstract {
            return false;
        }
        let final_constant = constant
            .modifiers
            .iter()
            .any(|m| matches!(m, Modifier::Final(_)));
        !is_final && !private && !final_constant
    }
}

/// The ancestors of the class: its parents, interfaces and traits (PHPStan's `getAncestors()` without the class
/// itself). Each one's constants hold the ones it inherits too, as `hasConstant()` sees them.
fn parent_class_likes<'f>(facts: &'f Facts, name: &str) -> Vec<&'f ClassLikeMetadata> {
    let codebase = &facts.codebase;
    (codebase.get_class_like(name.as_bytes()).into_iter())
        .flat_map(|own| {
            (own.all_parent_classes.iter())
                .chain(&own.all_parent_interfaces)
                .chain(&own.used_traits)
        })
        .filter_map(|ancestor| codebase.get_class_like(ancestor.as_bytes()))
        .collect()
}

fn find_value_type(facts: &Facts, expr: &Expression<'_>) -> Option<&'static str> {
    match unparenthesize(expr) {
        Expression::UnaryPrefix(unary)
            if matches!(
                unary.operator,
                UnaryPrefixOperator::Plus(_) | UnaryPrefixOperator::Negation(_)
            ) =>
        {
            find_value_type(facts, unary.operand)
        }
        Expression::Literal(Literal::String(_)) => Some("string"),
        Expression::CompositeString(CompositeString::Document(document))
            if document
                .parts
                .iter()
                .all(|part| matches!(part, StringPart::Literal(_))) =>
        {
            Some("string")
        }
        Expression::Literal(Literal::Integer(_)) => Some("int"),
        Expression::Literal(Literal::Float(_)) => Some("float"),
        Expression::Literal(Literal::Null(_)) => Some("null"),
        Expression::Literal(Literal::True(_) | Literal::False(_)) => Some("bool"),
        e @ (Expression::ConstantAccess(_) | Expression::Access(Access::ClassConstant(_))) => {
            native_identifier(facts.native_type(node_span(e))?)
        }
        Expression::Array(_) | Expression::LegacyArray(_) => Some("array"),
        Expression::Binary(binary) if binary.operator.is_concatenation() => Some("string"),
        _ => None,
    }
}

/// Rector's `StaticTypeMapper::mapPHPStanTypeToPhpParserNode()` for a constant, when it maps to one identifier: no
/// type for a type that is only `null`, and `bool` for `true` and `false`.
fn native_identifier(t: &TUnion) -> Option<&'static str> {
    if t.is_null() {
        return None;
    }
    let mut found = None;
    for atomic in t.types.iter() {
        let name = match atomic {
            TAtomic::Scalar(s) if s.is_bool() => "bool",
            TAtomic::Scalar(s) if s.is_int() => "int",
            TAtomic::Scalar(s) if s.is_float() => "float",
            TAtomic::Scalar(s) if s.is_any_string() => "string",
            TAtomic::Array(_) => "array",
            _ => return None,
        };
        if found.is_some_and(|f| f != name) {
            return None;
        }
        found = Some(name);
    }
    found
}

/// Rector's `VarTagRemover::removeVarTagIfUseless()` on the constant once it has the type.
fn useless_var_tag(
    ctx: &Context<'_>,
    facts: &Facts,
    path: &[Node<'_, '_>],
    constant: &ClassLikeConstant<'_>,
    value_type: &str,
) -> Option<Edit> {
    // The docblock comes before the attributes, which the constant's span leaves out.
    let start = match constant.attribute_lists.first() {
        Some(list) => node_span(list).start,
        None => node_span(constant).start,
    };
    let doc = Doc::last_of(ctx, path, start)?;
    let (_, tag) = first_named(&doc, "@var")?;
    let (declared, _) = pun_phpstan::phpdoc::parse_prefix(value_type)
        .expect("a type declaration parses as a PHPDoc type");
    let scope = TypeScope::new(path, facts.documented());
    let dead = with_classes(facts.documented(), || {
        is_dead(
            &tag,
            &Target::Mapped(scope.map_doc(&declared)),
            &scope,
            facts,
        )
    });
    if !dead || is_allowed(&tag.ty) {
        return None;
    }
    let removed = removed_by_type(&doc, Kind::Var, &tag.variable);
    Some(doc.remove_last(ctx.source, &removed))
}
