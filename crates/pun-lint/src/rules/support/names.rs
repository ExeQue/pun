use mago_span::HasSpan;
use mago_syntax::cst::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Class,
    Function,
    Const,
}

struct Scope {
    namespace: Vec<u8>,
    imports: Vec<(Kind, Vec<u8>, Vec<u8>)>,
}

fn scope(ancestors: &[Node<'_, '_>], offset: u32) -> Scope {
    let mut namespace = Vec::new();
    let mut statements: Vec<&Statement<'_>> = Vec::new();
    match ancestors
        .iter()
        .rev()
        .find(|n| matches!(n, Node::Namespace(_)))
    {
        Some(Node::Namespace(ns)) => {
            if let Some(name) = &ns.name {
                namespace = name.value().to_vec();
            }
            match &ns.body {
                NamespaceBody::Implicit(body) => statements.extend(body.statements.iter()),
                NamespaceBody::BraceDelimited(block) => statements.extend(block.statements.iter()),
            }
        }
        _ => {
            if let Some(Node::Program(program)) = ancestors.first() {
                statements.extend(program.statements.iter());
            }
        }
    }
    let mut imports = Vec::new();
    for statement in statements {
        let Statement::Use(r#use) = statement else {
            continue;
        };
        if r#use.span().start.offset > offset {
            break;
        }
        let mut add = |kind: Kind, prefix: &[u8], item: &UseItem<'_>| {
            let mut fqn = prefix.to_vec();
            if !fqn.is_empty() {
                fqn.push(b'\\');
            }
            fqn.extend_from_slice(
                item.name
                    .value()
                    .strip_prefix(b"\\")
                    .unwrap_or(item.name.value()),
            );
            let alias = match &item.alias {
                Some(alias) => alias.identifier.value.to_vec(),
                None => item.name.last_segment().to_vec(),
            };
            imports.push((kind, alias, fqn));
        };
        let kind_of = |t: &mago_syntax::cst::UseType<'_>| {
            if t.is_function() {
                Kind::Function
            } else {
                Kind::Const
            }
        };
        match &r#use.items {
            UseItems::Sequence(s) => s.items.iter().for_each(|i| add(Kind::Class, b"", i)),
            UseItems::TypedSequence(s) => {
                let kind = kind_of(&s.r#type);
                s.items.iter().for_each(|i| add(kind, b"", i));
            }
            UseItems::TypedList(l) => {
                let kind = kind_of(&l.r#type);
                let prefix = l.namespace.value();
                let prefix = prefix.strip_prefix(b"\\").unwrap_or(prefix);
                l.items.iter().for_each(|i| add(kind, prefix, i));
            }
            UseItems::MixedList(l) => {
                let prefix = l.namespace.value();
                let prefix = prefix.strip_prefix(b"\\").unwrap_or(prefix);
                for i in &l.items {
                    let kind = i.r#type.as_ref().map_or(Kind::Class, kind_of);
                    add(kind, prefix, &i.item);
                }
            }
        }
    }
    Scope { namespace, imports }
}

impl Scope {
    fn import(&self, kind: Kind, name: &[u8]) -> Option<&[u8]> {
        self.imports
            .iter()
            .rev()
            .find(|(k, alias, _)| *k == kind && alias.eq_ignore_ascii_case(name))
            .map(|(_, _, fqn)| fqn.as_slice())
    }

    fn resolve(&self, kind: Kind, name: &[u8]) -> (Vec<u8>, bool) {
        if let Some(fqn) = name.strip_prefix(b"\\") {
            return (fqn.to_vec(), true);
        }
        let (first, suffix) = match name.iter().position(|&b| b == b'\\') {
            Some(i) => (&name[..i], Some(&name[i + 1..])),
            None => (name, None),
        };
        if let Some(suffix) = suffix
            && first.eq_ignore_ascii_case(b"namespace")
        {
            return (self.prefixed(suffix), true);
        }
        let alias_kind = if suffix.is_some() { Kind::Class } else { kind };
        if let Some(fqn) = self.import(alias_kind, first) {
            return match suffix {
                Some(suffix) => ([fqn, &b"\\"[..], suffix].concat(), true),
                None => (fqn.to_vec(), true),
            };
        }
        (self.prefixed(name), false)
    }

