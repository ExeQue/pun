use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::Ordering::Relaxed;

use mago_codex::metadata::class_like::ClassLikeMetadata;
use mago_codex::symbol::SymbolKind;
use mago_span::Span;
use pun_phpstan::phpdoc::{self, Node};
use pun_phpstan::resolve::{Scope, resolve_in};
use pun_phpstan::symbols;
use pun_phpstan::types::{Level, NeverKind, Type};

use super::declared::{class_scope, original_name, with_classes};
use super::doc::NameScope;
use super::{Checker, DISCOVERING_SYMBOLS_TIP, missing_types};

#[derive(Clone)]
struct AliasTag {
    name: String,
    node: Option<Node>,
}

struct ImportTag {
    alias: String,
    from: String,
    imported_as: Option<String>,
}

struct AliasDoc {
    aliases: Vec<AliasTag>,
    imports: Vec<ImportTag>,
    scope: NameScope,
    alias_class: String,
}

impl AliasDoc {
    fn names(&self) -> HashSet<String> {
        self.imports
            .iter()
            .map(|i| i.imported_as.clone().unwrap_or_else(|| i.alias.clone()))
            .chain(self.aliases.iter().map(|a| a.name.clone()))
            .collect()
    }
}

fn identifier(bytes: &[u8]) -> Option<&[u8]> {
    let mut i = 0;
    loop {
        let start = i;
        if bytes.get(i) == Some(&b'\\') {
            i += 1;
        }
        let first = |b: u8| b.is_ascii_alphabetic() || b == b'_' || b >= 0x80;
        if !bytes.get(i).is_some_and(|b| first(*b)) {
            i = start;
            break;
        }
        i += 1;
        while bytes
            .get(i)
            .is_some_and(|b| first(*b) || b.is_ascii_digit() || *b == b'-')
        {
            i += 1;
        }
    }
    (i > 0).then(|| &bytes[..i])
}

fn alias_value(value: &[u8]) -> Option<AliasTag> {
    let name = identifier(value)?;
    let rest = value[name.len()..].trim_ascii_start();
    let rest = rest.strip_prefix(b"=").unwrap_or(rest).trim_ascii_start();
    let node = match phpdoc::parse_prefix(rest) {
        Ok((node, end)) if rest[end..].trim_ascii().is_empty() => Some(node),
        _ => None,
    };
    Some(AliasTag {
        name: String::from_utf8_lossy(name).into_owned(),
        node,
    })
}

fn import_value(value: &[u8], scope: &NameScope) -> Option<ImportTag> {
    let alias = identifier(value)?;
    let rest = value[alias.len()..].trim_ascii_start();
    let rest = rest.strip_prefix(b"from")?;
    if identifier(rest).is_some() {
        return None;
    }
    let rest = rest.trim_ascii_start();
    let from = identifier(rest)?;
    let rest = rest[from.len()..].trim_ascii_start();
    let imported_as = rest
        .strip_prefix(b"as")
        .filter(|r| identifier(r).is_none())
        .and_then(|r| identifier(r.trim_ascii_start()))
        .map(|name| String::from_utf8_lossy(name).into_owned());
    Some(ImportTag {
        alias: String::from_utf8_lossy(alias).into_owned(),
        from: scope.resolve_name(&String::from_utf8_lossy(from)),
        imported_as,
    })
}

fn upsert<T>(list: &mut Vec<T>, key: impl Fn(&T) -> &str, item: T) {
    match list.iter().position(|e| key(e) == key(&item)) {
        Some(i) => list[i] = item,
        None => list.push(item),
    }
}

const ALIAS_TAGS: [&str; 5] = [
    "@phan-type",
    "@psalm-type",
    "@phpstan-type",
    "@psalm-import-type",
    "@phpstan-import-type",
];

fn alias_doc(c: &Checker, k: &ClassLikeMetadata, scope: NameScope) -> Option<AliasDoc> {
    let (_, entry) = c.doc_before(k.span.file_id, k.span.start.offset)?;
    let tags = &entry.tags;
    let mut aliases = Vec::new();
    for kind in ["@phan-type", "@psalm-type", "@phpstan-type"] {
        for (_, value) in tags.iter().filter(|(t, _)| t == kind) {
            if let Some(tag) = alias_value(value) {
                upsert(&mut aliases, |a: &AliasTag| &a.name, tag);
            }
        }
    }
    let mut imports = Vec::new();
    for kind in ["@psalm-import-type", "@phpstan-import-type"] {
        for (_, value) in tags.iter().filter(|(t, _)| t == kind) {
            if let Some(tag) = import_value(value, &scope) {
                upsert(
                    &mut imports,
                    |i: &ImportTag| i.imported_as.as_deref().unwrap_or(&i.alias),
                    tag,
                );
            }
        }
    }
    Some(AliasDoc {
        aliases,
        imports,
        alias_class: original_name(k),
        scope,
    })
}

