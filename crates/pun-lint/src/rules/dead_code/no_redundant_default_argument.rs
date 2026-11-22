use crate::problem::{Edit, Fix, Safety, Span};
use mago_syntax::cst::*;

use crate::rules::support::class_members::{
    default_value, element_parts, expr_value, is_private, with_class_info,
};
use crate::rules::support::options::part_on;
use crate::rules::support::rewrites::list_removal;
use crate::rules::types::facts::facts;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct NoRedundantDefaultArgument {
    /// `parent-call` is left out of the rule's `checks`.
    off: bool,
}

impl Rule for NoRedundantDefaultArgument {
    fn id(&self) -> &'static str {
        "lint/dead-code/no-redundant-default-argument"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.off = !checks(options, "parent-call")?;
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
        if self.off {
            return;
        }
        let facts = facts(ctx);
        with_class_info(node, ancestors, &facts, |class| {
            let codebase = &facts.codebase;
            let Some(own) = (class.extends.as_ref())
                .and_then(|_| codebase.get_class_like(class.name.as_bytes()))
            else {
                return;
            };
            let ancestor_names: Vec<&[u8]> = own
                .all_parent_interfaces
                .iter()
                .chain(own.used_traits.iter())
                .chain(own.all_parent_classes.iter())
                .map(mago_word::Word::as_bytes)
                .collect();
            for member in class.members {
                let ClassLikeMember::Method(method) = member else {
                    continue;
                };
                let MethodBody::Concrete(body) = &method.body else {
                    continue;
                };
                if is_private(method) {
                    continue;
                }
                for statement in &body.statements {
                    let Statement::Expression(statement) = statement else {
                        continue;
                    };
                    let Expression::Call(Call::StaticMethod(call)) =
                        unparenthesize(statement.expression)
                    else {
                        continue;
                    };
                    let (Expression::Parent(_), ClassLikeMemberSelector::Identifier(name)) =
                        (call.class, &call.method)
                    else {
                        continue;
                    };
                    let args: Option<Vec<&PositionalArgument<'_>>> =
                        (call.argument_list.arguments.iter())
                            .map(|a| match a {
                                Argument::Positional(a) => Some(a),
                                Argument::Named(_) => None,
                            })
                            .collect();
                    let Some(args) = args.filter(|a| !a.is_empty()) else {
                        continue;
                    };
                    let Some(parent_method) = ancestor_names
                        .iter()
                        .find_map(|a| codebase.get_declaring_method(a, name.value))
                    else {
                        continue;
                    };
                    let params = &parent_method.parameters;
                    let mut kept = args.len();
                    for index in (0..params.len()).rev() {
                        let param = &params[index];
                        if !param.flags.has_default() || param.flags.is_variadic() || index >= kept
                        {
                            break;
                        }
                        let arg = args[index];
                        if index + 1 < kept
                            || arg.ellipsis.is_some()
                            || is_dynamic_expr(arg.value)
                            || expr_value(arg.value)
                                .is_none_or(|v| Some(v) != default_value(&facts, param))
                        {
                            break;
                        }
                        kept = index;
                    }
                    if kept == args.len() {
                        continue;
                    }
                    let spans: Vec<Span> = args.iter().map(|a| node_span(*a)).collect();
                    let removed: Vec<bool> = (0..args.len()).map(|i| i >= kept).collect();
                    let edits = list_removal(&spans, &removed).unwrap_or_else(|| {
                        let list = &call.argument_list;
                        vec![Edit::new(
                            Span::new(
                                list.left_parenthesis.end.offset as usize,
                                list.right_parenthesis.start.offset as usize,
                            ),
                            "",
                        )]
                    });
                    out.push(Report {
                    span: node_span(call),
                    message: format!(
                        "Remove the arguments of `parent::{}()` that equal the parameters' defaults.",
                        String::from_utf8_lossy(name.value)
                    ),
                    help: None,
                    fix: Some(Fix {
                        safety: Safety::Safe,
                        edits,
                    }),
                });
                }
            }
        });
    }
}

fn is_dynamic_expr(expr: &Expression<'_>) -> bool {
    let mut expr = unparenthesize(expr);
    if let Expression::UnaryPrefix(u) = expr
        && matches!(
            u.operator,
            UnaryPrefixOperator::Plus(_) | UnaryPrefixOperator::Negation(_)
        )
    {
        expr = unparenthesize(u.operand);
    }
    match expr {
        Expression::Array(a) => is_dynamic_array(&a.elements),
        Expression::LegacyArray(a) => is_dynamic_array(&a.elements),
        Expression::Literal(_) | Expression::MagicConstant(_) => false,
        Expression::ConstantAccess(_) => false,
        Expression::Access(Access::ClassConstant(c)) => {
            !matches!(
                c.class,
                Expression::Identifier(_) | Expression::Self_(_) | Expression::Parent(_)
            ) || !matches!(c.constant, ClassLikeConstantSelector::Identifier(_))
        }
        _ => true,
    }
}

fn is_dynamic_array(elements: &TokenSeparatedSequence<'_, ArrayElement<'_>>) -> bool {
    elements
        .iter()
        .filter_map(element_parts)
        .any(|(key, value)| {
            key.is_some_and(|k| {
                !matches!(
                    k,
                    Expression::Literal(Literal::String(_) | Literal::Integer(_))
                )
            }) || is_dynamic_expr(value)
        })
}

const CHECKS: [&str; 3] = ["null", "null-named", "parent-call"];

/// Pun's `checks`: the ones that run, one for each of Rector's 3 rules (`null` is
/// `RemoveNullArgOnNullDefaultParamRector`, `null-named` is `RemoveNullNamedArgOnNullDefaultParamRector`, `parent-call`
/// is `RemoveArgumentFromDefaultParentCallRector`).
pub(super) fn checks(options: &serde_json::Value, check: &str) -> Result<bool, OptionsError> {
    part_on(options, "checks", &CHECKS, check)
}
