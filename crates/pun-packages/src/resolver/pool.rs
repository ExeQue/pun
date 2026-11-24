use super::filters::{Advisory, Filters, ListEntry};
use super::fx;
use super::package::{Origin, Package, alias_package, load};
use crate::constraint::{Bound, Constraint, Op};
use crate::error::{Result, bail};
use crate::version::{is_platform, name_matches};
use indexmap::IndexMap;
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Debug, Clone, Default)]
pub struct Stabilities {
    pub minimum: u8,
    pub flags: HashMap<String, u8>,
}

impl Stabilities {
    pub fn accepts(&self, names: &[&str], stability: u8) -> bool {
        names.iter().any(|name| match self.flags.get(*name) {
            Some(flag) => stability <= *flag,
            None => stability <= self.minimum,
        })
    }
}

pub type Loaded = HashMap<String, HashSet<String>>;

pub struct Found {
    pub names: Vec<String>,
    pub packages: Vec<usize>,
}

pub trait Repository {
    fn load(
        &mut self,
        index: usize,
        arena: &mut Vec<Package>,
        batch: &[(String, Constraint)],
        stabilities: &Stabilities,
        loaded: &Loaded,
    ) -> Result<Found>;

    fn advisories(&mut self, names: &[String]) -> Result<Vec<Advisory>>;

    fn list_entries(&mut self, names: &[String]) -> Result<Vec<ListEntry>>;
}

#[derive(Default)]
pub struct ArrayRepository {
    pub packages: Vec<usize>,
    pub pending: Vec<Map<String, Value>>,
    pub advisories: Vec<Advisory>,
    pub entries: Vec<ListEntry>,
}

impl ArrayRepository {
    pub fn of(data: Vec<Map<String, Value>>) -> ArrayRepository {
        ArrayRepository {
            pending: data,
            ..ArrayRepository::default()
        }
    }

    pub fn from_config(config: &Map<String, Value>) -> Result<ArrayRepository> {
        let pending = match config.get("package") {
            Some(Value::Object(p)) => vec![p.clone()],
            other => (other.and_then(Value::as_array).into_iter().flatten())
                .filter_map(|p| p.as_object().cloned())
                .collect(),
        };
        let mut repo = ArrayRepository::of(pending);
        for (name, list) in config
            .get("security-advisories")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            for a in list
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_object)
            {
                repo.advisories.push(Advisory::parse(name, a)?);
            }
        }
        for (list, items) in config
            .get("filter")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            for e in items
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_object)
            {
                repo.entries.push(ListEntry::parse(list, e)?);
            }
        }
        Ok(repo)
    }
}

impl Repository for ArrayRepository {
    fn load(
        &mut self,
        index: usize,
        arena: &mut Vec<Package>,
        batch: &[(String, Constraint)],
        stabilities: &Stabilities,
        loaded: &Loaded,
    ) -> Result<Found> {
        for data in std::mem::take(&mut self.pending) {
            self.packages
                .extend(load(data, Origin::Repository(index), arena)?);
        }
        let mut names: Vec<String> = vec![];
        let mut result: IndexMap<usize, ()> = IndexMap::new();
        for &i in &self.packages {
            let p = &arena[i];
            let Some((_, constraint)) = batch.iter().find(|(n, _)| *n == p.name) else {
                continue;
            };
            if constraint.matches(&Constraint::exact(&p.version))
                && stabilities.accepts(&p.names(true), p.stability)
                && !loaded.get(&p.name).is_some_and(|v| v.contains(&p.version))
            {
                result.insert(i, ());
                if let Some(of) = p.alias_of {
                    result.entry(of).or_insert(());
                }
            }
            if !names.contains(&p.name) {
                names.push(p.name.clone());
            }
        }
        for &i in &self.packages {
            if let Some(of) = arena[i].alias_of
                && result.contains_key(&of)
            {
                result.entry(i).or_insert(());
            }
        }
        Ok(Found {
            names,
            packages: result.into_keys().collect(),
        })
    }