#[derive(Clone)]
enum Alias {
    Defined { doc: Rc<AliasDoc>, tag: AliasTag },
    Invalid,
}

struct Circular;

struct Resolver<'c, 'a, 'arena> {
    c: &'c Checker<'a, 'arena>,
    docs: RefCell<HashMap<String, Option<Rc<AliasDoc>>>>,
    aliases: RefCell<HashMap<String, Vec<(String, Alias)>>>,
    resolving_imports: RefCell<HashSet<String>>,
    resolved: RefCell<HashMap<String, (Type, bool)>>,
    in_process: RefCell<HashSet<String>>,
    throwing: Cell<bool>,
    circular: Cell<bool>,
}

impl<'c, 'a, 'arena> Resolver<'c, 'a, 'arena> {
    fn new(c: &'c Checker<'a, 'arena>) -> Self {
        Resolver {
            c,
            docs: Default::default(),
            aliases: Default::default(),
            resolving_imports: Default::default(),
            resolved: Default::default(),
            in_process: Default::default(),
            throwing: Cell::new(false),
            circular: Cell::new(false),
        }
    }

    fn doc(&self, name: &str) -> Option<Rc<AliasDoc>> {
        let key = name.to_ascii_lowercase();
        if let Some(found) = self.docs.borrow().get(&key) {
            return found.clone();
        }
        let c = self.c;
        let doc = c
            .codebase
            .get_class_like(name.as_bytes())
            .and_then(|k| alias_doc(c, k, class_scope(c, k.span, k)))
            .map(Rc::new);
        self.docs.borrow_mut().insert(key, doc.clone());
        doc
    }

    fn has_class(&self, name: &str) -> bool {
        symbols::lookup(name).is_some()
    }

    fn type_aliases(&self, class: &str) -> Vec<(String, Alias)> {
        self.aliases_of(class).unwrap_or_default()
    }

    fn aliases_of(&self, class: &str) -> Result<Vec<(String, Alias)>, Circular> {
        let key = class.to_ascii_lowercase();
        if let Some(found) = self.aliases.borrow().get(&key) {
            return Ok(found.clone());
        }
        let doc = self.doc(class);
        let locals: Vec<(String, Alias)> = doc
            .iter()
            .flat_map(|doc| doc.aliases.iter().map(move |t| (doc, t)))
            .map(|(doc, t)| {
                let alias = Alias::Defined {
                    doc: doc.clone(),
                    tag: t.clone(),
                };
                (t.name.clone(), alias)
            })
            .collect();
        if self.resolving_imports.borrow().contains(&key) {
            if locals.is_empty() {
                return Err(Circular);
            }
            self.aliases.borrow_mut().insert(key, locals.clone());
            return Ok(locals);
        }
        self.resolving_imports.borrow_mut().insert(key.clone());
        let mut merged: Vec<(String, Alias)> = Vec::new();
        for import in doc.iter().flat_map(|d| &d.imports) {
            if !self.has_class(&import.from) {
                continue;
            }
            let found = match self.aliases_of(&import.from) {
                Err(Circular) => Some(Alias::Invalid),
                Ok(aliases) => aliases
                    .into_iter()
                    .find(|(name, _)| *name == import.alias)
                    .map(|(_, a)| a),
            };
            if let Some(alias) = found {
                let name = import
                    .imported_as
                    .clone()
                    .unwrap_or_else(|| import.alias.clone());
                upsert(&mut merged, |(n, _): &(String, Alias)| n, (name, alias));
            }
        }
        self.resolving_imports.borrow_mut().remove(&key);
        for local in locals {
            upsert(&mut merged, |(n, _): &(String, Alias)| n, local);
        }
        self.aliases.borrow_mut().insert(key, merged.clone());
        Ok(merged)
    }

    fn resolve_alias(&self, alias: &Alias) -> Result<Type, Circular> {
        let Alias::Defined { doc, tag } = alias else {
            self.circular.set(true);
            return Ok(Type::Error);
        };
        let Some(node) = &tag.node else {
            return Ok(Type::Mixed { explicit: true });
        };
        let ty = resolve_in(node, &self.scope(doc, false));
        if self.throwing.replace(false) {
            return Err(Circular);
        }
        Ok(ty)
    }