    fn prefixed(&self, name: &[u8]) -> Vec<u8> {
        if self.namespace.is_empty() {
            return name.to_vec();
        }
        let mut out = self.namespace.clone();
        out.push(b'\\');
        out.extend_from_slice(name);
        out
    }
}

pub(crate) fn resolve_class(ancestors: &[Node<'_, '_>], identifier: &Identifier<'_>) -> Vec<u8> {
    scope(ancestors, identifier.span().start.offset)
        .resolve(Kind::Class, identifier.value())
        .0
}

pub(crate) fn function_call_matches_any<'n>(
    ancestors: &[Node<'_, '_>],
    call: &FunctionCall<'_>,
    names: &[&'n str],
) -> Option<&'n str> {
    let Expression::Identifier(identifier) = call.function else {
        return None;
    };
    let find = |candidate: &[u8]| {
        names
            .iter()
            .find(|n| candidate.eq_ignore_ascii_case(n.as_bytes()))
            .copied()
    };
    let scope = scope(ancestors, identifier.span().start.offset);
    let (resolved, imported) = scope.resolve(Kind::Function, identifier.value());
    if imported {
        return find(&resolved);
    }
    find(identifier.value()).or_else(|| find(&resolved))
}

pub(crate) fn is_imported(
    ancestors: &[Node<'_, '_>],
    identifier: &Identifier<'_>,
    constant: bool,
) -> bool {
    let kind = if constant {
        Kind::Const
    } else {
        Kind::Function
    };
    scope(ancestors, identifier.span().start.offset)
        .resolve(kind, identifier.value())
        .1
}

pub(crate) fn function_call_matches(
    ancestors: &[Node<'_, '_>],
    call: &FunctionCall<'_>,
    name: &str,
) -> bool {
    function_call_matches_any(ancestors, call, &[name]).is_some()
}

pub(crate) fn resolve_class_at(ancestors: &[Node<'_, '_>], offset: u32, name: &[u8]) -> Vec<u8> {
    scope(ancestors, offset).resolve(Kind::Class, name).0
}

pub(crate) fn resolve_function(
    ancestors: &[Node<'_, '_>],
    identifier: &Identifier<'_>,
) -> (Vec<u8>, bool) {
    scope(ancestors, identifier.span().start.offset).resolve(Kind::Function, identifier.value())
}