    fn advisories(&mut self, names: &[String]) -> Result<Vec<Advisory>> {
        Ok(self
            .advisories
            .iter()
            .filter(|a| names.contains(&a.package))
            .cloned()
            .collect())
    }

    fn list_entries(&mut self, names: &[String]) -> Result<Vec<ListEntry>> {
        Ok(self
            .entries
            .iter()
            .filter(|e| names.contains(&e.package))
            .cloned()
            .collect())
    }
}

pub struct Filtered<'a> {
    inner: Box<dyn Repository + 'a>,
    only: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    canonical: bool,
}

pub fn filtered<'a>(
    inner: Box<dyn Repository + 'a>,
    config: &Map<String, Value>,
) -> Result<Box<dyn Repository + 'a>> {
    let names = |key: &str| -> Result<Option<Vec<String>>> {
        match config.get(key) {
            None => Ok(None),
            Some(Value::Array(list)) => Ok(Some(
                list.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_ascii_lowercase)
                    .collect(),
            )),
            Some(_) => bail!("\"{key}\" key for repository should be an array"),
        }
    };
    let (only, exclude) = (names("only")?, names("exclude")?);
    if only.is_some() && exclude.is_some() {
        bail!("Only one of \"only\" and \"exclude\" can be specified for a repository");
    }
    let canonical = match config.get("canonical") {
        None => true,
        Some(Value::Bool(b)) => *b,
        Some(_) => bail!("\"canonical\" key for repository should be a boolean"),
    };
    if only.is_none() && exclude.is_none() && canonical {
        return Ok(inner);
    }
    Ok(Box::new(Filtered {
        inner,
        only,
        exclude,
        canonical,
    }))
}

impl Filtered<'_> {
    fn allowed(&self, name: &str) -> bool {
        match (&self.only, &self.exclude) {
            (Some(only), _) => only.iter().any(|p| name_matches(p, name)),
            (_, Some(exclude)) => !exclude.iter().any(|p| name_matches(p, name)),
            _ => true,
        }
    }
}

