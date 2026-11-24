//! Rector 2.8.4's `StaticCallToFuncCallRector`.

use mago_syntax::cst::{ClassLikeMemberSelector, Node, NodeKind};

use super::{assert_class, assert_function, assert_method, is_object_type, items, object, string};
use crate::problem::{Edit, Safety};
use crate::rules::support::expressions::text;
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

pub struct StaticToFunction {
    pub class: String,
    pub method: String,
    pub function: String,
}

#[derive(Default)]
pub struct ReplaceStaticWithFunction {
    pub items: Vec<StaticToFunction>,
}

const KEYS: [&str; 3] = ["class", "method", "function"];

impl Rule for ReplaceStaticWithFunction {
    fn id(&self) -> &'static str {
        "lint/refactor/replace-static-with-function"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut out = Vec::new();
        for item in items(options)? {
            let map = object(item, &KEYS, &KEYS)?;
            let item = StaticToFunction {
                class: string(map, "class")?,
                method: string(map, "method")?,
                function: string(map, "function")?,
            };
            assert_class(&item.class)?;
            assert_method(&item.method)?;
            assert_function(&item.function)?;
            out.push(item);
        }
        self.items = out;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[
            NodeKind::StaticMethodCall,
            NodeKind::StaticMethodPartialApplication,
        ])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let (class, method, args) = match node {
            Node::StaticMethodCall(call) => {
                (call.class, &call.method, text(ctx, &call.argument_list))
            }
            Node::StaticMethodPartialApplication(p) => {
                (p.class, &p.method, text(ctx, &p.argument_list))
            }
            _ => return,
        };
        let ClassLikeMemberSelector::Identifier(name) = method else {
            return;
        };
        let Some(item) = self.items.iter().find(|i| {
            i.method.as_bytes().eq_ignore_ascii_case(name.value)
                && is_object_type(ctx, ancestors, class, &i.class)
        }) else {
            return;
        };
        let span = node_span(&node);
        out.push(Report::fixed(
            span,
            format!(
                "Replace the call to `{}::{}()` with `{}()`.",
                item.class, item.method, item.function
            ),
            Safety::Unsafe,
            vec![Edit::new(span, format!("\\{}{args}", item.function))],
        ));
    }
}