fn namespace_and_imports(ancestors: &[Node<'_, '_>]) -> (String, Vec<(String, String)>) {
    let mut namespace = String::new();
    let mut statements: Option<&[Statement<'_>]> = None;
    for node in ancestors.iter().rev() {
        match node {
            Node::Namespace(ns) => {
                namespace = ns
                    .name
                    .as_ref()
                    .map(|n| String::from_utf8_lossy(n.value()).into_owned())
                    .unwrap_or_default();
                statements = Some(match &ns.body {
                    NamespaceBody::Implicit(b) => b.statements.as_slice(),
                    NamespaceBody::BraceDelimited(b) => b.statements.as_slice(),
                });
                break;
            }
            Node::Program(p) => {
                statements = Some(p.statements.as_slice());
                break;
            }
            _ => {}
        }
    }
    let mut imports = Vec::new();
    let mut add = |prefix: &str, item: &UseItem<'_>| {
        let name = String::from_utf8_lossy(item.name.value());
        let full = format!("{prefix}{}", name.trim_start_matches('\\'));
        let alias = match &item.alias {
            Some(a) => String::from_utf8_lossy(a.identifier.value).into_owned(),
            None => full.rsplit('\\').next().unwrap_or_default().to_string(),
        };
        imports.push((alias.to_ascii_lowercase(), full));
    };
    for statement in statements.unwrap_or_default() {
        let Statement::Use(u) = statement else {
            continue;
        };
        match &u.items {
            UseItems::Sequence(s) => s.items.iter().for_each(|i| add("", i)),
            UseItems::MixedList(l) => {
                let prefix = format!("{}\\", String::from_utf8_lossy(l.namespace.value()));
                l.items
                    .iter()
                    .filter(|i| i.r#type.is_none())
                    .for_each(|i| add(prefix.trim_start_matches('\\'), &i.item));
            }
            UseItems::TypedSequence(_) | UseItems::TypedList(_) => {}
        }
    }
    (namespace, imports)
}

pub(crate) fn class_name(ancestors: &[Node<'_, '_>], name: &Identifier<'_>) -> String {
    let written = String::from_utf8_lossy(name.value()).into_owned();
    if let Identifier::FullyQualified(_) = name {
        return written.trim_start_matches('\\').to_string();
    }
    let (namespace, imports) = namespace_and_imports(ancestors);
    if let Some(rest) = written
        .strip_prefix("namespace\\")
        .or_else(|| written.strip_prefix("NAMESPACE\\"))
    {
        return join(&namespace, rest);
    }
    let (first, rest) = match written.split_once('\\') {
        Some((f, r)) => (f, Some(r)),
        None => (written.as_str(), None),
    };
    if let Some((_, full)) = imports
        .iter()
        .find(|(alias, _)| *alias == first.to_ascii_lowercase())
    {
        return match rest {
            Some(rest) => format!("{full}\\{rest}"),
            None => full.clone(),
        };
    }
    join(&namespace, &written)
}

fn join(namespace: &str, name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{namespace}\\{name}")
    }
}

pub(crate) fn scope_statements<'a>(
    ancestors: &[Node<'a, 'a>],
) -> (Option<String>, Vec<&'a Statement<'a>>) {
    for node in ancestors.iter().rev() {
        if let Node::Namespace(ns) = node {
            let name = ns
                .name
                .as_ref()
                .map(|n| String::from_utf8_lossy(n.value()).into_owned());
            let statements = match &ns.body {
                NamespaceBody::Implicit(body) => body.statements.iter().collect(),
                NamespaceBody::BraceDelimited(block) => block.statements.iter().collect(),
            };
            return (name, statements);
        }
    }
    match ancestors.first() {
        Some(Node::Program(program)) => (None, program.statements.iter().collect()),
        _ => (None, Vec::new()),
    }
}

pub(crate) fn resolve_class_name(ancestors: &[Node<'_, '_>], name: &Identifier<'_>) -> String {
    let written = String::from_utf8_lossy(name.value()).into_owned();
    if let Some(fq) = written.strip_prefix('\\') {
        return fq.to_string();
    }
    if let Some(imported) = imported(ancestors, &written) {
        return imported;
    }
    match scope_statements(ancestors).0 {
        Some(ns) => format!("{ns}\\{written}"),
        None => written,
    }
}

pub(crate) fn imported(ancestors: &[Node<'_, '_>], written: &str) -> Option<String> {
    let (first, rest) = match written.split_once('\\') {
        Some((first, rest)) => (first, Some(rest)),
        None => (written, None),
    };
    for statement in scope_statements(ancestors).1 {
        let Statement::Use(r#use) = statement else {
            continue;
        };
        let items: Vec<(Option<&[u8]>, &UseItem)> = match &r#use.items {
            UseItems::Sequence(s) => s.items.iter().map(|i| (None, i)).collect(),
            UseItems::MixedList(l) => l
                .items
                .iter()
                .filter(|i| i.r#type.is_none())
                .map(|i| (Some(l.namespace.value()), &i.item))
                .collect(),
            UseItems::TypedSequence(_) | UseItems::TypedList(_) => continue,
        };
        for (prefix, item) in items {
            let name = String::from_utf8_lossy(item.name.value());
            let full = match prefix {
                Some(prefix) => format!("{}\\{name}", String::from_utf8_lossy(prefix)),
                None => name.into_owned(),
            };
            let full = full.trim_start_matches('\\').to_string();
            let alias = match &item.alias {
                Some(alias) => String::from_utf8_lossy(alias.identifier.value).into_owned(),
                None => full.rsplit('\\').next().unwrap_or(&full).to_string(),
            };
            if alias.eq_ignore_ascii_case(first) {
                return Some(match rest {
                    Some(rest) => format!("{full}\\{rest}"),
                    None => full,
                });
            }
        }
    }
    None
}

