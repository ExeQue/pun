use mago_codex::ttype::TType;
use mago_codex::ttype::atomic::TAtomic;
use mago_codex::ttype::atomic::object::TObject;
use mago_codex::ttype::atomic::scalar::TScalar;
use mago_codex::ttype::union::TUnion;
use mago_syntax::cst::{Block, Node, NodeKind, Statement};

use super::declare::{return_declaration, union_order};
use super::facts::{Facts, facts_with_dead_code};
use super::native::native_type;
use super::silent_void::Place;
use super::{
    SCALAR_TYPES, add_return_type, class_scope, has_only_return_with_expr, returns_scoped, untyped,
};
use crate::{Context, Report, Rule};

pub struct AddReturnTypeFromTryCatch;

impl Rule for AddReturnTypeFromTryCatch {
    fn id(&self) -> &'static str {
        "lint/types/add-return-type-from-try-catch"
    }

    fn min_php_version(&self) -> crate::PhpVersion {
        crate::PhpVersion::from_id(SCALAR_TYPES)
    }

    fn targets(&self) -> Option<&'static [NodeKind]> {
        Some(&[NodeKind::Method])
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        let Some(f) = untyped(node, ancestors, ctx) else {
            return;
        };
        let scope = class_scope(ancestors);
        let facts = facts_with_dead_code(ctx);
        let place = Place {
            facts: &facts,
            ancestors,
            scope: scope.as_ref(),
        };
        let returns = returns_scoped(f.body);
        if !has_only_return_with_expr(&place, f.body, &returns) {
            return;
        }
        let mut try_type: Option<&TUnion> = None;
        let mut catch_types: Vec<&TUnion> = Vec::new();
        for statement in &f.body.statements {
            let Statement::Try(r#try) = statement else {
                continue;
            };
            if r#try.catch_clauses.is_empty() {
                continue;
            }
            try_type = block_type(&facts, &r#try.block);
            for clause in &r#try.catch_clauses {
                let Some(t) = block_type(&facts, &clause.block) else {
                    return;
                };
                catch_types.push(t);
            }
            if let Some(t) =
                (r#try.finally_clause.as_ref()).and_then(|c| block_type(&facts, &c.block))
            {
                catch_types.push(t);
            }
        }
        let Some(try_type) = try_type else {
            return;
        };
        let Some(wanted) = hash(try_type) else {
            return;
        };
        if catch_types
            .iter()
            .any(|t| hash(t).as_ref() != Some(&wanted))
        {
            return;
        }
        if (try_type.types.iter())
            .any(|t| matches!(t, TAtomic::Scalar(TScalar::ClassLikeString(c)) if c.is_literal()))
        {
            return;
        }
        let types = union_order(try_type.types.iter());
        if let Some(r#type) =
            return_declaration(types, &facts.codebase, ctx.php_version.id(), &[], true)
        {
            out.push(add_return_type(f.span, f.at, &r#type));
        }
    }
}

fn block_type<'f>(facts: &'f Facts, block: &Block<'_>) -> Option<&'f TUnion> {
    block
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::Return(r) => r.value.and_then(|value| native_type(facts, value)),
            _ => None,
        })
}

fn hash(ty: &TUnion) -> Option<String> {
    let mut keys: Vec<String> = Vec::new();
    let mut constants = 0;
    for atomic in ty.types.iter() {
        let key = match atomic {
            TAtomic::Null => "null".to_string(),
            TAtomic::Scalar(TScalar::Bool(_)) => "bool".to_string(),
            TAtomic::Scalar(TScalar::Float(_)) => "float".to_string(),
            TAtomic::Scalar(TScalar::Integer(i)) if i.is_literal() => {
                constants += 1;
                "int-constant".to_string()
            }
            TAtomic::Scalar(TScalar::String(s)) if s.is_known_literal() => {
                constants += 1;
                "string-constant".to_string()
            }
            TAtomic::Array(array) if array.is_sealed() => "constant-array".to_string(),
            TAtomic::Scalar(TScalar::ClassLikeString(c)) if c.is_literal() => {
                constants += 1;
                "string-constant".to_string()
            }
            TAtomic::Object(TObject::Named(named)) if named.intersection_types.is_none() => {
                format!("object:{}", named.name.to_ascii_lowercase())
            }
            TAtomic::Scalar(_) | TAtomic::Object(_) | TAtomic::Array(_) => {
                atomic.get_id().to_string()
            }
            _ => return None,
        };
        keys.push(key);
    }
    if constants > 1 {
        return None;
    }
    keys.sort();
    keys.dedup();
    Some(keys.join("|"))
}
