//! The pool: every package version that could be part of the solution, numbered from 1. It is built from the
//! request by loading, repository by repository, the versions of each required name that match what is asked for,
//! then the names those versions require, until nothing new is asked for.

use super::filters::{Advisory, Filters, ListEntry};
use super::package::{Origin, Package, alias_package, load};
use crate::constraint::{Bound, Constraint, Op};
use crate::version::{is_platform, name_matches};
use anyhow::Result;
use indexmap::IndexMap;
use serde_json::{Map, Value};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// The stabilities a project accepts: everything up to `minimum`, and per package its stability flag.
#[derive(Debug, Clone, Default)]
pub struct Stabilities {
    pub minimum: u8,
    pub flags: HashMap<String, u8>,
}

impl Stabilities {
    /// `StabilityFilter::isPackageAcceptable`.
    pub fn accepts(&self, names: &[&str], stability: u8) -> bool {
        names.iter().any(|name| match self.flags.get(*name) {
            Some(flag) => stability <= *flag,
            None => stability <= self.minimum,
        })
    }
}

/// Versions already loaded from a repository, by name.
pub type Loaded = HashMap<String, HashSet<String>>;

/// What a repository returns for a batch of names: the names it has, and the matching packages in its order.
pub struct Found {
    pub names: Vec<String>,
    pub packages: Vec<usize>,
}

/// A source of package versions.
pub trait Repository {
    /// The versions of each name in `batch` that match its constraint and the stabilities, except those in
    /// `loaded`, added to `arena`. `index` is the repository's place in the pool builder's list.
    fn load(
        &mut self,
        index: usize,
        arena: &mut Vec<Package>,
        batch: &[(String, Constraint)],
        stabilities: &Stabilities,
        loaded: &Loaded,
    ) -> Result<Found>;

    /// The security advisories the repository publishes for these names.
    fn advisories(&mut self, names: &[String]) -> Result<Vec<Advisory>>;

    /// The filter-list entries the repository publishes for these names.
    fn list_entries(&mut self, names: &[String]) -> Result<Vec<ListEntry>>;
}

/// A repository listing its packages: the root package, a `package` repository, a lock. Packages given as JSON are
/// loaded into the arena on first use.
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

    /// A `package` repository from its config: `package` (one or a list), `security-advisories` and `filter`.
    pub fn from_config(config: &Map<String, Value>) -> Result<ArrayRepository> {
        // Composer's schema requires `package`, one package or a list of them.
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

/// A repository with `only`, `exclude` or `canonical` set (`FilterRepository`): it is asked only for the names it
/// allows, and a non-canonical one lets later repositories offer the names it has too.
pub struct Filtered<'a> {
    inner: Box<dyn Repository + 'a>,
    only: Option<Vec<String>>,
    exclude: Option<Vec<String>>,
    canonical: bool,
}

/// Wraps `inner` in its repository config's filters, when it has any.
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
            Some(_) => anyhow::bail!("\"{key}\" key for repository should be an array"),
        }
    };
    let (only, exclude) = (names("only")?, names("exclude")?);
    if only.is_some() && exclude.is_some() {
        anyhow::bail!("Only one of \"only\" and \"exclude\" can be specified for a repository");
    }
    let canonical = match config.get("canonical") {
        None => true,
        Some(Value::Bool(b)) => *b,
        Some(_) => anyhow::bail!("\"canonical\" key for repository should be a boolean"),
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

impl Repository for Filtered<'_> {
    fn load(
        &mut self,
        index: usize,
        arena: &mut Vec<Package>,
        batch: &[(String, Constraint)],
        stabilities: &Stabilities,
        loaded: &Loaded,
    ) -> Result<Found> {
        let allowed = |name: &str| match (&self.only, &self.exclude) {
            (Some(only), _) => only.iter().any(|p| name_matches(p, name)),
            (_, Some(exclude)) => !exclude.iter().any(|p| name_matches(p, name)),
            _ => true,
        };
        let batch: Vec<(String, Constraint)> =
            batch.iter().filter(|(n, _)| allowed(n)).cloned().collect();
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

    fn advisories(&mut self, names: &[String]) -> Result<Vec<Advisory>> {
        self.inner.advisories(names)
    }

    fn list_entries(&mut self, names: &[String]) -> Result<Vec<ListEntry>> {
        self.inner.list_entries(names)
    }
}

/// How far a partial update reaches beyond the packages it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transitive {
    #[default]
    OnlyListed,
    /// `--with-dependencies`: their dependencies too, except root requirements.
    Dependencies,
    /// `--with-all-dependencies`.
    All,
}

