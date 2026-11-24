//! rector-laravel 2.7.3's `MigrateToSimplifiedAttributeRector` (laravel-9), its legacy-factories rules
//! `FactoryApplyingStatesRector` and `FactoryFuncCallToStaticCallRector` (laravel-8), and its configurable upgrade
//! rules `AddArgumentDefaultValueRector` and `EventStringToClassConstantRector`.

use mago_syntax::cst::{
    Access, Argument, ArrayElement, Assignment, AssignmentOperator, ClassLikeMember,
    ClassLikeMemberSelector, Expression, Literal, Method, MethodBody, Node, NodeKind, Statement,
    Variable,
};

use super::helper_calls::is_function;
use super::member_name;
use super::object_type::is_object_type;
use super::property_attribute::{ClassView, MODEL, array_elements, report};
use crate::problem::{Edit, Safety, Span};
use crate::rules::refactor::configured_codemods::single_quoted;
use crate::rules::refactor::{assert_class, assert_method, int, object, string};
use crate::rules::support::class_members::remove_members;
use crate::rules::support::expressions::text;
use crate::rules::support::names::resolve_class_name;
use crate::rules::support::statement_edits::{first_comment, line_indent, reindent};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

const ATTRIBUTE: &str = "\\Illuminate\\Database\\Eloquent\\Casts\\Attribute";

/// `MigrateToSimplifiedAttributeRector`.
pub struct PreferAttributeAccessor;

fn attribute_name(name: &str) -> Option<String> {
    let rest = name
        .strip_prefix("get")
        .or_else(|| name.strip_prefix("set"))?;
    let rest = rest.strip_suffix("Attribute")?;
    let mut out = rest.to_string();
    if let Some(first) = out.get_mut(..1) {
        first.make_ascii_lowercase();
    }
    Some(out)
}

fn ucfirst(s: &str) -> String {
    let mut out = s.to_string();
    if let Some(first) = out.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    out
}

fn is_attributes_assignment<'a>(
    assignment: &'a Assignment<'a>,
) -> Option<(Option<&'a Expression<'a>>, &'a Expression<'a>)> {
    let Assignment {
        lhs,
        operator: AssignmentOperator::Assign(_),
        rhs,
        ..
    } = assignment
    else {
        return None;
    };
    let Expression::ArrayAccess(access) = lhs else {
        return None;
    };
    let Expression::Access(Access::Property(p)) = access.array else {
        return None;
    };
    (matches!(p.object, Expression::Variable(Variable::Direct(v)) if v.name == b"$this")
        && member_name(&p.property).is_some_and(|n| n.eq_ignore_ascii_case(b"attributes")))
    .then_some((Some(access.index), *rhs))
}

impl PreferAttributeAccessor {
    /// The closure body lines of an accessor or a mutator, indented with `to`.
    fn body(ctx: &Context<'_>, method: &Method<'_>, mutator: bool, to: &str) -> Vec<String> {
        let source = ctx.source;
        let MethodBody::Concrete(block) = &method.body else {
            return Vec::new();
        };
        let from = line_indent(source, node_span(method).start).to_string() + "    ";
        let mut lines = Vec::new();
        let mut items = Vec::new();
        let mut previous = block.left_brace.end.offset as usize;
        for statement in block.statements.iter() {
            let span = node_span(statement);
            let start = first_comment(source, previous, span.start).unwrap_or(span.start);
            previous = span.end;
            if mutator
                && let Statement::Expression(e) = statement
                && let Expression::Assignment(a) = e.expression
                && is_attributes_assignment(a).is_some()
            {
                continue;
            }
            lines.push(reindent(source, Span::new(start, span.end), &from, to));
        }
        if mutator {
            let mut stack = vec![Node::Block(block)];
            let mut found = Vec::new();
            while let Some(n) = stack.pop() {
                if let Node::Assignment(a) = n
                    && let Some((key, value)) = is_attributes_assignment(a)
                {
                    found.push((
                        node_span(a).start,
                        key.map(|k| text(ctx, k).to_string()),
                        text(ctx, value).to_string(),
                    ));
                }
                n.visit_children(|c| stack.push(c));
            }
            found.sort_by_key(|f| f.0);
            for (_, key, value) in found {
                items.push(match key {
                    Some(k) => format!("{k} => {value}"),
                    None => value,
                });
            }
            lines.push(format!("return [{}];", items.join(", ")));
        }
        lines
    }

