//! Rector 2.8.4's `ReadOnlyPropertyRector`, with `PropertyManipulator::isPropertyChangeableExceptConstructor()`,
//! `PropertyFetchFinder::findPrivatePropertyFetches()`, `ContextAnalyzer`,
//! `PropertyFetchAssignManipulator::isAssignedMultipleTimesInConstructor()` and `ParamAnalyzer::isParamReassign()`.

use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_syntax::cst::{
    Access, Argument, ArgumentList, AssignmentOperator, ClassLikeMember, ClassLikeMemberSelector,
    Expression, ForeachTarget, FunctionLikeParameter, Identifier, Method, MethodBody, Modifier,
    Node, NodeKind, Property, PropertyItem, Sequence, UnaryPostfixOperator, UnaryPrefixOperator,
    Variable,
};

use crate::problem::{Edit, Safety, Span};
use crate::rules::dead_code::no_redundant_property_default::is_property_assigned;
use crate::rules::support::class_members::{object_class_names, type_of, walk};
use crate::rules::support::docblocks::Doc;
use crate::rules::support::names::{has_attribute, imported, resolve_class_name};
use crate::rules::types::add_property_type_constructor::is_test_case;
use crate::rules::types::facts::{Facts, facts};
use crate::rules::types::property_type::ClassView;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct PreferReadonlyProperty;

impl Rule for PreferReadonlyProperty {
    fn id(&self) -> &'static str {
        "lint/php-8.1/prefer-readonly-property"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80100)
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
        let facts = facts(ctx);
        let Some(view) = ClassView::of(&facts, node, ancestors) else {
            return;
        };
        let class_modifiers = match node {
            Node::Class(class) => &class.modifiers,
            Node::AnonymousClass(class) => &class.modifiers,
            _ => return,
        };
        if should_skip(&view, class_modifiers) {
            return;
        }
        let class = Class {
            ctx,
            facts: &facts,
            view: &view,
        };
        if let Some(constructor) = view.method(b"__construct") {
            let mut path = view.path.clone();
            path.push(Node::Method(constructor));
            path.push(Node::FunctionLikeParameterList(&constructor.parameter_list));
            for param in &constructor.parameter_list.parameters {
                if class.refactor_param(constructor, param) {
                    out.push(report(ctx, &path, Node::FunctionLikeParameter(param)));
                }
            }
        }
        for member in view.members {
            let ClassLikeMember::Property(property) = member else {
                continue;
            };
            if class.refactor_property(property) {
                out.push(report(ctx, &view.path, Node::Property(property)));
            }
        }
    }
}

fn report(ctx: &Context<'_>, path: &[Node<'_, '_>], node: Node<'_, '_>) -> Report {
    let (modifiers, next, span) = match node {
        Node::FunctionLikeParameter(param) => (
            &param.modifiers,
            (param.hint.as_ref().map(|h| node_span(h).start))
                .or(param.ampersand.map(|s| s.start.offset as usize))
                .unwrap_or(node_span(&param.variable).start),
            node_span(&param.variable),
        ),
        Node::Property(property) => (
            property.modifiers(),
            (property.hint()).map_or(node_span(property.first_variable()).start, |h| {
                node_span(h).start
            }),
            node_span(property.first_variable()),
        ),
        _ => unreachable!("a parameter or a property"),
    };
    let mut edits = vec![make_readonly(modifiers, next)];
    edits.extend(remove_readonly_doc(ctx, path, node_span(&node).start));
    Report::fixed(
        span,
        "Make the property `readonly`: nothing writes it after the constructor.",
        Safety::Unsafe,
        edits,
    )
}

/// `VisibilityManipulator::makeReadonly()`: PHP-Parser reprints the changed modifiers in its own order.
fn make_readonly(modifiers: &Sequence<'_, Modifier<'_>>, next: usize) -> Edit {
    const TEXT: [&str; 10] = [
        "final ",
        "abstract ",
        "public ",
        "protected ",
        "private ",
        "public(set) ",
        "protected(set) ",
        "private(set) ",
        "static ",
        "readonly ",
    ];
    let rank = |m: &Modifier<'_>| match m {
        Modifier::Final(_) => 0,
        Modifier::Abstract(_) => 1,
        Modifier::Public(_) => 2,
        Modifier::Protected(_) => 3,
        Modifier::Private(_) => 4,
        Modifier::PublicSet(_) => 5,
        Modifier::ProtectedSet(_) => 6,
        Modifier::PrivateSet(_) => 7,
        Modifier::Static(_) => 8,
        Modifier::Readonly(_) => 9,
    };
    let mut ranks: Vec<usize> = modifiers.iter().map(rank).chain([9]).collect();
    ranks.sort_unstable();
    ranks.dedup();
    let code: String = ranks.iter().map(|&r| TEXT[r]).collect();
    let start = modifiers.first().map_or(next, |m| node_span(m).start);
    Edit::new(Span::new(start, next), code)
}

