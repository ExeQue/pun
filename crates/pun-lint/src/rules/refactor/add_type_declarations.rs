//! Rector's configurable `AddParamTypeDeclarationRector` and `AddReturnTypeDeclarationRector`, which the PHPUnit
//! upgrade sets configure. Rector takes PHPStan `Type` objects; Pun takes the type as a string, as `pun migrate`
//! writes it from the object's PHPDoc form (`string`, `\Foo\Bar`, `?string`, `(string | null)`, `static`, `mixed`).

use crate::problem::{Edit, Safety, Span};
use mago_syntax::cst::{ClassLikeMember, Hint, Method, Node, NodeKind};

use super::{assert_class, assert_method, int, is_class, items, object, string};
use crate::rules::support::class_members::qualified;
use crate::rules::support::names::resolve_class_name;
use crate::rules::types::class_scope;
use crate::syntax::node_span;
use crate::{Context, OptionsError, Report, Rule};

const BUILTINS: &[&str] = &[
    "string", "int", "float", "bool", "array", "callable", "iterable", "object", "mixed", "void",
    "null", "never", "false", "true", "static", "self",
];

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Atom {
    Builtin(String),
    Class(String),
}

fn parse_type(text: &str) -> Result<Vec<Atom>, OptionsError> {
    let bad = || -> OptionsError {
        format!(
            "\"{text}\" is not a type Pun can declare; use a native type such as `string`, `?\\Foo\\Bar` or `int|string`"
        )
        .into()
    };
    let mut t = text.trim();
    if let Some(inner) = t.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        t = inner.trim();
    }
    let mut atoms = Vec::new();
    let nullable = t.strip_prefix('?');
    if let Some(inner) = nullable {
        t = inner.trim();
    }
    for part in t.split('|') {
        let part = part.trim();
        if part.is_empty() || part.contains(|c: char| "&<>{}[]()?, ".contains(c)) {
            return Err(bad());
        }
        let lower = part.to_ascii_lowercase();
        atoms.push(if BUILTINS.contains(&lower.as_str()) {
            Atom::Builtin(lower)
        } else {
            let name = part.trim_start_matches('\\');
            assert_class(name).map_err(|_| bad())?;
            Atom::Class(name.to_string())
        });
    }
    if nullable.is_some() {
        if atoms.len() != 1 {
            return Err(bad());
        }
        atoms.push(Atom::Builtin("null".into()));
    }
    Ok(atoms)
}

/// A declared type's atoms, its class names resolved; `None` for an intersection or `parent`.
fn hint_atoms(
    source: &str,
    ancestors: &[Node<'_, '_>],
    hint: &Hint<'_>,
    out: &mut Vec<Atom>,
) -> Option<()> {
    match hint {
        Hint::Identifier(id) => out.push(Atom::Class(resolve_class_name(ancestors, id))),
        Hint::Parenthesized(p) => hint_atoms(source, ancestors, p.hint, out)?,
        Hint::Nullable(n) => {
            hint_atoms(source, ancestors, n.hint, out)?;
            out.push(Atom::Builtin("null".into()));
        }
        Hint::Union(u) => {
            hint_atoms(source, ancestors, u.left, out)?;
            hint_atoms(source, ancestors, u.right, out)?;
        }
        Hint::Intersection(_) | Hint::Parent(_) => return None,
        Hint::Self_(_) => out.push(Atom::Class(class_scope(ancestors)?.name?)),
        keyword => {
            let span = node_span(keyword);
            out.push(Atom::Builtin(
                source[span.start..span.end].to_ascii_lowercase(),
            ));
        }
    }
    Some(())
}

fn declared(ctx: &Context<'_>, ancestors: &[Node<'_, '_>], hint: &Hint<'_>) -> Option<Vec<Atom>> {
    let mut atoms = Vec::new();
    hint_atoms(ctx.source, ancestors, hint, &mut atoms)?;
    Some(atoms)
}