    fn closure(ctx: &Context<'_>, method: &Method<'_>, mutator: bool, indent: &str) -> String {
        let inner = format!("{indent}        ");
        let params: Vec<&str> = (method.parameter_list.parameters.iter())
            .map(|p| text(ctx, p))
            .collect();
        let body = Self::body(ctx, method, mutator, &inner);
        let mut out = format!("function ({}) {{", params.join(", "));
        for line in &body {
            out.push('\n');
            out.push_str(&inner);
            out.push_str(line);
        }
        out.push_str(&format!("\n{indent}    }}"));
        out
    }
}

impl Rule for PreferAttributeAccessor {
    fn id(&self) -> &'static str {
        "lint/laravel-9/prefer-attribute-accessor"
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
        let Some(class) = ClassView::of(node) else {
            return;
        };
        if !class.is_object_type(ctx, ancestors, MODEL) {
            return;
        }
        let source = ctx.source;
        let methods: Vec<(usize, &Method<'_>)> = (class.members.iter().enumerate())
            .filter_map(|(i, m)| match m {
                ClassLikeMember::Method(m) => Some((i, m)),
                _ => None,
            })
            .collect();
        let named = |name: &str| {
            methods
                .iter()
                .find(|(_, m)| m.name.value == name.as_bytes())
        };
        let mut removed = vec![false; class.members.len()];
        let mut edits = Vec::new();
        for &(index, method) in &methods {
            if matches!(method.body, MethodBody::Abstract(_)) {
                continue;
            }
            let name = String::from_utf8_lossy(method.name.value).into_owned();
            let Some(attribute) = attribute_name(&name).filter(|a| !a.is_empty()) else {
                continue;
            };
            if methods
                .iter()
                .any(|(_, m)| m.name.value.eq_ignore_ascii_case(attribute.as_bytes()))
            {
                continue;
            }
            let is_mutator = name.starts_with("set");
            let (accessor, mutator) = if is_mutator {
                (
                    named(&format!("get{}Attribute", ucfirst(&attribute))).map(|m| m.1),
                    Some(method),
                )
            } else {
                (
                    Some(method),
                    named(&format!("set{}Attribute", ucfirst(&attribute))).map(|m| m.1),
                )
            };
            if accessor.is_some() && mutator.is_some() && is_mutator {
                removed[index] = true;
                continue;
            }
            let span = node_span(method);
            let indent = line_indent(source, span.start);
            let arguments = match (accessor, mutator) {
                (Some(a), Some(m)) => format!(
                    "get: {}, set: {}",
                    Self::closure(ctx, a, false, indent),
                    Self::closure(ctx, m, true, indent)
                ),
                (Some(a), None) => format!("get: {}", Self::closure(ctx, a, false, indent)),
                (_, m) => format!("set: {}", Self::closure(ctx, m.unwrap(), true, indent)),
            };
            let mut code = String::new();
            if accessor.is_none() || mutator.is_none() {
                for list in method.attribute_lists.iter() {
                    code.push_str(text(ctx, list));
                    code.push('\n');
                    code.push_str(indent);
                }
            }
            code.push_str(&format!(
                "protected function {attribute}(): {ATTRIBUTE}\n{indent}{{\n{indent}    return {ATTRIBUTE}::make({arguments});\n{indent}}}"
            ));
            edits.push(Edit::new(span, code));
        }
        if removed.contains(&true) {
            edits.extend(remove_members(
                source,
                class.members,
                class.left_brace,
                class.right_brace,
                &removed,
            ));
        }
        if !edits.is_empty() {
            out.push(report(
                &class,
                "Define the accessor and mutator as one method that returns an `Attribute`.",
                edits,
            ));
        }
    }
}

/// `FactoryApplyingStatesRector`.
pub struct PreferFactoryStateMethods;

