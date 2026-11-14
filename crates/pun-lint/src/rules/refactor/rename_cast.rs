use mago_span::HasSpan;
use mago_syntax::cst::{Node, NodeKind, UnaryPrefixOperator};
use pun_problem::{Edit, Safety};

use super::{assert_class, int, items, object, string};
use crate::rules::support::expressions::{operand, text};
use crate::syntax::node_span;
use crate::{Context, Report, Rule};

const CASTS: &[&str] = &[
    "Array_", "Bool_", "Double", "Int_", "Object_", "String_", "Unset_", "Void_",
];

#[derive(Default)]
pub struct RenameCast {
    pub renames: Vec<(String, i64, i64)>,
}

fn class_and_kind(op: &UnaryPrefixOperator<'_>) -> Option<(&'static str, i64)> {
    Some(match op {
        UnaryPrefixOperator::BoolCast(..) => ("Bool_", 1),
        UnaryPrefixOperator::BooleanCast(..) => ("Bool_", 2),
        UnaryPrefixOperator::DoubleCast(..) => ("Double", 1),
        UnaryPrefixOperator::FloatCast(..) => ("Double", 2),
        UnaryPrefixOperator::IntCast(..) => ("Int_", 1),
        UnaryPrefixOperator::IntegerCast(..) => ("Int_", 2),
        UnaryPrefixOperator::StringCast(..) => ("String_", 1),
        UnaryPrefixOperator::BinaryCast(..) => ("String_", 2),
        _ => return None,
    })
}

fn printed(class: &str, kind: i64) -> &'static str {
    match class {
        "Double" if kind == 1 => "(double)",
        "Double" => "(float)",
        "Int_" => "(int)",
        "Bool_" => "(bool)",
        _ => "(string)",
    }
}

impl Rule for RenameCast {
    fn id(&self) -> &'static str {
        "lint/refactor/rename-cast"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), String> {
        const KEYS: &[&str] = &["from-cast-expr-class", "from-cast-kind", "to-cast-kind"];
        let mut renames = Vec::new();
        for item in items(options)? {
            let map = object(item, KEYS, KEYS)?;
            let class = string(map, "from-cast-expr-class")?;
            assert_class(&class)?;
            let short = class
                .trim_start_matches('\\')
                .strip_prefix("PhpParser\\Node\\Expr\\Cast\\")
                .filter(|s| CASTS.contains(s))
                .ok_or_else(|| {
                    format!("\"{class}\" is not a subclass of \"PhpParser\\Node\\Expr\\Cast\"")
                })?;
            let to = int(map, "to-cast-kind")?;
            if short == "Double" && !(1..=2).contains(&to) {
                return Err(format!(
                    "`to-cast-kind` {to} of \"{class}\" prints `(real)`, which PHP 8 removed"
                ));
            }
            renames.push((short.to_string(), int(map, "from-cast-kind")?, to));
        }
        self.renames = renames;
        Ok(())
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::UnaryPrefix])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Node::UnaryPrefix(cast) = node else {
            return;
        };
        let Some((class, kind)) = class_and_kind(&cast.operator) else {
            return;
        };
        let Some((_, _, to)) = self
            .renames
            .iter()
            .find(|(c, from, _)| c == class && *from == kind)
        else {
            return;
        };
        let new = printed(class, *to);
        let code = format!("{new} {}", operand(ctx, cast.operand, 10, None));
        if code == text(ctx, cast) {
            return;
        }
        out.push(Report::fixed(
            node_span(cast),
            format!(
                "Rename the cast `{}` to `{new}`.",
                text(ctx, &cast.operator.span())
            ),
            Safety::Unsafe,
            vec![Edit::new(node_span(cast), code)],
        ));
    }
}
