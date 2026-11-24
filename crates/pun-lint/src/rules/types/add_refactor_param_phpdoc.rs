//! Rector 2.8.4's `AddParamTypeToRefactorMethodRector`.

use mago_syntax::cst::{Node, NodeKind};

use crate::problem::{Edit, Safety, Span};
use crate::rules::dead_code::no_redundant_node_instanceof::{
    is_rector_rule, method, node_param_tag, node_types,
};
use crate::rules::support::conditions::indentation;
use crate::rules::support::docblocks::Doc;
use crate::rules::support::names::imported;
use crate::rules::types::facts::facts;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

/// Where Rector prints the union fully qualified (or, with `withImportNames()`, imports each class), Pun writes a
/// class by the name a `use` statement already gives it, else fully qualified.
pub struct AddRefactorParamPhpdoc;

impl Rule for AddRefactorParamPhpdoc {
    fn id(&self) -> &'static str {
        "lint/types/add-refactor-param-phpdoc"
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Class])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Class(class) = node else { return };
        if class.extends.is_none() || !is_rector_rule(ctx, ancestors, class) {
            return;
        }
        let Some(refactor) = method(class, "refactor") else {
            return;
        };
        if refactor.parameter_list.parameters.is_empty() {
            return;
        }
        let mut chain = ancestors.to_vec();
        chain.push(node);
        // only when missing, never over an existing `@param` of `$node`
        if node_param_tag(ctx, &chain, refactor).is_some() {
            return;
        }
        // matchSingleNodeTypesType(): known classes, no interface
        let Some(types) = node_types(ancestors, class) else {
            return;
        };
        if types.is_empty() {
            return;
        }
        let codebase = &facts(ctx).codebase;
        let mut names = Vec::new();
        for name in &types {
            let Some(metadata) = codebase.get_class_like(name.as_bytes()) else {
                return;
            };
            if metadata.kind.is_interface() {
                return;
            }
            let name = metadata.original_name.to_string();
            if !names.iter().any(|n: &String| n.eq_ignore_ascii_case(&name)) {
                names.push(name);
            }
        }
        // TypeCombinator::union() orders the classes by name
        names.sort_by_key(|n| n.to_ascii_lowercase());
        let printed: Vec<String> = names.iter().map(|n| print_class(ancestors, n)).collect();
        let tag = format!("@param {} $node", printed.join("|"));
        let start = node_span(refactor).start;
        let edit = match Doc::of(ctx, &chain, start) {
            Some(doc) => Edit::new(
                doc.span,
                doc.print_appending(&vec![false; doc.len()], &[&tag]),
            ),
            None => {
                let indent = " ".repeat(indentation(ctx.source, start));
                Edit::new(
                    Span::new(start, start),
                    format!("/**\n{indent} * {tag}\n{indent} */\n{indent}"),
                )
            }
        };
        out.push(Report::fixed(
            node_span(&refactor.name),
            "Add an `@param` tag that gives `$node` the types `getNodeTypes()` returns.",
            Safety::Safe,
            vec![edit],
        ));
    }
}

/// The name an import gives the class, else its fully qualified name.
fn print_class(ancestors: &[Node<'_, '_>], name: &str) -> String {
    let short = name.rsplit('\\').next().unwrap_or(name);
    match imported(ancestors, short) {
        Some(full) if full.eq_ignore_ascii_case(name) => short.to_string(),
        _ => format!("\\{name}"),
    }
}