/// What to solve for.
#[derive(Debug, Default)]
pub struct Request {
    /// Root requirements, lowercase names, in order.
    pub requires: IndexMap<String, Constraint>,
    /// Packages that must be installed as they are: the root and the platform.
    pub fixed: Vec<usize>,
    /// Packages of a partial update that stay at their locked version.
    pub locked: Vec<usize>,
    /// The lock's packages, when there is one.
    pub lock: Option<Vec<usize>>,
    /// The names a partial update may change (patterns with `*`).
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

/// The packages the solver chooses from.
pub struct Pool {
    pub arena: Vec<Package>,
    /// Arena index of each package id, from id 1.
    pub ids: Vec<usize>,
    id_of: HashMap<usize, i32>,
    by_name: HashMap<String, Vec<i32>>,
    cache: RefCell<HashMap<(String, Option<Constraint>), Rc<[i32]>>>,
    /// Fixed or locked packages whose stability the project does not accept.
    pub unacceptable: Vec<usize>,
    /// Packages the policy kept out, with the reason.
    pub removed: Vec<(usize, String)>,
}

impl Pool {
    pub fn new(arena: Vec<Package>, packages: Vec<usize>, unacceptable: Vec<usize>) -> Pool {
        let mut by_name: HashMap<String, Vec<i32>> = HashMap::new();
        let mut id_of = HashMap::new();
        for (i, &p) in packages.iter().enumerate() {
            let id = i as i32 + 1;
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

    /// The same arena with only `packages` in the pool, renumbered from 1.
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

    /// The package with this id or literal.
    pub fn package(&self, literal: i32) -> &Package {
        &self.arena[self.ids[literal.unsigned_abs() as usize - 1]]
    }

    /// The arena index of the package with this id or literal.
    pub fn index(&self, literal: i32) -> usize {
        self.ids[literal.unsigned_abs() as usize - 1]
    }

    /// The id of an arena package, if it is in the pool.
    pub fn id(&self, index: usize) -> Option<i32> {
        self.id_of.get(&index).copied()
    }

    /// The ids of the packages named, providing or replacing `name` in a way that matches `constraint`.
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

    /// Whether `candidate` is, provides or replaces `name` at a version `constraint` admits.
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

/// `Intervals::isSubsetOf` for the shapes that decide pool loading: equal constraints, anything within `*`, a
/// member of a disjunction, and two single ranges. Anything else counts as not a subset, which loads more but never
/// fewer versions.
fn subset(a: &Constraint, b: &Constraint) -> bool {
    if let Constraint::Multi(items, false) = a {
        return within_member(a, None, b) || items.iter().all(|i| subset(i, b));
    }
    within_member(a, simple(a).then(|| a.bounds()), b)
}

/// Whether `a` (with its bounds when it is a single range) equals `b`, lies within one of `b`'s alternatives, or is
/// a single range within `b`'s.
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

/// A single range: comparisons without `!=` or a branch.
fn simple(c: &Constraint) -> bool {
    let single = |c: &Constraint| matches!(c, Constraint::Single(op, v) if *op != Op::Ne && !v.starts_with("dev-"));
    match c {
        Constraint::Multi(items, true) => items.iter().all(single),
        c => single(c),
    }
}

/// Adds `c` to `existing` as Composer's union of the two would. `c` is never already one of `existing`'s
/// alternatives: it would be a subset then, and nothing is merged.
fn union(existing: &Constraint, c: Constraint) -> Constraint {
    let mut items = match existing {
        Constraint::Multi(items, false) => items.clone(),
        other => vec![other.clone()],
    };
    items.push(c);
    Constraint::Multi(items, false)
}

/// The root's `x as y` requirements: name, then aliased version, then (alias pretty, alias normalized).
pub type RootAliases = HashMap<String, HashMap<String, (String, String)>>;

pub struct PoolBuilder<'a> {
    pub stabilities: &'a Stabilities,
    pub root_aliases: &'a RootAliases,
    /// Root requirements pinned to a commit (`dev-main#abc123`).
    pub root_references: &'a HashMap<String, String>,
    /// Package types never installed (`php-ext`, `php-ext-zend`).
    pub ignored_types: &'a [&'a str],
    pub filters: &'a Filters,
    /// The time cooldown periods count back from, in seconds since the epoch.
    pub now: i64,
    alias_map: HashMap<usize, Vec<(usize, usize)>>,
    to_load: IndexMap<String, Constraint>,
    loaded: HashMap<String, Constraint>,
    loaded_per_repo: HashMap<usize, Loaded>,
    packages: IndexMap<usize, usize>,
    unacceptable: Vec<usize>,
    skipped: HashMap<String, Vec<usize>>,
    path_unlocked: HashSet<String>,
    max_extended: HashSet<String>,
    /// The constraints `mark` found already loaded, by name.
    covered: HashMap<String, HashSet<Constraint>>,
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
            alias_map: HashMap::new(),
            to_load: IndexMap::new(),
            loaded: HashMap::new(),
            loaded_per_repo: HashMap::new(),
            packages: IndexMap::new(),
            unacceptable: vec![],
            skipped: HashMap::new(),
            path_unlocked: HashSet::new(),
            max_extended: HashSet::new(),
            covered: HashMap::new(),
            counter: 0,
        }
    }