impl Rule for PreferFactoryStateMethods {
    fn id(&self) -> &'static str {
        "lint/laravel-8/prefer-factory-state-methods"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::MethodCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::MethodCall(call) = node else { return };
        if !member_name(&call.method)
            .is_some_and(|n| n.eq_ignore_ascii_case(b"state") || n.eq_ignore_ascii_case(b"states"))
            || !is_object_type(
                ctx,
                ancestors,
                call.object,
                "Illuminate\\Database\\Eloquent\\FactoryBuilder",
            )
        {
            return;
        }
        let args: Vec<&Argument<'_>> = call.argument_list.arguments.iter().collect();
        let string = |e: &Expression<'_>| match unparenthesize(e) {
            Expression::Literal(Literal::String(s)) => {
                s.value.map(|v| String::from_utf8_lossy(v).into_owned())
            }
            _ => None,
        };
        let mut states = Vec::new();
        if let [only] = &args[..]
            && let Some(elements) = array_elements(unparenthesize(only.value()))
        {
            for e in elements.iter() {
                let ArrayElement::Value(v) = e else { return };
                let Some(s) = string(v.value) else { return };
                states.push(s);
            }
        } else {
            for a in &args {
                if matches!(a, Argument::Positional(p) if p.ellipsis.is_some()) {
                    return;
                }
                let Some(s) = string(a.value()) else { return };
                states.push(s);
            }
        }
        if states.iter().any(|s| {
            s.is_empty()
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80)
                || s.as_bytes()[0].is_ascii_digit()
        }) {
            return;
        }
        let calls: String = states.iter().map(|s| format!("->{s}()")).collect();
        out.push(Report::fixed(
            node_span(call),
            "Call the factory's state methods instead of naming the states.",
            Safety::Unsafe,
            vec![Edit::new(
                node_span(call),
                format!("{}{calls}", text(ctx, call.object)),
            )],
        ));
    }
}

/// `FactoryFuncCallToStaticCallRector`.
#[derive(Default)]
pub struct PreferModelFactoryMethod {
    pub models: Vec<String>,
}

pub(crate) fn allow_list(options: &serde_json::Value) -> Result<Vec<String>, OptionsError> {
    let items = crate::rules::refactor::items(options)?;
    items
        .into_iter()
        .map(|v| {
            let name = v.as_str().ok_or("each of `items` must be a string")?;
            assert_class(name)?;
            Ok(name.trim_start_matches('\\').to_string())
        })
        .collect()
}

impl Rule for PreferModelFactoryMethod {
    fn id(&self) -> &'static str {
        "lint/laravel-8/prefer-model-factory-method"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.models = allow_list(options)?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::FunctionCall(call) = node else {
            return;
        };
        if !is_function(ctx, ancestors, call.function, "factory") {
            return;
        }
        let args: Vec<&Argument<'_>> = call.argument_list.arguments.iter().collect();
        let Some(first) = args.first() else { return };
        if matches!(first, Argument::Positional(p) if p.ellipsis.is_some()) {
            return;
        }
        let Expression::Access(Access::ClassConstant(constant)) = first.value() else {
            return;
        };
        if !self.models.is_empty() {
            let Expression::Identifier(id) = constant.class else {
                return;
            };
            let name = resolve_class_name(ancestors, id);
            if !self.models.iter().any(|m| m.eq_ignore_ascii_case(&name)) {
                return;
            }
        }
        let count = args
            .get(1)
            .map_or(String::new(), |a| text(ctx, *a).to_string());
        out.push(Report::fixed(
            node_span(call),
            "Call the model's static `factory()` method instead of the `factory()` helper.",
            Safety::Unsafe,
            vec![Edit::new(
                node_span(call),
                format!("{}::factory({count})", text(ctx, constant.class)),
            )],
        ));
    }
}

/// `AddArgumentDefaultValueRector`.
#[derive(Default)]
pub struct AddArgumentDefault {
    pub arguments: Vec<(String, String, usize, String)>,
}

/// `BuilderHelpers::normalizeValue()` printed as php-parser prints it.
fn php_value(value: &serde_json::Value) -> Result<String, OptionsError> {
    use serde_json::Value;
    Ok(match value {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => single_quoted(s),
        Value::Array(a) => {
            let items: Result<Vec<String>, OptionsError> = a.iter().map(php_value).collect();
            format!("[{}]", items?.join(", "))
        }
        Value::Object(o) => {
            let items: Result<Vec<String>, OptionsError> = o
                .iter()
                .map(|(k, v)| Ok(format!("{} => {}", single_quoted(k), php_value(v)?)))
                .collect();
            format!("[{}]", items?.join(", "))
        }
    })
}