    fn local(&self, name: &str, class: &str, names: &HashSet<String>) -> Option<Type> {
        if self.throwing.get() {
            return Some(Type::Error);
        }
        if !names.contains(name) {
            return None;
        }
        let key = format!("{}::{name}", class.to_ascii_lowercase());
        if let Some((ty, circular)) = self.resolved.borrow().get(&key) {
            if *circular {
                self.circular.set(true);
            }
            return Some(ty.clone());
        }
        let (_, alias) = self
            .type_aliases(class)
            .into_iter()
            .find(|(n, _)| n == name)?;
        if !self.in_process.borrow_mut().insert(key.clone()) {
            self.throwing.set(true);
            return Some(Type::Error);
        }
        let before = self.circular.replace(false);
        let resolved = match self.resolve_alias(&alias) {
            Ok(ty) => (ty, self.circular.get()),
            Err(Circular) => (Type::Error, true),
        };
        self.circular.set(before || resolved.1);
        self.resolved
            .borrow_mut()
            .insert(key.clone(), resolved.clone());
        self.in_process.borrow_mut().remove(&key);
        Some(resolved.0)
    }

    fn scope<'r>(&'r self, doc: &'r AliasDoc, bypass: bool) -> AliasScope<'r, 'c, 'a, 'arena> {
        AliasScope {
            resolver: self,
            doc,
            names: doc.names(),
            bypass,
        }
    }

    fn take_circular(&self) -> bool {
        self.throwing.set(false);
        self.circular.replace(false)
    }
}

fn without_aliases<R>(c: &Checker, f: impl FnOnce() -> R) -> R {
    let previous = c.resolving_aliases.swap(true, Relaxed);
    let out = f();
    c.resolving_aliases.store(previous, Relaxed);
    out
}

pub(super) fn class_aliases(
    c: &Checker,
    k: &ClassLikeMetadata,
    using: Option<&ClassLikeMetadata>,
) -> Vec<(String, Type)> {
    if c.resolving_aliases.swap(true, Relaxed) {
        return Vec::new();
    }
    let tagged = c
        .doc_before(k.span.file_id, k.span.start.offset)
        .is_some_and(|(_, entry)| {
            entry
                .tags
                .iter()
                .any(|(tag, _)| ALIAS_TAGS.contains(&tag.as_str()))
        });
    if !tagged {
        c.resolving_aliases.store(false, Relaxed);
        return Vec::new();
    }
    let resolver = Resolver::new(c);
    let class = original_name(k);
    if let Some(doc) = using.and_then(|u| in_context(c, k, u)) {
        resolver
            .docs
            .borrow_mut()
            .insert(class.to_ascii_lowercase(), Some(Rc::new(doc)));
    }
    let aliases = resolver.doc(&class).map_or_else(Vec::new, |doc| {
        let names = doc.names();
        names
            .iter()
            .filter_map(|n| Some((n.clone(), resolver.local(n, &doc.alias_class, &names)?)))
            .collect()
    });
    c.resolving_aliases.store(false, Relaxed);
    aliases
}

struct AliasScope<'r, 'c, 'a, 'arena> {
    resolver: &'r Resolver<'c, 'a, 'arena>,
    doc: &'r AliasDoc,
    names: HashSet<String>,
    bypass: bool,
}

impl Scope for AliasScope<'_, '_, '_, '_> {
    fn class(&self) -> Option<&str> {
        self.doc.scope.class()
    }
    fn parent(&self) -> Option<String> {
        self.doc.scope.parent()
    }
    fn resolve_name(&self, name: &str) -> String {
        self.doc.scope.resolve_name(name)
    }
    fn constant_names(&self, name: &str) -> Vec<String> {
        self.doc.scope.constant_names(name)
    }
    fn template(&self, name: &str) -> Option<Type> {
        let alias = (!self.bypass)
            .then(|| {
                self.resolver
                    .local(name, &self.doc.alias_class, &self.names)
            })
            .flatten();
        alias.or_else(|| self.doc.scope.template(name))
    }
}

fn described(class: &symbols::Class) -> &'static str {
    if class.is_interface {
        "an interface"
    } else if class.is_trait {
        "a trait"
    } else if class.is_enum {
        "an enum"
    } else {
        "a class"
    }
}

pub(super) fn each(ty: &Type, f: &mut dyn FnMut(&Type)) {
    f(ty);
    let f = RefCell::new(f);
    ty.traverse(&|inner| {
        each(inner, &mut *f.borrow_mut());
        inner.clone()
    });
}