pub(crate) fn imported_function(ancestors: &[Node<'_, '_>], local: &[u8]) -> Option<String> {
    imported_member(ancestors, local, true)
}

pub(crate) fn imported_member(
    ancestors: &[Node<'_, '_>],
    local: &[u8],
    function: bool,
) -> Option<String> {
    let wanted = |t: &UseType<'_>| {
        if function {
            t.is_function()
        } else {
            t.is_const()
        }
    };
    for statement in scope_statements(ancestors).1 {
        let Statement::Use(r#use) = statement else {
            continue;
        };
        let items: Vec<(Option<&[u8]>, &UseItem)> = match &r#use.items {
            UseItems::TypedSequence(s) if wanted(&s.r#type) => {
                s.items.iter().map(|i| (None, i)).collect()
            }
            UseItems::TypedList(l) if wanted(&l.r#type) => l
                .items
                .iter()
                .map(|i| (Some(l.namespace.value()), i))
                .collect(),
            UseItems::MixedList(l) => l
                .items
                .iter()
                .filter(|i| i.r#type.as_ref().is_some_and(wanted))
                .map(|i| (Some(l.namespace.value()), &i.item))
                .collect(),
            _ => continue,
        };
        for (prefix, item) in items {
            let name = String::from_utf8_lossy(item.name.value());
            let full = match prefix {
                Some(prefix) => format!("{}\\{name}", String::from_utf8_lossy(prefix)),
                None => name.into_owned(),
            };
            let full = full.trim_start_matches('\\').to_string();
            let alias = match &item.alias {
                Some(alias) => alias.identifier.value.to_vec(),
                None => full
                    .rsplit('\\')
                    .next()
                    .unwrap_or(&full)
                    .as_bytes()
                    .to_vec(),
            };
            if (function && alias.eq_ignore_ascii_case(local)) || alias == local {
                return Some(full);
            }
        }
    }
    None
}

pub(crate) fn resolve_constant_name(ancestors: &[Node<'_, '_>], name: &Identifier<'_>) -> String {
    let written = String::from_utf8_lossy(name.value()).into_owned();
    if let Some(fq) = written.strip_prefix('\\') {
        return fq.to_string();
    }
    let imported = match name {
        Identifier::Local(local) => imported_member(ancestors, local.value, false),
        _ => imported(ancestors, &written),
    };
    if let Some(imported) = imported {
        return imported;
    }
    match scope_statements(ancestors).0 {
        Some(ns) => format!("{ns}\\{written}"),
        None => written,
    }
}

pub(crate) fn called_function_name(
    ancestors: &[Node<'_, '_>],
    function: &Expression<'_>,
) -> Option<String> {
    let Expression::Identifier(identifier) = function else {
        return None;
    };
    if let Identifier::Local(local) = identifier
        && let Some(full) = imported_function(ancestors, local.value)
    {
        return Some(full);
    }
    let written = String::from_utf8_lossy(identifier.value());
    Some(written.trim_start_matches('\\').to_string())
}

pub(crate) fn has_attribute(
    ancestors: &[Node<'_, '_>],
    lists: &Sequence<'_, AttributeList<'_>>,
    name: &str,
) -> bool {
    lists.iter().any(|list| {
        list.attributes
            .iter()
            .any(|a| resolve_class_name(ancestors, &a.name).eq_ignore_ascii_case(name))
    })
}
