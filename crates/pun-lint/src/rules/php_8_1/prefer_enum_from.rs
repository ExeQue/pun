//! Rector 2.8.4's `MyCLabsConstructorCallToEnumFromRector`.

use mago_syntax::cst::{Expression, Node, NodeKind};
use mago_word::word;

use crate::problem::{Edit, Safety};
use crate::rules::refactor::is_object_type;
use crate::rules::support::expressions::text;
use crate::rules::types::class_scope;
use crate::rules::types::facts::facts;
use crate::rules::types::names::resolve_class_name;
use crate::syntax::{node_span, unparenthesize};
use crate::{Context, Report, Rule};

const MY_C_LABS_CLASS: &str = "MyCLabs\\Enum\\Enum";

pub struct PreferEnumFrom;

impl Rule for PreferEnumFrom {
    fn id(&self) -> &'static str {
        "lint/php-8.1/prefer-enum-from"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80100)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Instantiation])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::Instantiation(new) = node else {
            return;
        };
        let class = unparenthesize(new.class);
        let name = match class {
            Expression::Identifier(id) => resolve_class_name(ancestors, id),
            Expression::Self_(_) | Expression::Static(_) => {
                match class_scope(ancestors).and_then(|s| s.name) {
                    Some(name) => name,
                    None => return,
                }
            }
            _ => return,
        };
        if !is_object_type(ctx, ancestors, class, MY_C_LABS_CLASS) {
            return;
        }
        // `isMyCLabsConstructor`: no constructor, or the one `MyCLabs\Enum\Enum` declares.
        let facts = facts(ctx);
        let Some(metadata) = facts.codebase.get_class_like(name.as_bytes()) else {
            return;
        };
        if let Some(constructor) = metadata.declaring_method_ids.get(&word("__construct"))
            && !constructor
                .get_class_name()
                .as_bytes()
                .eq_ignore_ascii_case(MY_C_LABS_CLASS.as_bytes())
        {
            return;
        }
        let args = new
            .argument_list
            .as_ref()
            .map_or("()", |list| text(ctx, list));
        let span = node_span(new);
        out.push(Report::fixed(
            span,
            "Create the enum with `from()` instead of its constructor.",
            Safety::Unsafe,
            vec![Edit::new(
                span,
                format!("\\{}::from{args}", metadata.original_name),
            )],
        ));
    }
}