/// `removeReadOnlyDoc()`: drops a bare `@readonly` tag, and the docblock when nothing else is left.
fn remove_readonly_doc(ctx: &Context<'_>, path: &[Node<'_, '_>], start: usize) -> Option<Edit> {
    let doc = Doc::of(ctx, path, start)?;
    let words: Vec<Vec<&str>> = (0..doc.len()).map(|i| doc.words(i)).collect();
    let first = words.iter().position(|w| w.first() == Some(&"@readonly"))?;
    if words[first].len() != 1 {
        return None;
    }
    let removed: Vec<bool> = (0..words.len()).map(|i| i == first).collect();
    Some(doc.remove(&removed, start))
}

fn should_skip(view: &ClassView<'_, '_>, modifiers: &Sequence<'_, Modifier<'_>>) -> bool {
    if modifiers.iter().any(|m| matches!(m, Modifier::Readonly(_))) {
        return true;
    }
    // not safe
    if (view.members.iter()).any(|m| matches!(m, ClassLikeMember::TraitUse(_))) {
        return true;
    }
    // skip "clone $this" cases, as can create unexpected write to local constructor property
    find(
        view.node,
        |n, _| matches!(n, Node::Clone(c) if is_this(c.object)),
    )
}

/// `BetterNodeFinder::findFirst()` over every node below `root`.
fn find<'ast, 'arena>(
    root: Node<'ast, 'arena>,
    mut f: impl FnMut(Node<'ast, 'arena>, &[Node<'ast, 'arena>]) -> bool,
) -> bool {
    let mut found = false;
    walk(root, &mut |n, path| {
        found = found || f(n, path);
        !found
    });
    found
}

fn is_this(expr: &Expression<'_>) -> bool {
    matches!(unparenthesize(expr), Expression::Variable(Variable::Direct(v)) if v.name == b"$this")
}

fn identifier<'a>(selector: &ClassLikeMemberSelector<'a>) -> Option<&'a [u8]> {
    match selector {
        ClassLikeMemberSelector::Identifier(id) => Some(id.value),
        _ => None,
    }
}

fn static_name<'a>(property: &Variable<'a>) -> Option<&'a [u8]> {
    match property {
        Variable::Direct(v) => v.name.get(1..),
        _ => None,
    }
}

/// `$this->name`.
fn is_this_property(expr: &Expression<'_>, name: &[u8]) -> bool {
    matches!(
        unparenthesize(expr),
        Expression::Access(Access::Property(fetch))
            if is_this(fetch.object) && identifier(&fetch.property) == Some(name)
    )
}

fn has_modifier(modifiers: &Sequence<'_, Modifier<'_>>, pick: fn(&Modifier<'_>) -> bool) -> bool {
    modifiers.iter().any(pick)
}

fn plain_assign<'a>(node: Node<'a, 'a>) -> Option<&'a mago_syntax::cst::Assignment<'a>> {
    match node {
        Node::Assignment(a)
            if matches!(a.operator, AssignmentOperator::Assign(_))
                && !matches!(a.rhs, Expression::UnaryPrefix(u) if u.operator.is_reference()) =>
        {
            Some(a)
        }
        _ => None,
    }
}

fn statements<'a>(method: &'a Method<'a>) -> Option<Node<'a, 'a>> {
    match &method.body {
        MethodBody::Concrete(block) => Some(Node::Block(block)),
        MethodBody::Abstract(_) => None,
    }
}

