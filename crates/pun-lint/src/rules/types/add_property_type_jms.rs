use mago_syntax::cst::{
    Access, Attribute, ClassLikeConstantSelector, Expression, Literal, Node, NodeKind,
    PartialArgument, Property,
};
use pun_problem::{Edit, Safety, Span};

use super::facts::facts;
use super::property_type::{ClassView, Prop, VarTag, is_legal, variable_span};
use crate::rules::phpunit::phpdoc_type::TagType;
use crate::rules::support::names::{has_attribute, resolve_class_name};
use crate::syntax::unparenthesize;
use crate::{Context, Report, Rule};

const ATTRIBUTES: u32 = 80000;
const JMS_TYPE: &str = "JMS\\Serializer\\Annotation\\Type";
const COLLECTION_MAPPING: [&str; 8] = [
    "Doctrine\\ORM\\Mapping\\OneToMany",
    "Doctrine\\ORM\\Mapping\\ManyToMany",
    "Doctrine\\ODM\\MongoDB\\Mapping\\Annotations\\ReferenceMany",
    "Doctrine\\ODM\\MongoDB\\Mapping\\Annotations\\EmbedMany",
    "Doctrine\\ORM\\Mapping\\ManyToOne",
    "Doctrine\\ORM\\Mapping\\OneToOne",
    "Doctrine\\ODM\\MongoDB\\Mapping\\Annotations\\ReferenceOne",
    "Doctrine\\ODM\\MongoDB\\Mapping\\Annotations\\EmbedOne",
];

pub struct AddPropertyTypeJmsScalar;

pub struct AddPropertyTypeJmsObject;

impl Rule for AddPropertyTypeJmsScalar {
    fn id(&self) -> &'static str {
        "lint/types/add-property-type-jms-scalar"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(ATTRIBUTES)
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
        visit(node, ancestors, ctx, out, Kind::Scalar);
    }
}

impl Rule for AddPropertyTypeJmsObject {
    fn id(&self) -> &'static str {
        "lint/types/add-property-type-jms-object"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(ATTRIBUTES)
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
        visit(node, ancestors, ctx, out, Kind::Object);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Scalar,
    Object,
}

fn visit<'ast, 'arena>(
    node: Node<'ast, 'arena>,
    ancestors: &[Node<'ast, 'arena>],
    ctx: &Context<'_>,
    out: &mut Vec<Report>,
    kind: Kind,
) {
    let facts = facts(ctx);
    let Some(view) = ClassView::of(&facts, node, ancestors) else {
        return;
    };
    let mut edits: Vec<Edit> = Vec::new();
    for prop in &view.properties() {
        if prop.default().is_some()
            || !has_jms_type(prop, &view.path)
            || !is_legal(&facts, &view, prop, true)
        {
            continue;
        }
        let Some(value) = type_value(prop, &view.path) else {
            continue;
        };
        let lowered = value.to_ascii_lowercase();
        if lowered == "lowercase-string" || lowered == "uppercase-string" {
            return;
        }
        let declaration = match kind {
            Kind::Scalar => {
                let var = VarTag::of(ctx, prop.property, &view.path);
                let doc = match &var.tag {
                    Ok(Some(tag)) => match tag.ty {
                        TagType::Name(n) => Some(n.to_ascii_lowercase()),
                        _ => None,
                    },
                    _ => None,
                };
                let swapped = match (lowered.as_str(), doc.as_deref()) {
                    ("float", Some("string" | "class-string")) => Some("string"),
                    ("string", Some("float" | "double" | "real")) => Some("float"),
                    _ => None,
                };
                match swapped {
                    Some(t) => {
                        let Some(e) = var.remove(ctx.source) else {
                            continue;
                        };
                        edits.extend(e);
                        t.to_string()
                    }
                    None => match scalar_declaration(&lowered) {
                        Some(t) => t.to_string(),
                        None => continue,
                    },
                }
            }
            Kind::Object => {
                if value.contains('<') || scalar_name(&lowered) {
                    continue;
                }
                if value.starts_with('\\')
                    || facts.codebase.get_class_like(value.as_bytes()).is_none()
                {
                    continue;
                }
                format!("\\{value}")
            }
        };
        let at = variable_span(prop);
        edits.push(Edit::new(
            Span::new(at.start, at.start),
            format!("?{declaration} "),
        ));
        edits.push(Edit::new(Span::new(at.end, at.end), " = null"));
    }
    let Some(first) = edits.first() else { return };
    out.push(Report::fixed(
        first.span,
        "Type the properties from their JMS Serializer type.",
        Safety::Unsafe,
        edits,
    ));
}

fn attribute_lists<'a>(
    prop: &Prop<'a, 'a>,
) -> &'a mago_syntax::cst::Sequence<'a, mago_syntax::cst::AttributeList<'a>> {
    match prop.property {
        Property::Plain(p) => &p.attribute_lists,
        Property::Hooked(p) => &p.attribute_lists,
    }
}

fn has_jms_type(prop: &Prop<'_, '_>, path: &[Node<'_, '_>]) -> bool {
    let lists = attribute_lists(prop);
    has_attribute(path, lists, JMS_TYPE)
        && !COLLECTION_MAPPING
            .iter()
            .any(|c| has_attribute(path, lists, c))
}

fn type_value(prop: &Prop<'_, '_>, path: &[Node<'_, '_>]) -> Option<String> {
    let attribute: &Attribute<'_> = attribute_lists(prop)
        .iter()
        .flat_map(|l| l.attributes.iter())
        .find(|a| resolve_class_name(path, &a.name).eq_ignore_ascii_case(JMS_TYPE))?;
    let value = match attribute.argument_list.as_ref()?.arguments.iter().next()? {
        PartialArgument::Positional(p) => p.value,
        PartialArgument::Named(n) => n.value,
        _ => return None,
    };
    let value = match unparenthesize(value) {
        Expression::Literal(Literal::String(s)) => String::from_utf8(s.value?.to_vec()).ok()?,
        Expression::Access(Access::ClassConstant(c)) => match (c.class, &c.constant) {
            (Expression::Identifier(id), ClassLikeConstantSelector::Identifier(k))
                if k.value.eq_ignore_ascii_case(b"class") =>
            {
                resolve_class_name(path, id)
            }
            _ => return None,
        },
        _ => return None,
    };
    if let Some(at) = value.find("DateTime<")
        && value[at..].contains('>')
    {
        return Some("DateTime".into());
    }
    Some(value)
}

fn scalar_name(lowered: &str) -> bool {
    scalar_declaration(lowered).is_some()
        || matches!(
            lowered,
            "null"
                | "void"
                | "resource"
                | "callback"
                | "callable"
                | "never"
                | "never-return"
                | "never-returns"
                | "no-return"
                | "array-key"
                | "lowercase-string"
                | "uppercase-string"
        )
}

fn scalar_declaration(lowered: &str) -> Option<&'static str> {
    Some(match lowered {
        "string"
        | "non-empty-string"
        | "literal-string"
        | "numeric-string"
        | "non-falsy-string"
        | "truthy-string"
        | "class-string"
        | "callable-string"
        | "non-empty-lowercase-string"
        | "non-empty-uppercase-string"
        | "non-empty-literal-string"
        | "decimal-int-string"
        | "non-decimal-int-string" => "string",
        "float" | "real" | "double" => "float",
        "int" | "integer" | "positive-int" | "negative-int" | "non-negative-int"
        | "non-positive-int" => "int",
        "bool" | "boolean" | "true" | "false" => "bool",
        "array" | "non-empty-array" | "list" => "array",
        "iterable" => "iterable",
        "object" => "object",
        _ => return None,
    })
}