fn referenced_classes(ty: &Type) -> Vec<String> {
    let mut out = Vec::new();
    each(ty, &mut |t| match t {
        Type::Object(name, _, _)
        | Type::Static(name, _)
        | Type::This(name)
        | Type::EnumCase(name, _)
            if !out.contains(name) =>
        {
            out.push(name.clone());
        }
        _ => {}
    });
    out
}

type Found = (&'static str, String, Option<&'static str>);

fn definition_context(r: &Resolver, k: &ClassLikeMetadata, display: &str, out: &mut Vec<Found>) {
    let Some(doc) = r.doc(&original_name(k)) else {
        return;
    };
    let resolve_name = |n: &str| doc.scope.resolve_name(n);
    let exists_as = |name: &str| symbols::lookup(&resolve_name(name)).map(|k| described(&k));
    let mut imported = Vec::new();
    for import in &doc.imports {
        let name = import.imported_as.as_deref().unwrap_or(&import.alias);
        if !r.has_class(&import.from) {
            out.push((
                "class.notFound",
                format!(
                    "Cannot import type alias {}: class {} does not exist.",
                    import.alias, import.from
                ),
                None,
            ));
            continue;
        }
        let exported = r.type_aliases(&import.from);
        if !exported.iter().any(|(n, _)| *n == import.alias) {
            out.push((
                "typeAlias.notFound",
                format!(
                    "Cannot import type alias {}: type alias does not exist in {}.",
                    import.alias, import.from
                ),
                None,
            ));
            continue;
        }
        if let Some(kind) = exists_as(name) {
            out.push((
                "typeAlias.duplicate",
                format!("Type alias {name} already exists as {kind} in scope of {display}."),
                None,
            ));
            continue;
        }
        if pun_phpstan::resolve::has_global_alias(name) {
            out.push((
                "typeAlias.duplicate",
                format!("Type alias {name} already exists as a global type alias."),
                None,
            ));
            continue;
        }
        if let Some(as_) = &import.imported_as
            && !valid_name(r, &doc, as_)
        {
            out.push((
                "typeAlias.invalidName",
                format!(
                    "Imported type alias {} has an invalid name: {as_}.",
                    import.alias
                ),
                None,
            ));
            continue;
        }
        imported.push(name.to_string());
    }
    for tag in &doc.aliases {
        let name = &tag.name;
        if imported.contains(name) {
            out.push((
                "typeAlias.duplicate",
                format!("Type alias {name} overwrites an imported type alias of the same name."),
                None,
            ));
            continue;
        }
        if let Some(kind) = exists_as(name) {
            out.push((
                "typeAlias.duplicate",
                format!("Type alias {name} already exists as {kind} in scope of {display}."),
                None,
            ));
            continue;
        }
        if pun_phpstan::resolve::has_global_alias(name) {
            out.push((
                "typeAlias.duplicate",
                format!("Type alias {name} already exists as a global type alias."),
                None,
            ));
            continue;
        }
        if !valid_name(r, &doc, name) {
            out.push((
                "typeAlias.invalidName",
                format!("Type alias has an invalid name: {name}."),
                None,
            ));
            continue;
        }
        let ty = fresh(r, &doc, tag);
        if let Some(found) = error_type(&ty.0, ty.1, name) {
            out.push(found);
            continue;
        }
        let kind = match k.kind {
            SymbolKind::Interface => "Interface",
            SymbolKind::Trait => "Trait",
            SymbolKind::Enum => "Enum",
            _ => "Class",
        };
        let subject = format!("{kind} {display}");
        out.extend(missing_types::parameter(
            r.c,
            &ty.0,
            &subject,
            &format!("type alias {name}"),
        ));
    }
}

fn fresh(r: &Resolver, doc: &AliasDoc, tag: &AliasTag) -> (Type, bool) {
    r.take_circular();
    let Some(node) = &tag.node else {
        return (Type::Mixed { explicit: true }, false);
    };
    let ty = resolve_in(node, &r.scope(doc, false));
    let circular = r.take_circular();
    (ty, circular)
}

fn valid_name(r: &Resolver, doc: &AliasDoc, name: &str) -> bool {
    let ty = resolve_in(&Node::Ident(name.to_string()), &r.scope(doc, true));
    let object = matches!(
        ty,
        Type::Object(..)
            | Type::Static(..)
            | Type::This(_)
            | Type::ObjectWithoutClass
            | Type::EnumCase(..)
    );
    (object && name != "self" && name != "parent") || matches!(ty, Type::Template(_))
}