const ALLOWED_NOT_READONLY_CLASS_ANNOTATIONS: [&str; 10] = [
    "ApiPlatform\\Core\\Annotation\\ApiResource",
    "ApiPlatform\\Metadata\\ApiResource",
    "Doctrine\\ORM\\Mapping\\Entity",
    "Doctrine\\ORM\\Mapping\\Table",
    "Doctrine\\ORM\\Mapping\\MappedSuperclass",
    "Doctrine\\ORM\\Mapping\\Embeddable",
    // Deprecated in ODM 2.16
    "Doctrine\\ODM\\MongoDB\\Mapping\\Annotations\\Document",
    "Doctrine\\ODM\\MongoDB\\Mapping\\Annotations\\EmbeddedDocument",
    // New in ODM 2.16
    "Doctrine\\ODM\\MongoDB\\Mapping\\Attribute\\Document",
    "Doctrine\\ODM\\MongoDB\\Mapping\\Attribute\\EmbeddedDocument",
];

const JMS_TYPE: &str = "JMS\\Serializer\\Annotation\\Type";

struct Class<'c, 'ast, 'arena> {
    ctx: &'c Context<'c>,
    facts: &'c Facts,
    view: &'c ClassView<'ast, 'arena>,
}

impl<'ast, 'arena> Class<'_, 'ast, 'arena> {
    fn refactor_property(&self, property: &'ast Property<'arena>) -> bool {
        // 1. is property read-only?
        let modifiers = property.modifiers();
        if has_modifier(modifiers, |m| matches!(m, Modifier::Readonly(_))) {
            return false;
        }
        let Property::Plain(plain) = property else {
            return false;
        };
        let Some(first) = plain.items.first() else {
            return false;
        };
        if matches!(first, PropertyItem::Concrete(_)) || plain.hint.is_none() {
            return false;
        }
        if has_modifier(modifiers, |m| matches!(m, Modifier::Static(_)))
            || !has_modifier(modifiers, |m| matches!(m, Modifier::Private(_)))
        {
            return false;
        }
        let name = &first.variable().name[1..];
        if self.is_property_changeable_except_constructor(&plain.attribute_lists, name) {
            return false;
        }
        if self.is_assigned_multiple_times_in_constructor(name) {
            return false;
        }
        !self.is_property_returned_by_ref(name) && !self.is_property_changed_in_by_ref_foreach(name)
    }

    fn refactor_param(
        &self,
        constructor: &'ast Method<'arena>,
        param: &'ast FunctionLikeParameter<'arena>,
    ) -> bool {
        if !has_modifier(&param.modifiers, |m| matches!(m, Modifier::Private(_))) {
            return false;
        }
        if param.hint.is_none() {
            return false;
        }
        // early check not property promotion and already readonly
        if has_modifier(&param.modifiers, |m| matches!(m, Modifier::Readonly(_))) {
            return false;
        }
        if param.hooks.is_some() {
            return false;
        }
        let name = &param.variable.name[1..];
        if self.is_property_changeable_except_constructor(&param.attribute_lists, name) {
            return false;
        }
        if param.ampersand.is_some() {
            return false;
        }
        if is_param_reassign(constructor, param.variable.name) {
            return false;
        }
        if self.is_promoted_property_assigned(name) {
            return false;
        }
        !self.is_property_returned_by_ref(name) && !self.is_property_changed_in_by_ref_foreach(name)
    }

    fn is_property_changeable_except_constructor(
        &self,
        attributes: &Sequence<'_, mago_syntax::cst::AttributeList<'_>>,
        name: &[u8],
    ) -> bool {
        if self.has_allowed_not_readonly_annotation_or_attribute() {
            return true;
        }
        if has_attribute(&self.view.path, attributes, JMS_TYPE) {
            return true;
        }
        let constructor = self.view.method(b"__construct");
        let mut assigned_only_in_constructor = None;
        find(self.view.node, |node, path| {
            if !self.is_private_property_fetch(node, path, name) {
                return false;
            }
            let logical = logical(path);
            if self.is_changeable_context(node, &logical) {
                return true;
            }
            // skip for constructor? it is allowed to set value in constructor method
            if constructor.is_some_and(|c| {
                path.iter()
                    .any(|n| matches!(n, Node::Method(m) if std::ptr::eq(*m, c)))
            }) && *assigned_only_in_constructor.get_or_insert_with(|| {
                is_property_assigned(
                    self.ctx,
                    self.view,
                    is_test_case(self.facts, self.view),
                    name,
                )
            }) {
                return false;
            }
            is_left_part_of_assign(node, &logical)
        })
    }

    fn has_allowed_not_readonly_annotation_or_attribute(&self) -> bool {
        let path = &self.view.path[..self.view.path.len() - 1];
        let attributes = match self.view.node {
            Node::Class(class) => &class.attribute_lists,
            Node::AnonymousClass(class) => &class.attribute_lists,
            _ => return false,
        };
        if (ALLOWED_NOT_READONLY_CLASS_ANNOTATIONS.iter())
            .any(|c| has_attribute(path, attributes, c))
        {
            return true;
        }
        let Some(doc) = Doc::of(self.ctx, path, node_span(&self.view.node).start) else {
            return false;
        };
        doc.text_all().split('@').skip(1).any(|tag| {
            let len = tag
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '\\'))
                .unwrap_or(tag.len());
            let written = &tag[..len];
            let resolved = match written.strip_prefix('\\') {
                Some(fq) => fq.to_string(),
                None => imported(path, written).unwrap_or_else(|| {
                    match crate::rules::types::namespace_of(path) {
                        Some(ns) => format!("{ns}\\{written}"),
                        None => written.to_string(),
                    }
                }),
            };
            (ALLOWED_NOT_READONLY_CLASS_ANNOTATIONS.iter())
                .any(|c| c.eq_ignore_ascii_case(&resolved))
        })
    }

    /// `findPropertyFetchesInClassLike()`: a fetch of the property on an object of this class, or any static fetch
    /// of that name.
    fn is_private_property_fetch(
        &self,
        node: Node<'_, '_>,
        path: &[Node<'_, '_>],
        name: &[u8],
    ) -> bool {
        match node {
            Node::PropertyAccess(fetch) => {
                if identifier(&fetch.property) != Some(name) {
                    return false;
                }
                // a fetch inside an anonymous class belongs to that class
                if path[1..]
                    .iter()
                    .any(|n| matches!(n, Node::Class(_) | Node::AnonymousClass(_)))
                {
                    return false;
                }
                if is_this(fetch.object) {
                    return true;
                }
                self.is_object_of_this_class(fetch.object)
            }
            Node::StaticPropertyAccess(fetch) => static_name(&fetch.property) == Some(name),
            _ => false,
        }
    }

    /// The object's type is this class (an `ObjectType` or `StaticType` naming it).
    fn is_object_of_this_class(&self, object: &Expression<'_>) -> bool {
        let Some(t) = type_of(self.facts, object) else {
            return false;
        };
        let [atomic] = t.types.as_ref() else {
            return false;
        };
        matches!(atomic, TAtomic::Object(TObject::Named(_)))
            && object_class_names(t)
                .first()
                .is_some_and(|n| n.eq_ignore_ascii_case(&self.view.name))
    }

    /// `ContextAnalyzer::isChangeableContext()`.
    fn is_changeable_context(&self, node: Node<'_, '_>, path: &[Node<'_, '_>]) -> bool {
        let span = node_span(&node);
        let Some(parent) = path.last() else {
            return false;
        };
        match parent {
            Node::Unset(_) => true,
            Node::ArrayAccess(a) => node_span(a.array) == span,
            Node::ArrayAppend(a) => node_span(a.array) == span,
            Node::UnaryPrefix(u) => matches!(
                u.operator,
                UnaryPrefixOperator::PreIncrement(_) | UnaryPrefixOperator::PreDecrement(_)
            ),
            Node::UnaryPostfix(u) => matches!(
                u.operator,
                UnaryPostfixOperator::PostIncrement(_) | UnaryPostfixOperator::PostDecrement(_)
            ),
            Node::PositionalArgument(_) | Node::NamedArgument(_) => {
                self.is_used_as_arg_by_ref_value(span, path)
            }
            _ => false,
        }
    }

    /// `PropertyFetchFinder::decoratePropertyFetch()`: the fetch is an argument the called function or method takes
    /// by reference.
    fn is_used_as_arg_by_ref_value(&self, span: Span, path: &[Node<'_, '_>]) -> bool {
        let [.., call, Node::ArgumentList(list), _] = path else {
            return false;
        };
        let Some(key) = position(list, span) else {
            return false;
        };
        let parameters = match call {
            Node::FunctionCall(call) => self.function_parameters(path, call.function),
            Node::MethodCall(call) => identifier(&call.method).and_then(|method| {
                let t = type_of(self.facts, call.object)?;
                (object_class_names(t).iter())
                    .find_map(|class| self.method_parameters(class, method))
            }),
            Node::StaticMethodCall(call) => identifier(&call.method).and_then(|method| {
                let class = self.static_class(path, call.class)?;
                self.method_parameters(&class, method)
            }),
            _ => None,
        };
        parameters.is_some_and(|p| p.get(key).copied().unwrap_or(false))
    }

    fn function_parameters(
        &self,
        path: &[Node<'_, '_>],
        function: &Expression<'_>,
    ) -> Option<Vec<bool>> {
        let ancestors = [&self.view.path[..], path].concat();
        let name = crate::rules::support::names::called_function_name(&ancestors, function)?;
        let local = matches!(function, Expression::Identifier(Identifier::Local(_)));
        let namespaced = (crate::rules::types::names::class_imports(&ancestors).0)
            .filter(|_| local && !name.contains('\\'));
        let codebase = &self.facts.codebase;
        let function = namespaced
            .and_then(|ns| codebase.get_function(format!("{ns}\\{name}").as_bytes()))
            .or_else(|| codebase.get_function(name.as_bytes()))?;
        Some(
            (function.parameters.iter())
                .map(|p| p.flags.is_by_reference())
                .collect(),
        )
    }

    fn method_parameters(&self, class: &str, method: &[u8]) -> Option<Vec<bool>> {
        let method = self
            .facts
            .codebase
            .get_declaring_method(class.as_bytes(), method)?;
        Some(
            (method.parameters.iter())
                .map(|p| p.flags.is_by_reference())
                .collect(),
        )
    }

    fn static_class(&self, path: &[Node<'_, '_>], class: &Expression<'_>) -> Option<String> {
        let Expression::Identifier(id) = class else {
            return None;
        };
        let written = String::from_utf8_lossy(id.value());
        if written.eq_ignore_ascii_case("self") || written.eq_ignore_ascii_case("static") {
            return Some(self.view.name.clone());
        }
        if written.eq_ignore_ascii_case("parent") {
            return self.view.parent.clone();
        }
        let ancestors = [&self.view.path[..], path].concat();
        Some(resolve_class_name(&ancestors, id))
    }

    /// `PropertyFetchAssignManipulator::isAssignedMultipleTimesInConstructor()`.
    fn is_assigned_multiple_times_in_constructor(&self, name: &[u8]) -> bool {
        let Some(body) = self.view.method(b"__construct").and_then(statements) else {
            return false;
        };
        let mut count = 0;
        walk(body, &mut |node, _| {
            if count == 2 {
                return false;
            }
            // skip anonymous classes and inner function
            if matches!(
                node,
                Node::Class(_) | Node::AnonymousClass(_) | Node::Function(_)
            ) {
                return false;
            }
            if let Node::Assignment(a) = node
                && !matches!(a.rhs, Expression::UnaryPrefix(u) if u.operator.is_reference())
                && self.is_local_property_fetch_name(a.lhs, name)
            {
                count += 1;
            }
            true
        });
        count == 2
    }

    /// `PropertyFetchAnalyzer::isLocalPropertyFetchName()`.
    fn is_local_property_fetch_name(&self, expr: &Expression<'_>, name: &[u8]) -> bool {
        match unparenthesize(expr) {
            Expression::Access(Access::Property(fetch)) => {
                identifier(&fetch.property) == Some(name)
                    && (is_this(fetch.object) || self.is_object_of_this_class(fetch.object))
            }
            Expression::Access(Access::NullSafeProperty(fetch)) => {
                identifier(&fetch.property) == Some(name)
                    && (is_this(fetch.object) || self.is_object_of_this_class(fetch.object))
            }
            Expression::Access(Access::StaticProperty(fetch)) => {
                static_name(&fetch.property) == Some(name)
                    && self
                        .static_class(&[], fetch.class)
                        .is_some_and(|c| c.eq_ignore_ascii_case(&self.view.name))
            }
            _ => false,
        }
    }

    /// `isPromotedPropertyAssigned()`: an assign to `$this->name` anywhere in the class.
    fn is_promoted_property_assigned(&self, name: &[u8]) -> bool {
        find(self.view.node, |node, _| {
            plain_assign(node).is_some_and(|a| is_this_property(a.lhs, name))
        })
    }

    /// `isPropertyReturnedByRef()`: a method returning by reference returns an expression holding `$this->name`.
    fn is_property_returned_by_ref(&self, name: &[u8]) -> bool {
        self.view.members.iter().any(|member| {
            let ClassLikeMember::Method(method) = member else {
                return false;
            };
            method.ampersand.is_some()
                && find(Node::Method(method), |node, _| {
                    let Node::Return(r#return) = node else {
                        return false;
                    };
                    r#return.value.is_some_and(|value| {
                        is_this_property(value, name)
                            || find(Node::Expression(value), |n, _| {
                                matches!(n, Node::Expression(e) if is_this_property(e, name))
                            })
                    })
                })
        })
    }

    /// `isPropertyChangedInByRefForeach()`: `foreach ($this->name as &$item)`.
    fn is_property_changed_in_by_ref_foreach(&self, name: &[u8]) -> bool {
        find(self.view.node, |node, _| {
            let Node::Foreach(foreach) = node else {
                return false;
            };
            let value = match &foreach.target {
                ForeachTarget::Value(target) => target.value,
                ForeachTarget::KeyValue(target) => target.value,
            };
            matches!(value, Expression::UnaryPrefix(u) if u.operator.is_reference())
                && is_this_property(foreach.expression, name)
        })
    }
}

