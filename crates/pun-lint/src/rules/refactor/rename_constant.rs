use mago_syntax::cst::{Node, NodeKind};
use pun_problem::{Edit, Safety};

use super::{Kind, assert_constant, is_name, name_of, string_map};
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct RenameConstant {
    pub renames: Vec<(String, String)>,
}

impl Rule for RenameConstant {
    fn id(&self) -> &'static str {
        "lint/refactor/rename-constant"
    }

    fn once(&self) -> bool {
        true
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let renames = string_map(options)?;
        for (old, new) in &renames {
            assert_constant(old)?;
            assert_constant(new)?;
        }
        self.renames = renames;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ConstantAccess])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::ConstantAccess(constant) = node else {
            return;
        };
        let resolved = name_of(ancestors, &constant.name, Kind::Constant);
        let Some((old, new)) = self.renames.iter().find(|(old, _)| is_name(&resolved, old)) else {
            return;
        };
        if resolved == *new {
            return;
        }
        out.push(Report::fixed(
            node_span(constant),
            format!("Rename the constant `{old}` to `{new}`."),
            Safety::Unsafe,
            vec![Edit::new(node_span(&constant.name), new.clone())],
        ));
    }
}