fn error_type(ty: &Type, circular: bool, name: &str) -> Option<Found> {
    let mut error = false;
    each(ty, &mut |t| error |= matches!(t, Type::Error));
    if !error {
        return None;
    }
    Some(if circular {
        (
            "typeAlias.circular",
            format!("Circular definition detected in type alias {name}."),
            None,
        )
    } else {
        (
            "typeAlias.invalidType",
            format!("Invalid type definition detected in type alias {name}."),
            None,
        )
    })
}

fn use_context(r: &Resolver, doc: &AliasDoc, out: &mut Vec<Found>) {
    for tag in &doc.aliases {
        let name = &tag.name;
        let (ty, circular) = fresh(r, doc, tag);
        if error_type(&ty, circular, name).is_some() {
            continue;
        }
        for class in referenced_classes(&ty) {
            match symbols::lookup(&class) {
                None => out.push((
                    "class.notFound",
                    format!("Type alias {name} contains unknown class {class}."),
                    Some(DISCOVERING_SYMBOLS_TIP),
                )),
                Some(k) if k.is_trait => out.push((
                    "typeAlias.trait",
                    format!("Type alias {name} contains invalid type {class}."),
                    None,
                )),
                Some(_) => {}
            }
        }
        let mut unresolvable = false;
        each(&ty, &mut |t| {
            unresolvable |= matches!(t, Type::Never(NeverKind::Implicit));
        });
        if unresolvable {
            out.push((
                "typeAlias.unresolvableType",
                format!("Type alias {name} contains unresolvable type."),
                None,
            ));
        }
        let mut generics = Vec::new();
        each(&ty, &mut |t| {
            if let Type::Object(class, args, _) = t
                && !args.is_empty()
            {
                generics.push((class.clone(), t.clone()));
            }
        });
        for (class, generic) in generics {
            if let Some(k) = symbols::lookup(&class)
                && k.templates.is_empty()
            {
                let kind = described(&k)
                    .trim_start_matches("an ")
                    .trim_start_matches("a ");
                out.push((
                    "generics.notGeneric",
                    format!(
                        "Type alias {name} contains generic type {} but {kind} {} is not generic.",
                        generic.describe(Level::TypeOnly),
                        k.name
                    ),
                    None,
                ));
            }
        }
    }
}

fn report(c: &mut Checker, at: Span, found: Vec<Found>) {
    for (identifier, message, tip) in found {
        c.report(at, identifier, message, tip);
    }
}

pub(super) fn class_like(c: &mut Checker, at: Span) {
    let mut found = Vec::new();
    without_aliases(c, || {
        with_classes(c, || {
            let r = Resolver::new(c);
            let k = c.class_scope().expect("a class scope");
            let display = original_name(k);
            definition_context(&r, k, &display, &mut found);
            if let Some(doc) = r.doc(&display) {
                use_context(&r, &doc, &mut found);
            }
        });
    });
    report(c, at, found);
}

pub(super) fn trait_definition(c: &mut Checker, at: Span, name: &str) {
    let mut found = Vec::new();
    without_aliases(c, || {
        with_classes(c, || {
            let r = Resolver::new(c);
            if let Some(k) = c.codebase.get_class_like(name.as_bytes()) {
                definition_context(&r, k, &original_name(k), &mut found);
            }
        });
    });
    report(c, at, found);
}

fn in_context(c: &Checker, k: &ClassLikeMetadata, using: &ClassLikeMetadata) -> Option<AliasDoc> {
    let mut scope = class_scope(c, k.span, k);
    scope.class = Some(original_name(using));
    scope.parent = using
        .direct_parent_class
        .and_then(|p| c.codebase.get_class_like(p.as_bytes()))
        .map(original_name);
    alias_doc(c, k, scope)
}

pub(super) fn trait_use(c: &mut Checker, at: Span, name: &str, class: &str) {
    let using = c.codebase.get_class_like(class.as_bytes());
    let mut found = Vec::new();
    without_aliases(c, || {
        with_classes(c, || {
            let r = Resolver::new(c);
            let k = c.codebase.get_class_like(name.as_bytes());
            let doc = k.zip(using).and_then(|(k, u)| in_context(c, k, u));
            if let Some(doc) = doc.map(Rc::new) {
                r.docs
                    .borrow_mut()
                    .insert(doc.alias_class.to_ascii_lowercase(), Some(doc.clone()));
                use_context(&r, &doc, &mut found);
            }
        });
    });
    report(c, at, found);
}
