use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{
    Access, AssignmentOperator, ClassLikeConstantSelector, ClassLikeMember,
    ClassLikeMemberSelector, CompositeString, Expression, MethodBody, Modifier, Node, NodeKind,
    Property, PropertyItem, Statement, StringPart, UnaryPrefixOperator, Variable,
};

use crate::rules::support::class_members::method;
use crate::rules::support::rewrites::{merge, remove_statements};
use crate::rules::support::statement_edits::{block_list, line_indent, reindent};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferReadonlyPropertyDefault;

impl Rule for PreferReadonlyPropertyDefault {
    fn id(&self) -> &'static str {
        "lint/php-8.6/prefer-readonly-property-default"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80600)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class, NodeKind::AnonymousClass])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let members = match node {
            Node::Class(c) => &c.members,
            Node::AnonymousClass(c) => &c.members,
            _ => return,
        };
        let Some(constructor) = method(members, b"__construct") else {
            return;
        };
        let MethodBody::Concrete(block) = &constructor.body else {
            return;
        };
        let source = ctx.source;
        let list = block_list(source, block);
        let mut removed = vec![false; list.stmts.len()];
        let mut edits = Vec::new();
        for member in members {
            let ClassLikeMember::Property(Property::Plain(property)) = member else {
                continue;
            };
            let modifiers = &property.modifiers;
            if !modifiers.iter().any(|m| matches!(m, Modifier::Readonly(_)))
                || modifiers.iter().any(|m| matches!(m, Modifier::Static(_)))
                || property.items.len() != 1
            {
                continue;
            }
            let Some(PropertyItem::Abstract(item)) = property.items.first() else {
                continue;
            };
            let name = &item.variable.name[1..];
            let Some((index, value)) = constant_assign(&list.stmts, name) else {
                continue;
            };
            let code = reindent(
                source,
                node_span(unparenthesize(value)),
                line_indent(source, node_span(list.stmts[index]).start),
                line_indent(source, node_span(property).start),
            );
            let end = node_span(&item.variable).end;
            edits.push(Edit::new(Span::new(end, end), format!(" = {code}")));
            removed[index] = true;
        }
        if edits.is_empty() {
            return;
        }
        edits.extend(remove_statements(ctx, &list, &removed));
        out.push(Report::fixed(
            node_span(constructor),
            "Make these constant constructor assignments the readonly properties' default values.",
            Safety::Unsafe,
            merge(edits),
        ));
    }
}

/// The first top-level `$this->name = ...;` of the constructor, when it assigns a constant value.
fn constant_assign<'a>(
    statements: &[&'a Statement<'a>],
    name: &[u8],
) -> Option<(usize, &'a Expression<'a>)> {
    for (index, statement) in statements.iter().enumerate() {
        let Statement::Expression(e) = statement else {
            continue;
        };
        let Expression::Assignment(assign) = unparenthesize(e.expression) else {
            continue;
        };
        if !matches!(assign.operator, AssignmentOperator::Assign(_)) || assign.rhs.is_reference() {
            continue;
        }
        let Expression::Access(Access::Property(fetch)) = unparenthesize(assign.lhs) else {
            continue;
        };
        let is_this = matches!(fetch.object, Expression::Variable(Variable::Direct(v)) if v.name.eq_ignore_ascii_case(b"$this"));
        let named = matches!(&fetch.property, ClassLikeMemberSelector::Identifier(id) if id.value.eq_ignore_ascii_case(name));
        if !is_this || !named {
            continue;
        }
        return is_constant_value(assign.rhs).then_some((index, assign.rhs));
    }
    None
}

/// Rector's `isConstantValue()`, without the values PHP does not take as a default: a string with interpolated
/// parts, and a class constant fetched from `static` or an expression.
fn is_constant_value(expr: &Expression<'_>) -> bool {
    match unparenthesize(expr) {
        Expression::Literal(_) | Expression::MagicConstant(_) | Expression::ConstantAccess(_) => {
            true
        }
        Expression::CompositeString(CompositeString::Document(d)) => {
            d.parts.iter().all(|p| matches!(p, StringPart::Literal(_)))
        }
        Expression::Access(Access::ClassConstant(c)) => {
            matches!(
                c.class,
                Expression::Identifier(_) | Expression::Self_(_) | Expression::Parent(_)
            ) && matches!(c.constant, ClassLikeConstantSelector::Identifier(_))
        }
        Expression::UnaryPrefix(u) => {
            matches!(
                u.operator,
                UnaryPrefixOperator::Negation(_) | UnaryPrefixOperator::Plus(_)
            ) && is_constant_value(u.operand)
        }
        _ => false,
    }
}