impl Repository for Filtered<'_> {
    fn load(
        &mut self,
        index: usize,
        arena: &mut Vec<Package>,
        batch: &[(String, Constraint)],
        stabilities: &Stabilities,
        loaded: &Loaded,
    ) -> Result<Found> {
        let batch: Vec<(String, Constraint)> =
            batch.iter().filter(|(n, _)| self.allowed(n)).cloned().collect();
        if batch.is_empty() {
            return Ok(Found {
                names: vec![],
                packages: vec![],
            });
        }
        let mut found = self.inner.load(index, arena, &batch, stabilities, loaded)?;
        if !self.canonical {
            found.names.clear();
        }
        Ok(found)
    }

    /// As Composer's `FilterRepository`, only the names the repository may supply are asked for, and an entry for
    /// another name cannot remove a package another repository supplies.
    fn advisories(&mut self, names: &[String]) -> Result<Vec<Advisory>> {
        let names: Vec<String> = names.iter().filter(|n| self.allowed(n)).cloned().collect();
        let found = self.inner.advisories(&names)?;
        Ok(found.into_iter().filter(|a| self.allowed(&a.package)).collect())
    }

    fn list_entries(&mut self, names: &[String]) -> Result<Vec<ListEntry>> {
        let names: Vec<String> = names.iter().filter(|n| self.allowed(n)).cloned().collect();
        let found = self.inner.list_entries(&names)?;
        Ok(found.into_iter().filter(|e| self.allowed(&e.package)).collect())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transitive {
    #[default]
    OnlyListed,
    Dependencies,
    All,
}

#[derive(Debug, Default)]
pub struct Request {
    pub requires: IndexMap<String, Constraint>,
    pub fixed: Vec<usize>,
    pub locked: Vec<usize>,
    pub lock: Option<Vec<usize>>,
    pub allow_list: Vec<String>,
    pub transitive: Transitive,
}

impl Request {
    pub fn is_fixed(&self, p: usize) -> bool {
        self.fixed.contains(&p)
    }

    pub fn is_locked(&self, p: usize) -> bool {
        self.locked.contains(&p)
    }

    pub fn in_allow_list(&self, name: &str) -> bool {
        self.allow_list
            .iter()
            .any(|pattern| name_matches(pattern, name))
    }

    fn fixed_or_locked(&self) -> Vec<usize> {
        self.fixed.iter().chain(&self.locked).copied().collect()
    }
}

/// The package ids that provide a name, by the name and constraint asked for.
type Providers = HashMap<(String, Option<Constraint>), Rc<[i32]>>;

pub struct Pool {
    pub arena: Vec<Package>,
    pub ids: Vec<usize>,
    id_of: HashMap<usize, i32>,
    by_name: HashMap<String, Vec<i32>>,
    cache: RefCell<Providers>,
    pub unacceptable: Vec<usize>,
    pub removed: Vec<(usize, String)>,
}

impl Pool {
    pub fn new(arena: Vec<Package>, packages: Vec<usize>, unacceptable: Vec<usize>) -> Pool {
        let mut by_name: HashMap<String, Vec<i32>> = HashMap::new();
        let mut id_of = HashMap::new();
        for (id, &p) in (1..).zip(&packages) {
            id_of.insert(p, id);
            for name in arena[p].names(true) {
                by_name.entry(name.to_string()).or_default().push(id);
            }
        }
        Pool {
            arena,
            ids: packages,
            id_of,
            by_name,
            cache: RefCell::default(),
            unacceptable,
            removed: vec![],
        }
    }

    #[must_use]
    pub fn with_packages(self, packages: Vec<usize>) -> Pool {
        let removed = self.removed;
        Pool {
            removed,
            ..Pool::new(self.arena, packages, self.unacceptable)
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn package(&self, literal: i32) -> &Package {
        &self.arena[self.ids[literal.unsigned_abs() as usize - 1]]
    }

    pub fn index(&self, literal: i32) -> usize {
        self.ids[literal.unsigned_abs() as usize - 1]
    }

    pub fn id(&self, index: usize) -> Option<i32> {
        self.id_of.get(&index).copied()
    }

    pub fn what_provides(&self, name: &str, constraint: Option<&Constraint>) -> Rc<[i32]> {
        let key = (name.to_string(), constraint.cloned());
        if let Some(hit) = self.cache.borrow().get(&key) {
            return hit.clone();
        }
        let ids: Rc<[i32]> = (self.by_name.get(name).into_iter().flatten())
            .copied()
            .filter(|&id| self.provides(self.package(id), name, constraint))
            .collect();
        self.cache.borrow_mut().insert(key, ids.clone());
        ids
    }

    pub fn provides(
        &self,
        candidate: &Package,
        name: &str,
        constraint: Option<&Constraint>,
    ) -> bool {
        if candidate.name == name {
            return constraint.is_none_or(|c| c.matches(&Constraint::exact(&candidate.version)));
        }
        (candidate.provides.iter().chain(&candidate.replaces))
            .any(|l| l.target == name && constraint.is_none_or(|c| c.matches(&l.constraint)))
    }
}

fn subset(a: &Constraint, b: &Constraint) -> bool {
    if let Constraint::Multi(items, false) = a {
        return within_member(a, None, b) || items.iter().all(|i| subset(i, b));
    }
    within_member(a, simple(a).then(|| a.bounds()), b)
}

fn within_member(a: &Constraint, bounds: Option<(Bound<'_>, Bound<'_>)>, b: &Constraint) -> bool {
    if a == b || *b == Constraint::Any {
        return true;
    }
    if let Constraint::Multi(items, false) = b {
        return items.iter().any(|i| within_member(a, bounds.clone(), i));
    }
    match bounds {
        Some((al, au)) if simple(b) => {
            let (bl, bu) = b.bounds();
            !bl.beyond(&al, true) && !bu.beyond(&au, false)
        }
        _ => false,
    }
}

fn simple(c: &Constraint) -> bool {
    let single = |c: &Constraint| matches!(c, Constraint::Single(op, v) if *op != Op::Ne && !v.starts_with("dev-"));
    match c {
        Constraint::Multi(items, true) => items.iter().all(single),
        c => single(c),
    }
}

fn union(existing: &Constraint, c: Constraint) -> Constraint {
    let mut items = match existing {
        Constraint::Multi(items, false) => items.clone(),
        other => vec![other.clone()],
    };
    items.push(c);
    Constraint::Multi(items, false)
}

pub type RootAliases = HashMap<String, HashMap<String, (String, String)>>;

pub struct PoolBuilder<'a> {
    pub stabilities: &'a Stabilities,
    pub root_aliases: &'a RootAliases,
    pub root_references: &'a HashMap<String, String>,
    pub ignored_types: &'a [&'a str],
    pub filters: &'a Filters,
    pub now: i64,
    alias_map: fx::HashMap<usize, Vec<(usize, usize)>>,
    to_load: fx::IndexMap<String, Constraint>,
    loaded: fx::HashMap<String, Constraint>,
    loaded_per_repo: HashMap<usize, Loaded>,
    packages: fx::IndexMap<usize, usize>,
    unacceptable: Vec<usize>,
    skipped: fx::HashMap<String, Vec<usize>>,
    path_unlocked: HashSet<String>,
    /// The names `unlock` is working on: replacements that form a cycle unlock each name once.
    unlocking: HashSet<String>,
    max_extended: fx::HashSet<String>,
    covered: fx::HashMap<String, fx::HashSet<Constraint>>,
    counter: usize,
}

impl<'a> PoolBuilder<'a> {
    pub fn new(
        stabilities: &'a Stabilities,
        root_aliases: &'a RootAliases,
        root_references: &'a HashMap<String, String>,
        ignored_types: &'a [&'a str],
        filters: &'a Filters,
        now: i64,
    ) -> PoolBuilder<'a> {
        PoolBuilder {
            stabilities,
            root_aliases,
            root_references,
            ignored_types,
            filters,
            now,
            alias_map: fx::HashMap::default(),
            to_load: fx::IndexMap::default(),
            loaded: fx::HashMap::default(),
            loaded_per_repo: HashMap::new(),
            packages: fx::IndexMap::default(),
            unacceptable: vec![],
            skipped: fx::HashMap::default(),
            path_unlocked: HashSet::new(),
            unlocking: HashSet::new(),
            max_extended: fx::HashSet::default(),
            covered: fx::HashMap::default(),
            counter: 0,
        }
    }

    pub fn build(
        mut self,
        mut arena: Vec<Package>,
        repositories: &mut [Box<dyn Repository + '_>],
        request: &mut Request,
    ) -> Result<Pool> {
        if !request.allow_list.is_empty() {
            for &p in &request.lock.clone().unwrap_or_default() {
                let package = &arena[p];
                if request.in_allow_list(&package.name) {
                    continue;
                }
                self.skipped
                    .entry(package.name.clone())
                    .or_default()
                    .push(p);
                for link in &package.replaces {
                    self.skipped.entry(link.target.clone()).or_default().push(p);
                }
                let data = package.data();
                if data.get("dist").and_then(|d| d.get("type")) == Some(&Value::from("path"))
                    && (data.get("transport-options"))
                        .and_then(|o| o.get("symlink"))
                        .and_then(serde_json::Value::as_bool)
                        != Some(false)
                {
                    self.path_unlocked.insert(package.name.clone());
                    continue;
                }
                request.locked.push(p);
            }
        }
        for p in request.fixed_or_locked() {
            let package = &arena[p];
            self.loaded.insert(package.name.clone(), Constraint::Any);
            for link in &package.replaces {
                self.loaded.insert(link.target.clone(), Constraint::Any);
            }
            if matches!(package.origin, Origin::Root | Origin::Platform)
                || self
                    .stabilities
                    .accepts(&package.names(true), package.stability)
            {
                self.load_package(&mut arena, request, p, false);
            } else {
                self.unacceptable.push(p);
            }
        }
        for (name, constraint) in &request.requires {
            if self.loaded.contains_key(name) {
                continue;
            }
            self.to_load.insert(name.clone(), constraint.clone());
            self.max_extended.insert(name.clone());
        }
        let loaded = &self.loaded;
        self.to_load.retain(|name, _| !loaded.contains_key(name));
        while !self.to_load.is_empty() {
            self.load_marked(&mut arena, repositories, request)?;
        }
        let packages: Vec<usize> = self.packages.values().copied().collect();
        let (packages, removed) = self.filter(&arena, packages, repositories, request)?;
        let mut pool = Pool::new(arena, packages, self.unacceptable);
        pool.removed = removed;
        Ok(pool)
    }

    #[expect(clippy::type_complexity, reason = "the kept and the removed packages")]
    fn filter(
        &self,
        arena: &[Package],
        packages: Vec<usize>,
        repositories: &mut [Box<dyn Repository + '_>],
        request: &Request,
    ) -> Result<(Vec<usize>, Vec<(usize, String)>)> {
        let f = self.filters;
        let mut names = indexmap::IndexSet::new();
        for &p in &packages {
            // As Composer's pool filters, the repositories are asked about the name of each package in the pool,
            // except the root and platform packages.
            if matches!(arena[p].origin, Origin::Root | Origin::Platform) || is_platform(&arena[p].name) {
                continue;
            }
            names.insert(arena[p].name.clone());
        }
        let names: Vec<String> = names.into_iter().collect();
        let (mut advisories, mut entries) = (vec![], vec![]);
        for repo in repositories.iter_mut() {
            if f.wants_advisories() {
                advisories.extend(repo.advisories(&names)?);
            }
            if f.wants_lists() {
                entries.extend(repo.list_entries(&names)?);
            }
        }
        let locked: HashSet<(&str, &str)> = (request.lock.iter().flatten())
            .map(|&p| (arena[p].name.as_str(), arena[p].version.as_str()))
            .collect();
        let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, a) in advisories.iter().enumerate() {
            by_name.entry(&a.package).or_default().push(i);
        }
        let (mut kept, mut removed) = (vec![], vec![]);
        for p in packages {
            let package = &arena[p];
            let is_locked =
                request.is_locked(p) || package.alias_of.is_some_and(|of| request.is_locked(of));
            let base = &arena[package.alias_of.unwrap_or(p)];
            let equivalent = is_locked || locked.contains(&(&package.name, &package.version));
            let target = if package.root_alias { base } else { package };
            let mut relevant: Vec<usize> = (target.names(false).into_iter())
                .flat_map(|n| by_name.get(n).into_iter().flatten().copied())
                .collect();
            relevant.sort_unstable();
            relevant.dedup();
            let relevant: Vec<&Advisory> = relevant.iter().map(|&i| &advisories[i]).collect();
            match f.removal(
                package, base, &relevant, &entries, equivalent, is_locked, self.now,
            ) {
                Some(why) => removed.push((p, why)),
                None => kept.push(p),
            }
        }
        Ok((kept, removed))
    }

    fn mark(&mut self, request: &Request, name: &str, constraint: &Constraint) {
        if (self.covered.get(name)).is_some_and(|c| c.contains(constraint)) {
            return;
        }
        let original = constraint;
        let covered = |builder: &mut Self| {
            (builder.covered.entry(name.to_string()).or_default()).insert(original.clone());
        };
        if is_platform(name) || self.max_extended.contains(name) {
            return covered(self);
        }
        let constraint = match request.requires.get(name) {
            Some(root) if !subset(constraint, root) => root,
            _ => constraint,
        };
        let Some(loaded) = self.loaded.get(name) else {
            let constraint = match self.to_load.get(name) {
                Some(pending) if subset(constraint, pending) => return covered(self),
                Some(pending) => union(pending, constraint.clone()),
                None => constraint.clone(),
            };
            self.to_load.insert(name.to_string(), constraint);
            return;
        };
        if subset(constraint, loaded) {
            return covered(self);
        }
        let merged = union(loaded, constraint.clone());
        self.to_load.shift_remove(name);
        self.to_load.insert(name.to_string(), merged);
        self.loaded.remove(name);
    }

    fn load_marked(
        &mut self,
        arena: &mut Vec<Package>,
        repositories: &mut [Box<dyn Repository + '_>],
        request: &mut Request,
    ) -> Result<()> {
        for (name, constraint) in &self.to_load {
            self.loaded.insert(name.clone(), constraint.clone());
        }
        let pending: Vec<(String, Constraint)> =
            std::mem::take(&mut self.to_load).into_iter().collect();
        let mut batches: Vec<Vec<(String, Constraint)>> =
            pending.chunks(50).map(<[_]>::to_vec).collect();
        for (i, repository) in repositories.iter_mut().enumerate() {
            if batches.is_empty() {
                break;
            }
            let repo = i;
            for batch in &mut batches {
                let empty = Loaded::new();
                let already = self.loaded_per_repo.get(&repo).unwrap_or(&empty);
                let found = repository.load(repo, arena, batch, self.stabilities, already)?;
                batch.retain(|(n, _)| !found.names.contains(n));
                for p in found.packages {
                    let package = &arena[p];
                    (self.loaded_per_repo.entry(repo).or_default())
                        .entry(package.name.clone())
                        .or_default()
                        .insert(package.version.clone());
                    if self
                        .ignored_types
                        .contains(&package.kind.to_ascii_lowercase().as_str())
                    {
                        continue;
                    }
                    let propagate = !self.path_unlocked.contains(&package.name);
                    self.load_package(arena, request, p, propagate);
                }
            }
            let rest: Vec<(String, Constraint)> = batches.into_iter().flatten().collect();
            batches = rest.chunks(50).map(<[_]>::to_vec).collect();
        }
        Ok(())
    }

    fn load_package(
        &mut self,
        arena: &mut Vec<Package>,
        request: &mut Request,
        p: usize,
        propagate: bool,
    ) {
        let index = self.counter;
        self.counter += 1;
        self.packages.insert(index, p);
        if let Some(of) = arena[p].alias_of {
            self.alias_map.entry(of).or_default().push((index, p));
        }
        let name = arena[p].name.clone();
        if let Some(reference) = self.root_references.get(&name)
            && !request.is_locked(p)
            && !request.is_fixed(p)
        {
            let base = arena[p].alias_of.unwrap_or(p);
            set_references(&mut arena[base], reference);
        }
        let version = arena[p].version.clone();
        if (propagate || self.path_unlocked.contains(&name))
            && let Some((pretty, normalized)) =
                self.root_aliases.get(&name).and_then(|v| v.get(&version))
        {
            let base = arena[p].alias_of.unwrap_or(p);
            let alias = alias_package(arena, base, normalized.clone(), pretty.clone(), true);
            let index = self.counter;
            self.counter += 1;
            self.packages.insert(index, alias);
            self.alias_map.entry(base).or_default().push((index, alias));
        }
        // A link to a skipped package can unlock it, which changes the arena; any other only marks its target.
        if (arena[p].requires.iter()).any(|l| self.skipped.contains_key(&l.target)) {
            let requires = arena[p].requires.clone();
            for link in &requires {
                if self.skipped.contains_key(&link.target) {
                    if propagate && request.transitive != Transitive::OnlyListed {
                        let skipped_roots =
                            self.skipped_root_requires(arena, request, &link.target);
                        if request.transitive == Transitive::All || skipped_roots.is_empty() {
                            self.unlock(arena, request, &link.target);
                            self.mark(request, &link.target, &link.constraint);
                        }
                    } else if self.path_unlocked.contains(&link.target)
                        && !self.loaded.contains_key(&link.target)
                    {
                        self.mark(request, &link.target, &link.constraint);
                    }
                } else {
                    self.mark(request, &link.target, &link.constraint);
                }
            }
        } else {
            for link in &arena[p].requires {
                self.mark(request, &link.target, &link.constraint);
            }
        }
        if propagate && request.transitive != Transitive::OnlyListed {
            let replaces = arena[p].replaces.clone();
            for link in &replaces {
                let target = &link.target;
                if self.loaded.contains_key(target) && self.skipped.contains_key(target) {
                    let skipped_roots = self.skipped_root_requires(arena, request, target);
                    if request.transitive == Transitive::All || skipped_roots.is_empty() {
                        self.unlock(arena, request, target);
                        self.mark_if_required(arena, request, target);
                    }
                }
            }
        }
    }

    fn mark_if_required(&mut self, arena: &[Package], request: &Request, name: &str) {
        if let Some(c) = request.requires.get(name) {
            let c = c.clone();
            self.mark(request, name, &c);
        }
        let links: Vec<_> = (self.packages.values())
            .flat_map(|&p| {
                arena[p]
                    .requires
                    .iter()
                    .filter(|l| l.target == name)
                    .cloned()
            })
            .collect();
        for link in links {
            self.mark(request, &link.target, &link.constraint);
        }
    }

    fn skipped_root_requires(
        &self,
        arena: &[Package],
        request: &Request,
        name: &str,
    ) -> Vec<String> {
        let skipped = &self.skipped[name];
        if request.requires.contains_key(name) {
            return skipped.iter().map(|&p| arena[p].name.clone()).collect();
        }
        let mut out = vec![];
        for &p in skipped {
            let package = &arena[p];
            if request.requires.contains_key(&package.name) {
                out.push(package.name.clone());
            }
            if package
                .replaces
                .iter()
                .any(|l| request.requires.contains_key(&l.target))
            {
                out.push(package.name.clone());
            }
        }
        out
    }

    fn is_root_require(request: &Request, name: &str) -> bool {
        request.requires.contains_key(name)
    }

    fn unlock(&mut self, arena: &mut Vec<Package>, request: &mut Request, name: &str) {
        if !self.unlocking.insert(name.to_string()) {
            return;
        }
        self.unlock_name(arena, request, name);
        self.unlocking.remove(name);
    }

    fn unlock_name(&mut self, arena: &mut Vec<Package>, request: &mut Request, name: &str) {
        for p in self.skipped.get(name).cloned().unwrap_or_default() {
            let replacer = arena[p].name.clone();
            if replacer != name
                && self.skipped.contains_key(&replacer)
                && (request.transitive == Transitive::All
                    || (!Self::is_root_require(request, name)
                        && !Self::is_root_require(request, &replacer)))
            {
                self.unlock(arena, request, &replacer);
                if Self::is_root_require(request, &replacer) {
                    self.mark(request, &replacer, &Constraint::Any);
                } else {
                    let links: Vec<_> = (self.packages.values())
                        .filter_map(|&q| {
                            arena[q]
                                .requires
                                .iter()
                                .find(|l| l.target == replacer)
                                .cloned()
                        })
                        .collect();
                    for link in links {
                        self.mark(request, &replacer, &link.constraint);
                    }
                }
            }
        }
        if self.path_unlocked.contains(name) {
            let matching: Vec<(usize, usize)> = (self.packages.iter())
                .filter(|(_, p)| arena[**p].name == name)
                .map(|(i, p)| (*i, *p))
                .collect();
            for (index, p) in matching {
                self.remove_loaded(arena, p, index);
            }
        }
        self.skipped.remove(name);
        self.loaded.remove(name);
        self.covered.remove(name);
        self.max_extended.remove(name);
        self.path_unlocked.remove(name);
        let locked = request.locked.clone();
        for p in locked {
            if arena[p].alias_of.is_some() || arena[p].name != name {
                continue;
            }
            let Some(index) = self
                .packages
                .iter()
                .find(|(_, q)| **q == p)
                .map(|(i, _)| *i)
            else {
                continue;
            };
            request.locked.retain(|&q| q != p);
            self.remove_loaded(arena, p, index);
            for other in request.fixed_or_locked() {
                if other == p || !self.skipped.contains_key(&arena[other].name) {
                    continue;
                }
                if let Some(link) = arena[other]
                    .requires
                    .iter()
                    .find(|l| l.target == arena[p].name)
                    .cloned()
                {
                    self.mark(request, &link.target, &link.constraint);
                }
                let replaces = arena[p].replaces.clone();
                for replace in replaces {
                    if arena[other]
                        .requires
                        .iter()
                        .any(|l| l.target == replace.target)
                        && self.skipped.contains_key(&replace.target)
                    {
                        self.unlock(arena, request, &replace.target);
                        self.mark(request, &replace.target, &replace.constraint);
                    }
                }
            }
        }
    }

    fn remove_loaded(&mut self, arena: &[Package], p: usize, index: usize) {
        let repo = match arena[p].origin {
            Origin::Repository(i) => i,
            Origin::Root | Origin::Platform | Origin::Locked => usize::MAX,
        };
        let mut forget = |q: usize| {
            if let Some(versions) = self
                .loaded_per_repo
                .get_mut(&repo)
                .and_then(|l| l.get_mut(&arena[q].name))
            {
                versions.remove(&arena[q].version);
            }
        };
        forget(p);
        self.packages.shift_remove(&index);
        if let Some(aliases) = self.alias_map.remove(&p) {
            for (alias_index, alias) in aliases {
                forget(alias);
                self.packages.shift_remove(&alias_index);
            }
        }
    }
}

fn set_references(p: &mut Package, reference: &str) {
    use serde_json::Value;
    static HOSTS: std::sync::LazyLock<pcre2::bytes::Regex> = std::sync::LazyLock::new(|| {
        pcre2::bytes::Regex::new(
            r"(?i)^https?://(?:(?:www\.)?bitbucket\.org|(api\.)?github\.com|(?:www\.)?gitlab\.com|codeberg\.org)/",
        )
        .expect("valid pattern")
    });
    static SHA: std::sync::LazyLock<pcre2::bytes::Regex> = std::sync::LazyLock::new(|| {
        pcre2::bytes::Regex::new(r"(?i)(?<=/|sha=)[a-f0-9]{40}(?=/|(?:\.zip)?(?:$|\?))")
            .expect("valid pattern")
    });
    let mut data = p.data();
    if let Some(Value::Object(source)) = data.get_mut("source") {
        source.insert("reference".into(), reference.into());
    }
    if let Some(Value::Object(dist)) = data.get_mut("dist") {
        let url = dist.get("url").and_then(Value::as_str).map(String::from);
        if let Some(url) = url.filter(|u| HOSTS.is_match(u.as_bytes()).unwrap_or(false)) {
            dist.insert("reference".into(), reference.into());
            let mut replaced = String::new();
            let mut last = 0;
            for m in SHA.find_iter(url.as_bytes()).flatten() {
                replaced.push_str(&url[last..m.start()]);
                replaced.push_str(reference);
                last = m.end();
            }
            replaced.push_str(&url[last..]);
            dist.insert("url".into(), replaced.into());
        } else if dist
            .get("reference")
            .and_then(Value::as_str)
            .is_some_and(|r| !r.is_empty())
        {
            dist.insert("reference".into(), reference.into());
        }
    }
    p.data = std::sync::Arc::from(Value::Object(data).to_string());
}