fn same(a: &Atom, b: &Atom) -> bool {
    match (a, b) {
        (Atom::Builtin(x), Atom::Builtin(y)) => x == y,
        (Atom::Class(x), Atom::Class(y)) => x.eq_ignore_ascii_case(y),
        _ => false,
    }
}

/// `TypeComparator::areTypesEqual()`, on the atoms.
fn equal(a: &[Atom], b: &[Atom]) -> bool {
    a.iter().all(|x| b.iter().any(|y| same(x, y))) && b.iter().all(|y| a.iter().any(|x| same(x, y)))
}

/// `TypeComparator::isSubtype()`: every atom of `checked` is in `main`.
fn is_subtype(ctx: &Context<'_>, checked: &[Atom], main: &[Atom]) -> bool {
    let mixed = |t: &[Atom]| {
        t.iter()
            .any(|a| matches!(a, Atom::Builtin(b) if b == "mixed"))
    };
    if mixed(main) {
        return false;
    }
    checked.iter().all(|a| {
        main.iter().any(|b| {
            same(a, b)
                || match (a, b) {
                    (Atom::Class(x), Atom::Class(y)) => is_class(ctx, x, y),
                    (Atom::Class(_), Atom::Builtin(y)) => y == "object",
                    (Atom::Builtin(x), Atom::Builtin(y)) => {
                        y == "bool" && (x == "true" || x == "false")
                    }
                    _ => false,
                }
        })
    })
}

/// `StaticTypeMapper::mapPHPStanTypeToPhpParserNode()`: a class fully qualified, a type with `null` as `?type`.
fn print(atoms: &[Atom]) -> String {
    let one = |a: &Atom| match a {
        Atom::Builtin(b) => b.clone(),
        Atom::Class(c) => format!("\\{c}"),
    };
    let null = |a: &Atom| matches!(a, Atom::Builtin(b) if b == "null");
    if atoms.len() == 2
        && let Some(other) = atoms.iter().find(|a| !null(a))
        && atoms.iter().any(null)
        && !matches!(other, Atom::Builtin(b) if b == "mixed")
    {
        return format!("?{}", one(other));
    }
    atoms.iter().map(one).collect::<Vec<_>>().join("|")
}

fn is_mixed(atoms: &[Atom]) -> bool {
    matches!(atoms, [Atom::Builtin(b)] if b == "mixed")
}

fn own_class(ancestors: &[Node<'_, '_>], class: &mago_syntax::cst::Class<'_>) -> String {
    qualified(ancestors, class.name.value)
        .trim_start_matches('\\')
        .to_string()
}

fn methods<'a>(class: &'a mago_syntax::cst::Class<'a>) -> impl Iterator<Item = &'a Method<'a>> {
    class.members.iter().filter_map(|m| match m {
        ClassLikeMember::Method(m) => Some(m),
        _ => None,
    })
}

/// `AddParamTypeDeclarationRector`.
#[derive(Default)]
pub struct AddParamTypeDeclaration {
    /// (class, method, position, type)
    types: Vec<(String, String, usize, Vec<Atom>)>,
}