/// The ancestors PHP-Parser has: without Mago's wrapper nodes and parentheses.
fn logical<'ast, 'arena>(path: &[Node<'ast, 'arena>]) -> Vec<Node<'ast, 'arena>> {
    (path.iter())
        .filter(|n| {
            !matches!(
                n,
                Node::Expression(_)
                    | Node::Access(_)
                    | Node::Call(_)
                    | Node::Argument(_)
                    | Node::ArrayElement(_)
                    | Node::Statement(_)
                    | Node::Construct(_)
                    | Node::Parenthesized(_)
            )
        })
        .copied()
        .collect()
}

fn position(list: &ArgumentList<'_>, span: Span) -> Option<usize> {
    list.arguments.iter().position(|a| {
        let value = match a {
            Argument::Positional(a) => a.value,
            Argument::Named(a) => a.value,
        };
        node_span(unparenthesize(value)) == span
    })
}

/// `ContextAnalyzer::isLeftPartOfAssign()`: the assigned variable of an assign (or an item of a destructuring
/// list), the expression of an assign by reference, or the variable of a compound assign.
fn is_left_part_of_assign(node: Node<'_, '_>, path: &[Node<'_, '_>]) -> bool {
    let span = node_span(&node);
    match path {
        // the variable of an assign by reference is not marked
        [.., Node::Assignment(a)] => {
            node_span(unparenthesize(a.lhs)) == span
                && !matches!(a.rhs, Expression::UnaryPrefix(u) if u.operator.is_reference())
        }
        [.., Node::Assignment(a), Node::UnaryPrefix(u)] => {
            u.operator.is_reference() && node_span(a.rhs) == node_span(*u)
        }
        [
            ..,
            Node::Assignment(a),
            list @ (Node::Array(_) | Node::List(_)),
            Node::ValueArrayElement(_) | Node::KeyValueArrayElement(_),
        ] => {
            matches!(a.operator, AssignmentOperator::Assign(_))
                && node_span(a.lhs) == node_span(list)
        }
        _ => false,
    }
}

/// `ParamAnalyzer::isParamReassign()`: an assign to the parameter variable in the method, outside nested functions
/// and classes.
fn is_param_reassign(method: &Method<'_>, variable: &[u8]) -> bool {
    let Some(body) = statements(method) else {
        return false;
    };
    let mut found = false;
    walk(body, &mut |node, _| {
        if found {
            return false;
        }
        if let Some(a) = plain_assign(node)
            && matches!(unparenthesize(a.lhs), Expression::Variable(Variable::Direct(v)) if v.name == variable)
        {
            found = true;
            return false;
        }
        !matches!(
            node,
            Node::Class(_)
                | Node::AnonymousClass(_)
                | Node::Function(_)
                | Node::Closure(_)
                | Node::ArrowFunction(_)
        )
    });
    found
}
