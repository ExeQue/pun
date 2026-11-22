use crate::problem::{Edit, Safety};
use mago_syntax::cst::{Expression, Node, NodeKind};

use super::{called, fully_qualified, function_name, is_name, string_map};
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct RenameFunction {
    pub renames: Vec<(String, String)>,
}

impl Rule for RenameFunction {
    fn id(&self) -> &'static str {
        "lint/refactor/rename-function"
    }

    fn once(&self) -> bool {
        true
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        self.renames = string_map(options)?;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::FunctionCall, NodeKind::FunctionPartialApplication])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(Expression::Identifier(name)) = called(node) else {
            return;
        };
        let resolved = function_name(ctx, ancestors, name);
        let Some((old, new)) = self.renames.iter().find(|(old, _)| is_name(&resolved, old)) else {
            return;
        };
        let text = match new.contains('\\') {
            true => fully_qualified(new),
            false => new.clone(),
        };
        if text == String::from_utf8_lossy(name.value()) {
            return;
        }
        out.push(Report::fixed(
            node_span(&node),
            format!("Rename the function `{old}()` to `{new}()`."),
            Safety::Unsafe,
            vec![Edit::new(node_span(name), text)],
        ));
    }
}
