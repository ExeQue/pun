use crate::problem::{Edit, Safety};
use mago_syntax::cst::{ClassLikeConstantSelector, Node, NodeKind};

use super::{assert_class, assert_constant, is_object_type, items, object, string};
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

pub struct Rename {
    pub old_class: String,
    pub old_constant: String,
    pub new_class: Option<String>,
    pub new_constant: String,
}

#[derive(Default)]
pub struct RenameClassConstant {
    pub renames: Vec<Rename>,
}

impl Rule for RenameClassConstant {
    fn id(&self) -> &'static str {
        "lint/refactor/rename-class-constant"
    }

    fn once(&self) -> bool {
        true
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut renames = Vec::new();
        for item in items(options)? {
            let map = object(
                item,
                &["old-class", "old-constant", "new-class", "new-constant"],
                &["old-class", "old-constant", "new-constant"],
            )?;
            let old_class = string(map, "old-class")?;
            let old_constant = string(map, "old-constant")?;
            let new_class = match map.contains_key("new-class") {
                true => Some(string(map, "new-class")?),
                false => None,
            };
            let new_constant = string(map, "new-constant")?;
            assert_class(&old_class)?;
            assert_constant(&old_constant)?;
            if let Some(class) = &new_class {
                assert_class(class)?;
            }
            assert_constant(&new_constant)?;
            renames.push(Rename {
                old_class,
                old_constant,
                new_class,
                new_constant,
            });
        }
        self.renames = renames;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::ClassConstantAccess])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::ClassConstantAccess(fetch) = node else {
            return;
        };
        let ClassLikeConstantSelector::Identifier(constant) = &fetch.constant else {
            return;
        };
        let name = String::from_utf8_lossy(constant.value);
        for rename in &self.renames {
            if !name.eq_ignore_ascii_case(&rename.old_constant)
                || !is_object_type(ctx, ancestors, fetch.class, &rename.old_class)
            {
                continue;
            }
            let (span, text) = match &rename.new_class {
                Some(class) => (
                    node_span(fetch),
                    format!("\\{class}::{}", rename.new_constant),
                ),
                None => (node_span(constant), rename.new_constant.clone()),
            };
            if ctx.source[span.start..span.end] == text {
                return;
            }
            out.push(Report::fixed(
                node_span(fetch),
                format!(
                    "Rename the class constant `{}::{}` to `{}::{}`.",
                    rename.old_class,
                    rename.old_constant,
                    rename.new_class.as_deref().unwrap_or(&rename.old_class),
                    rename.new_constant
                ),
                Safety::Unsafe,
                vec![Edit::new(span, text)],
            ));
            return;
        }
    }
}
