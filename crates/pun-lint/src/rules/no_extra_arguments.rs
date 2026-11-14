use mago_codex::metadata::CodebaseMetadata;
use mago_codex::metadata::function_like::FunctionLikeMetadata;
use mago_codex::symbol::SymbolKind;
use mago_syntax::cst::{ArgumentList, ClassLikeMemberSelector, Expression, Node, NodeKind};
use pun_problem::{Edit, Fix, Safety, Span};

use super::rector_batch5::{list_removal, resolve_class_name};
use super::refactor::{function_name, object_class_name};
use super::types::class_scope;
use super::types::facts::{facts, is_phpstan_phar_file};
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

pub struct NoExtraArguments;

fn user_maximum(function: &FunctionLikeMetadata) -> Option<usize> {
    let variadic = function.flags.uses_func_get_args()
        || function.parameters.iter().any(|p| p.flags.is_variadic());
    (!variadic).then_some(function.parameters.len())
}

fn method_maximum(codebase: &CodebaseMetadata, class: &str, name: &[u8]) -> Option<usize> {
    let metadata = codebase.get_class_like(class.as_bytes())?;
    let (_, id) = (metadata.appearing_method_ids.iter())
        .find(|(m, _)| m.as_bytes().eq_ignore_ascii_case(name))?;
    let declaring = codebase.get_class_like(id.get_class_name().as_bytes())?;
    if declaring.flags.is_built_in()
        || is_phpstan_phar_file(declaring.span.file_id)
        || declaring.kind == SymbolKind::Interface
    {
        return None;
    }
    let method = codebase.get_declaring_method(id.get_class_name().as_bytes(), name)?;
    if method
        .method_metadata
        .as_ref()
        .is_some_and(|m| m.is_abstract)
    {
        return None;
    }
    user_maximum(method)
}

impl Rule for NoExtraArguments {
    fn id(&self) -> &'static str {
        "lint/functions/no-extra-arguments"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::FunctionCall,
            NodeKind::MethodCall,
            NodeKind::StaticMethodCall,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let facts = facts(ctx);
        let codebase = &facts.codebase;
        let (list, maximum): (&ArgumentList<'_>, Option<usize>) = match node {
            Node::FunctionCall(call) => {
                let Expression::Identifier(name) = call.function else {
                    return;
                };
                let name = function_name(ctx, ancestors, name);
                let maximum = match codebase.get_function(name.as_bytes()) {
                    Some(f) if is_phpstan_phar_file(f.span.file_id) => None,
                    Some(f) if f.flags.is_user_defined() => user_maximum(f),
                    _ => pun_phpstan::symbols::function(&name).and_then(|f| {
                        let variadic = f
                            .variants
                            .iter()
                            .any(|v| v.params.iter().any(|p| p.variadic));
                        (!variadic).then(|| {
                            f.variants
                                .iter()
                                .map(|v| v.params.len())
                                .max()
                                .unwrap_or(usize::MAX)
                        })
                    }),
                };
                (&call.argument_list, maximum)
            }
            Node::MethodCall(call) => {
                let ClassLikeMemberSelector::Identifier(method) = &call.method else {
                    return;
                };
                let maximum = object_class_name(ctx, call.object)
                    .and_then(|class| method_maximum(codebase, &class, method.value));
                (&call.argument_list, maximum)
            }
            Node::StaticMethodCall(call) => {
                let ClassLikeMemberSelector::Identifier(method) = &call.method else {
                    return;
                };
                let class = match unparenthesize(call.class) {
                    Expression::Self_(_) | Expression::Static(_) => {
                        class_scope(ancestors).and_then(|c| c.name)
                    }
                    Expression::Identifier(id) => Some(resolve_class_name(ancestors, id)),
                    _ => return,
                };
                let maximum =
                    class.and_then(|class| method_maximum(codebase, &class, method.value));
                (&call.argument_list, maximum)
            }
            _ => return,
        };
        let Some(maximum) = maximum else {
            return;
        };
        let args: Vec<Span> = list.arguments.iter().map(node_span).collect();
        if args.is_empty() || args.len() <= maximum {
            return;
        }
        let removed: Vec<bool> = (0..args.len()).map(|i| i >= maximum).collect();
        let edits = list_removal(&args, &removed).unwrap_or_else(|| {
            vec![Edit::new(
                Span::new(
                    list.left_parenthesis.end.offset as usize,
                    list.right_parenthesis.start.offset as usize,
                ),
                "",
            )]
        });
        out.push(Report {
            span: node_span(&node),
            message: format!(
                "Remove the arguments after the {maximum} the {} takes.",
                if matches!(node, Node::FunctionCall(_)) {
                    "function"
                } else {
                    "method"
                }
            ),
            help: None,
            fix: Some(Fix {
                safety: Safety::Safe,
                edits,
            }),
        });
    }
}
