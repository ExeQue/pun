//! rector-laravel 2.7.3's rules in no set that rewrite one call or statement: `EmptyToBlankAndFilledFuncRector`,
//! `ResponseHelperCallToJsonResponseRector`, `ConfigToTypedConfigMethodCallRector`, `RemoveDumpDataDeadCodeRector`,
//! `UseComponentPropertyWithinCommandsRector` and `RemoveModelPropertyFromFactoriesRector`.

use mago_syntax::cst::{
    Call, ClassLikeMember, Construct, Expression, MethodBody, Node, NodeKind, Property, Statement,
    UnaryPrefixOperator, Variable,
};

use super::property_attribute::ClassView;
use super::{is_name, member_name, replace};
use crate::rules::phpunit::remove_class_members;
use crate::rules::refactor::{RemoveFunctionCall, function_name};
use crate::rules::support::expressions::text;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

fn parent<'n, 'ast, 'arena>(
    node: &Node<'ast, 'arena>,
    ancestors: &'n [Node<'ast, 'arena>],
) -> Option<&'n Node<'ast, 'arena>> {
    let span = node_span(node);
    ancestors.iter().rev().find(|a| {
        node_span(*a) != span && !matches!(a, Node::Expression(_) | Node::Parenthesized(_))
    })
}

pub(super) fn is_function(
    ctx: &Context<'_>,
    ancestors: &[Node<'_, '_>],
    expr: &Expression<'_>,
    wanted: &str,
) -> bool {
    matches!(expr, Expression::Identifier(name) if function_name(ctx, ancestors, name).eq_ignore_ascii_case(wanted))
}

/// `EmptyToBlankAndFilledFuncRector`.
pub struct PreferBlankFilled;

impl Rule for PreferBlankFilled {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-blank-filled"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::EmptyConstruct, NodeKind::UnaryPrefix])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (function, value) = match node {
            Node::UnaryPrefix(u) if matches!(u.operator, UnaryPrefixOperator::Not(_)) => {
                let Expression::Construct(Construct::Empty(empty)) = unparenthesize(u.operand)
                else {
                    return;
                };
                ("filled", empty.value)
            }
            Node::EmptyConstruct(empty) => {
                if let Some(Node::UnaryPrefix(u)) = parent(&node, ancestors)
                    && matches!(u.operator, UnaryPrefixOperator::Not(_))
                {
                    return;
                }
                ("blank", empty.value)
            }
            _ => return,
        };
        out.push(replace(
            node_span(&node),
            format!("Use `{function}()` instead of `empty()`."),
            format!("{function}({})", text(ctx, value)),
        ));
    }
}

/// `ResponseHelperCallToJsonResponseRector`.
pub struct PreferJsonResponse;

impl Rule for PreferJsonResponse {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-json-response"
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
        if !member_name(&call.method).is_some_and(|n| is_name(n, "json")) {
            return;
        }
        let Expression::Call(Call::Function(response)) = call.object else {
            return;
        };
        if !is_function(ctx, ancestors, response.function, "response")
            || !response.argument_list.arguments.is_empty()
        {
            return;
        }
        let mut code = format!(
            "new \\Illuminate\\Http\\JsonResponse{}",
            text(ctx, &call.argument_list)
        );
        let span = node_span(call);
        // php-parser's printer wraps a `new` that is the object of an access or call
        if parent(&node, ancestors).is_some_and(|p| {
            matches!(
                p,
                Node::MethodCall(_)
                    | Node::NullSafeMethodCall(_)
                    | Node::PropertyAccess(_)
                    | Node::NullSafePropertyAccess(_)
                    | Node::ArrayAccess(_)
                    | Node::FunctionCall(_)
            ) && node_span(p).start == span.start
        }) {
            code = format!("({code})");
        }
        out.push(replace(
            span,
            "Use `new JsonResponse()` instead of `response()->json()`.".to_string(),
            code,
        ));
    }
}

/// `ConfigToTypedConfigMethodCallRector`.
pub struct PreferTypedConfig;