impl Rule for AddParamTypeDeclaration {
    fn id(&self) -> &'static str {
        "lint/refactor/add-param-type"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut types = Vec::new();
        for item in items(options)? {
            let keys = ["class-name", "method-name", "position", "param-type"];
            let map = object(item, &keys, &keys)?;
            let class = string(map, "class-name")?;
            let class = class.trim_start_matches('\\').to_string();
            let method = string(map, "method-name")?;
            assert_class(&class)?;
            assert_method(&method)?;
            let position = usize::try_from(int(map, "position")?)
                .map_err(|_| "`position` must be 0 or more")?;
            types.push((
                class,
                method,
                position,
                parse_type(&string(map, "param-type")?)?,
            ));
        }
        self.types = types;
        Ok(())
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
        // skip class without parents/interfaces, there is no param type to mimic
        if class.extends.is_none() && class.implements.is_none() {
            return;
        }
        let mut scope = ancestors.to_vec();
        scope.push(node);
        let name = own_class(ancestors, class);
        for method in methods(class) {
            let params: Vec<_> = method.parameter_list.parameters.iter().collect();
            for (_, _, position, new) in self.types.iter().filter(|(c, m, _, _)| {
                m.as_bytes().eq_ignore_ascii_case(method.name.value) && is_class(ctx, &name, c)
            }) {
                let Some(param) = params.get(*position) else {
                    continue;
                };
                // already set → no change
                if let Some(hint) = &param.hint
                    && declared(ctx, &scope, hint).is_some_and(|current| equal(&current, new))
                {
                    continue;
                }
                let at = param
                    .ampersand
                    .or(param.ellipsis)
                    .map_or(node_span(&param.variable).start, |s| {
                        s.start.offset as usize
                    });
                let edit = match (&param.hint, is_mixed(new) && ctx.php_version.id() < 80000) {
                    // remove it
                    (Some(hint), true) => Edit::new(Span::new(node_span(hint).start, at), ""),
                    (None, true) => continue,
                    (Some(hint), false) => Edit::new(node_span(hint), print(new)),
                    (None, false) => Edit::new(Span::new(at, at), format!("{} ", print(new))),
                };
                out.push(Report::fixed(
                    node_span(*param),
                    "Declare the parameter's type as configured.",
                    Safety::Unsafe,
                    vec![edit],
                ));
            }
        }
    }
}

/// `AddReturnTypeDeclarationRector`.
#[derive(Default)]
pub struct AddReturnTypeDeclaration {
    /// (class, method, type)
    types: Vec<(String, String, Vec<Atom>)>,
}

impl Rule for AddReturnTypeDeclaration {
    fn id(&self) -> &'static str {
        "lint/refactor/add-return-type"
    }

    fn configure(&mut self, options: &serde_json::Value) -> Result<(), OptionsError> {
        let mut types = Vec::new();
        for item in items(options)? {
            let keys = ["class", "method", "return-type"];
            let map = object(item, &keys, &keys)?;
            let class = string(map, "class")?;
            let class = class.trim_start_matches('\\').to_string();
            let method = string(map, "method")?;
            assert_class(&class)?;
            assert_method(&method)?;
            types.push((class, method, parse_type(&string(map, "return-type")?)?));
        }
        self.types = types;
        Ok(())
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
        let mut scope = ancestors.to_vec();
        scope.push(node);
        let name = own_class(ancestors, class);
        for method in methods(class) {
            for (object_type, _, new) in self.types.iter().filter(|(c, m, _)| {
                m.as_bytes().eq_ignore_ascii_case(method.name.value) && is_class(ctx, &name, c)
            }) {
                let current = method.return_type_hint.as_ref();
                if is_mixed(new) {
                    if !object_type.eq_ignore_ascii_case(&name) && current.is_some() {
                        continue;
                    }
                    // Rector clears the type before PHP 8.0 without counting it as a change
                    if ctx.php_version.id() < 80000 {
                        continue;
                    }
                }
                // already set and sub type or equal → no change
                if let Some(current) = current
                    && declared(ctx, &scope, &current.hint).is_some_and(|current| {
                        is_subtype(ctx, &current, new) || equal(&current, new)
                    })
                {
                    continue;
                }
                let edit = match current {
                    Some(current) => Edit::new(node_span(&current.hint), print(new)),
                    None => {
                        let at = method.parameter_list.right_parenthesis.end.offset as usize;
                        Edit::new(Span::new(at, at), format!(": {}", print(new)))
                    }
                };
                out.push(Report::fixed(
                    node_span(&method.name),
                    "Declare the method's return type as configured.",
                    Safety::Unsafe,
                    vec![edit],
                ));
            }
        }
    }
}