    /// Builds the pool. `repositories` holds the root package's repository, then the project's repositories in
    /// priority order, numbered as the packages' [`Origin::Repository`]; the platform and lock packages are already
    /// in `arena` and in the request.
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
                        .and_then(|s| s.as_bool())
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

    /// Drops what the policy filters keep out of the pool; returns the kept packages and the removed ones with why.
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
            names.extend(arena[p].names(false).into_iter().map(String::from));
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
        // Each package is checked only against the advisories for its names, in the repositories' order.
        let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, a) in advisories.iter().enumerate() {
            by_name.entry(&a.package).or_default().push(i);
        }
        let (mut kept, mut removed) = (vec![], vec![]);
        for p in packages {
            let package = &arena[p];
            // A locked package is checked only for being abandoned and against the filter lists that block installs.
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
        // Most versions of a package require the same constraints, and a requirement that needs nothing loaded
        // keeps needing nothing until `unlock` forgets the name, so each is checked once.
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
        let requires = arena[p].requires.clone();
        for link in &requires {
            if self.skipped.contains_key(&link.target) {
                if propagate && request.transitive != Transitive::OnlyListed {
                    let skipped_roots = self.skipped_root_requires(arena, request, &link.target);
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

    /// The root requirements among the skipped packages providing `name` (`getSkippedRootRequires`).
    fn skipped_root_requires(
        &self,
        arena: &[Package],
        request: &Request,
        name: &str,
    ) -> Vec<String> {
        // Called only for a skipped name.
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
        // Only a repository's packages are in `loaded_per_repo`; the root is fixed and never removed, and the
        // platform and the lock are never loaded from.
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

/// `setSourceDistReferences`: the source reference, and the dist reference (and URL, for the hosts whose archive
/// URLs carry it) when the dist has one.
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