impl Rule for PreferTypedConfig {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-typed-config"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::UnaryPrefix])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::UnaryPrefix(cast) = node else {
            return;
        };
        let method = match cast.operator {
            UnaryPrefixOperator::StringCast(..) | UnaryPrefixOperator::BinaryCast(..) => "string",
            UnaryPrefixOperator::IntCast(..) | UnaryPrefixOperator::IntegerCast(..) => "integer",
            UnaryPrefixOperator::BoolCast(..) | UnaryPrefixOperator::BooleanCast(..) => "boolean",
            UnaryPrefixOperator::FloatCast(..)
            | UnaryPrefixOperator::DoubleCast(..)
            | UnaryPrefixOperator::RealCast(..) => "float",
            UnaryPrefixOperator::ArrayCast(..) => "array",
            _ => return,
        };
        let Expression::Call(Call::Function(config)) = unparenthesize(cast.operand) else {
            return;
        };
        if !is_function(ctx, ancestors, config.function, "config")
            || config.argument_list.arguments.len() != 1
        {
            return;
        }
        out.push(replace(
            node_span(cast),
            format!("Use `config()->{method}()` instead of casting `config()`."),
            format!("config()->{method}{}", text(ctx, &config.argument_list)),
        ));
    }
}

/// `RemoveDumpDataDeadCodeRector`: `RemoveFuncCallRector`'s removal, on `dd` and `dump` unless configured.
pub struct NoDumpCalls(RemoveFunctionCall);

impl Default for NoDumpCalls {
    fn default() -> Self {
        NoDumpCalls(RemoveFunctionCall {
            functions: vec!["dd".to_string(), "dump".to_string()],
        })
    }
}

impl Rule for NoDumpCalls {
    fn id(&self) -> &'static str {
        "lint/laravel/no-dump-calls"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        if options.is_null() {
            *self = NoDumpCalls::default();
            return Ok(());
        }
        self.0.configure(options)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        self.0.targets()
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        self.0.visit(node, ancestors, ctx, out);
    }
}

const COMMAND: &str = "Illuminate\\Console\\Command";
const COMPONENT_METHODS: [&str; 9] = [
    "ask",
    "line",
    "info",
    "error",
    "warn",
    "confirm",
    "askWithCompletion",
    "choice",
    "alert",
];

/// `UseComponentPropertyWithinCommandsRector`.
pub struct PreferComponentsProperty;

impl Rule for PreferComponentsProperty {
    fn id(&self) -> &'static str {
        "lint/laravel/prefer-components-property"
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
        let class = ClassView::of(node).unwrap();
        let Some(extends) = class.extends.as_ref().and_then(|e| e.types.first()) else {
            return;
        };
        let parent = crate::rules::support::names::resolve_class_name(ancestors, extends);
        if !super::is_a(ctx, &parent, &[COMMAND]) {
            return;
        }
        for member in class.members {
            let ClassLikeMember::Method(method) = member else {
                continue;
            };
            let MethodBody::Concrete(block) = &method.body else {
                continue;
            };
            for statement in &block.statements {
                let Statement::Expression(e) = statement else {
                    continue;
                };
                let Expression::Call(Call::Method(call)) = e.expression else {
                    continue;
                };
                let this = matches!(call.object, Expression::Variable(Variable::Direct(v)) if v.name == b"$this");
                let named = member_name(&call.method)
                    .is_some_and(|n| COMPONENT_METHODS.iter().any(|m| is_name(n, m)));
                if this && named {
                    out.push(replace(
                        node_span(call.object),
                        "Call the output method on `$this->components`.".to_string(),
                        "$this->components".to_string(),
                    ));
                }
            }
        }
    }
}

const FACTORY: &str = "Illuminate\\Database\\Eloquent\\Factories\\Factory";

/// `RemoveModelPropertyFromFactoriesRector`.
pub struct NoFactoryModelProperty;

impl Rule for NoFactoryModelProperty {
    fn id(&self) -> &'static str {
        "lint/laravel/no-factory-model-property"
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
        let class = ClassView::of(node).unwrap();
        if !class.is_object_type(ctx, ancestors, FACTORY) {
            return;
        }
        let Some(index) = class.members.iter().position(|m| match m {
            ClassLikeMember::Property(p) => {
                let first = match p {
                    Property::Plain(p) => p.items.first(),
                    Property::Hooked(p) => Some(&p.item),
                };
                first.is_some_and(|item| item.variable().name.get(1..) == Some(b"model".as_slice()))
            }
            _ => false,
        }) else {
            return;
        };
        // Rector removes the whole statement, so the other properties it declares would go with it
        if let ClassLikeMember::Property(Property::Plain(p)) = &class.members.nodes[index]
            && p.items.len() != 1
        {
            return;
        }
        let mut removed = vec![false; class.members.len()];
        removed[index] = true;
        out.push(Report::fixed(
            node_span(&class.members.nodes[index]),
            "Remove the `$model` property: the factory resolves its model by name.",
            crate::problem::Safety::Unsafe,
            remove_class_members(ctx.source, node, &removed),
        ));
    }
}
