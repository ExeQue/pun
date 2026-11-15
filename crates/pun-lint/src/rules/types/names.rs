use mago_syntax::cst::{Identifier, NamespaceBody, Node, Statement, UseItem, UseItems};

pub(crate) fn scope<'a>(ancestors: &[Node<'a, 'a>]) -> (Option<&'a [u8]>, &'a [Statement<'a>]) {
    for node in ancestors.iter().rev() {
        if let Node::Namespace(ns) = *node {
            let statements = match &ns.body {
                NamespaceBody::Implicit(body) => body.statements.as_slice(),
                NamespaceBody::BraceDelimited(block) => block.statements.as_slice(),
            };
            return (
                ns.name.as_ref().map(mago_syntax::cst::Identifier::value),
                statements,
            );
        }
    }
    match ancestors.first() {
        Some(Node::Program(program)) => (None, program.statements.as_slice()),
        _ => (None, &[]),
    }
}

fn class_use_items<'a>(
    statements: &'a [Statement<'a>],
) -> impl Iterator<Item = (Option<&'a [u8]>, &'a UseItem<'a>)> {
    statements.iter().flat_map(|statement| {
        let (sequence, list) = match statement {
            Statement::Use(r#use) => match &r#use.items {
                UseItems::Sequence(s) => (Some(s), None),
                UseItems::MixedList(l) => (None, Some(l)),
                UseItems::TypedSequence(_) | UseItems::TypedList(_) => (None, None),
            },
            _ => (None, None),
        };
        let plain = sequence
            .into_iter()
            .flat_map(|s| s.items.iter().map(|i| (None, i)));
        let grouped = list.into_iter().flat_map(|l| {
            (l.items.iter())
                .filter(|i| i.r#type.is_none())
                .map(|i| (Some(l.namespace.value()), &i.item))
        });
        plain.chain(grouped)
    })
}

pub(crate) fn resolve_class_name(ancestors: &[Node<'_, '_>], name: &Identifier<'_>) -> String {
    resolve_written(ancestors, &String::from_utf8_lossy(name.value())).0
}

pub(crate) fn resolve_written(ancestors: &[Node<'_, '_>], written: &str) -> (String, bool) {
    if let Some(fq) = written.strip_prefix('\\') {
        return (fq.to_string(), false);
    }
    let (namespace, statements) = scope(ancestors);
    let (first, rest) = match written.split_once('\\') {
        Some((first, rest)) => (first, Some(rest)),
        None => (written, None),
    };
    for (prefix, item) in class_use_items(statements) {
        let name = item.name.value();
        let alias = match &item.alias {
            Some(a) => a.identifier.value,
            None => name.rsplit(|b| *b == b'\\').next().unwrap_or(name),
        };
        if alias.eq_ignore_ascii_case(first.as_bytes()) {
            let mut full = match prefix {
                Some(prefix) => format!(
                    "{}\\{}",
                    String::from_utf8_lossy(prefix),
                    String::from_utf8_lossy(name)
                ),
                None => String::from_utf8_lossy(name).into_owned(),
            };
            if full.starts_with('\\') {
                full = full.trim_start_matches('\\').to_string();
            }
            if let Some(rest) = rest {
                full.push('\\');
                full.push_str(rest);
            }
            return (full, true);
        }
    }
    let resolved = match namespace {
        Some(ns) => format!("{}\\{written}", String::from_utf8_lossy(ns)),
        None => written.to_string(),
    };
    (resolved, false)
}

pub(crate) fn class_imports(
    ancestors: &[Node<'_, '_>],
) -> (Option<String>, Vec<(String, Option<String>)>) {
    let (namespace, statements) = scope(ancestors);
    let mut imports = Vec::new();
    for (prefix, item) in class_use_items(statements) {
        let name = String::from_utf8_lossy(item.name.value());
        let full = match prefix {
            Some(prefix) => format!("{}\\{name}", String::from_utf8_lossy(prefix)),
            None => name.into_owned(),
        };
        let alias =
            (item.alias.as_ref()).map(|a| String::from_utf8_lossy(a.identifier.value).into_owned());
        imports.push((full.trim_start_matches('\\').to_string(), alias));
    }
    (
        namespace.map(|n| String::from_utf8_lossy(n).into_owned()),
        imports,
    )
}

pub(crate) fn resolve_written_class_name(ancestors: &[Node<'_, '_>], written: &str) -> String {
    resolve_written(ancestors, written).0
}
