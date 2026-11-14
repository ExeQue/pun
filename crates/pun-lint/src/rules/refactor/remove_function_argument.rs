use mago_syntax::cst::{Expression, Node, NodeKind};
use pun_problem::{Edit, Safety, Span};

use super::{Kind, assert_function, called, int, is_name, items, name_of, object, string};
use crate::rules::support::rewrites::list_removal;
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

#[derive(Default)]
pub struct RemoveFunctionArgument {
    pub removals: Vec<(String, i64)>,
}

impl Rule for RemoveFunctionArgument {
    fn id(&self) -> &'static str {
        "lint/refactor/remove-function-argument"
    }

    fn once(&self) -> bool {
        true
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), String> {
        let mut removals = Vec::new();
        for item in items(options)? {
            let map = object(
                item,
                &["function", "argument-position"],
                &["function", "argument-position"],
            )?;
            let function = string(map, "function")?;
            assert_function(&function)?;
            removals.push((function, int(map, "argument-position")?));
        }
        self.removals = removals;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall, NodeKind::FunctionPartialApplication])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(Expression::Identifier(name)) = called(node) else {
            return;
        };
        if self.removals.is_empty() {
            return;
        }
        let resolved = name_of(ancestors, name, Kind::Function);
        let list = node.children().pop().unwrap_or(node);
        let args: Vec<Span> = list.children().iter().map(node_span).collect();
        let outer = node_span(&list);
        let inner = Span::new(outer.start + 1, outer.end - 1);
        let mut removed = vec![false; args.len()];
        for (function, position) in &self.removals {
            if is_name(&resolved, function)
                && let Ok(at) = usize::try_from(*position)
                && at < args.len()
            {
                removed[at] = true;
            }
        }
        if !removed.contains(&true) {
            return;
        }
        let edits = list_removal(&args, &removed).unwrap_or_else(|| vec![Edit::new(inner, "")]);
        let positions: Vec<String> = (removed.iter().enumerate())
            .filter(|(_, r)| **r)
            .map(|(i, _)| i.to_string())
            .collect();
        out.push(Report::fixed(
            node_span(&node),
            format!(
                "Remove the argument at position {} of `{resolved}()`.",
                positions.join(", ")
            ),
            Safety::Unsafe,
            edits,
        ));
    }
}
