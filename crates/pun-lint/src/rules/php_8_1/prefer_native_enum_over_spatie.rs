//! Rector 2.8.4's `SpatieEnumClassToEnumRector`.

use mago_syntax::cst::{Node, NodeKind};

use super::enum_factory::from_spatie_class;
use crate::problem::Safety;
use crate::rules::refactor::is_class;
use crate::rules::support::class_members::qualified;
use crate::rules::support::options::{boolean, configure};
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

#[derive(Default)]
pub struct PreferNativeEnumOverSpatie {
    /// Rector's `toUpperSnakeCase` option: `needsReview()` becomes `NEEDS_REVIEW`, not `NEEDSREVIEW`.
    pub to_upper_snake_case: bool,
}

impl Rule for PreferNativeEnumOverSpatie {
    fn id(&self) -> &'static str {
        "lint/php-8.1/prefer-native-enum-over-spatie"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(80100)
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        configure(options, |key, value| {
            match key {
                "to-upper-snake-case" => self.to_upper_snake_case = boolean(value)?,
                _ => return None,
            }
            Some(())
        })
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
        if !is_class(
            ctx,
            &qualified(ancestors, class.name.value),
            "Spatie\\Enum\\Enum",
        ) {
            return;
        }
        out.push(Report::fixed(
            node_span(&class.name),
            "Turn the Spatie enum class into a native enum.",
            Safety::Unsafe,
            vec![from_spatie_class(
                ctx,
                ancestors,
                class,
                self.to_upper_snake_case,
            )],
        ));
    }
}