impl Rule for AddArgumentDefault {
    fn id(&self) -> &'static str {
        "lint/refactor/add-argument-default"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut options = options.clone();
        if let Some(map) = options.as_object_mut()
            && let Some(list) = map.remove("added-arguments")
        {
            map.insert("items".into(), list);
        }
        let mut arguments = Vec::new();
        for item in crate::rules::refactor::items(&options)? {
            let keys = ["class", "method", "position", "default-value"];
            let map = object(item, &keys, &keys[..3])?;
            let (class, method) = (string(map, "class")?, string(map, "method")?);
            assert_class(&class)?;
            assert_method(&method)?;
            let position = usize::try_from(int(map, "position")?)
                .map_err(|_| "`position` must not be negative")?;
            let value = php_value(map.get("default-value").unwrap_or(&serde_json::Value::Null))?;
            arguments.push((
                class.trim_start_matches('\\').to_string(),
                method,
                position,
                value,
            ));
        }
        self.arguments = arguments;
        Ok(())
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
        let Some(class) = ClassView::of(node) else {
            return;
        };
        let name = class.name(ctx, ancestors);
        let mut edits: Vec<Edit> = Vec::new();
        for (target, method_name, position, value) in &self.arguments {
            if !super::is_a(ctx, &name, &[target]) {
                continue;
            }
            for member in class.members.iter() {
                let ClassLikeMember::Method(method) = member else {
                    continue;
                };
                if !method
                    .name
                    .value
                    .eq_ignore_ascii_case(method_name.as_bytes())
                {
                    continue;
                }
                let Some(param) = method.parameter_list.parameters.get(*position) else {
                    continue;
                };
                let span = node_span(param);
                if param.default_value.is_some() || edits.iter().any(|e| e.span == span) {
                    continue;
                }
                let variable = String::from_utf8_lossy(param.variable.name);
                edits.push(Edit::new(span, format!("{variable} = {value}")));
            }
        }
        if !edits.is_empty() {
            out.push(report(
                &class,
                "Give the argument its default value.",
                edits,
            ));
        }
    }
}

/// `EventStringToClassConstantRector`.
#[derive(Default)]
pub struct ReplaceEventStringWithConstant {
    pub events: Vec<(String, String, String)>,
}

const EVENT_METHODS: [&str; 8] = [
    "dispatch",
    "fire",
    "flush",
    "forget",
    "hasListeners",
    "listen",
    "push",
    "until",
];

impl ReplaceEventStringWithConstant {
    fn constant(&self, expr: &Expression<'_>) -> Option<String> {
        let Expression::Literal(Literal::String(s)) = expr else {
            return None;
        };
        let value = s.value?;
        let (_, class, constant) = self.events.iter().find(|(e, _, _)| e.as_bytes() == value)?;
        Some(format!("\\{class}::{constant}"))
    }
}

impl Rule for ReplaceEventStringWithConstant {
    fn id(&self) -> &'static str {
        "lint/refactor/replace-event-string-with-constant"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut events = Vec::new();
        for item in crate::rules::refactor::items(options)? {
            let keys = ["string", "class", "constant"];
            let map = object(item, &keys, &keys)?;
            let class = string(map, "class")?;
            assert_class(&class)?;
            events.push((
                string(map, "string")?,
                class.trim_start_matches('\\').to_string(),
                string(map, "constant")?,
            ));
        }
        self.events = events;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::StaticMethodCall,
            NodeKind::MethodCall,
            NodeKind::FunctionCall,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let event_method = |s: &ClassLikeMemberSelector<'_>| {
            matches!(s, ClassLikeMemberSelector::Identifier(id)
                if EVENT_METHODS.iter().any(|m| id.value.eq_ignore_ascii_case(m.as_bytes())))
        };
        let list = match node {
            Node::FunctionCall(f)
                if matches!(f.function, Expression::Identifier(_))
                    && is_function(ctx, ancestors, f.function, "event") =>
            {
                &f.argument_list
            }
            Node::StaticMethodCall(s)
                if event_method(&s.method)
                    && is_object_type(
                        ctx,
                        ancestors,
                        s.class,
                        "Illuminate\\Support\\Facades\\Event",
                    ) =>
            {
                &s.argument_list
            }
            Node::MethodCall(m)
                if event_method(&m.method)
                    && is_object_type(
                        ctx,
                        ancestors,
                        m.object,
                        "Illuminate\\Contracts\\Events\\Dispatcher",
                    ) =>
            {
                &m.argument_list
            }
            _ => return,
        };
        let Some(first) = list.arguments.first() else {
            return;
        };
        if matches!(first, Argument::Positional(p) if p.ellipsis.is_some()) {
            return;
        }
        let value = first.value();
        let mut edits = Vec::new();
        if let Expression::Array(_) | Expression::LegacyArray(_) = value {
            for e in array_elements(value).into_iter().flat_map(|e| e.iter()) {
                if let ArrayElement::Value(v) = e
                    && let Some(constant) = self.constant(v.value)
                {
                    edits.push(Edit::new(node_span(v.value), constant));
                }
            }
        } else if let Some(constant) = self.constant(value) {
            edits.push(Edit::new(node_span(value), constant));
        }
        if !edits.is_empty() {
            out.push(Report::fixed(
                node_span(&node),
                "Name the event by its class instead of a string.",
                Safety::Unsafe,
                edits,
            ));
        }
    }
}
