use super::package::{Link, Package};
use super::policy::Policy;
use super::pool::{Pool, Request};
use crate::constraint::Constraint;
use indexmap::{IndexMap, IndexSet};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

type ByName<'a> = HashMap<&'a str, IndexSet<&'a Constraint>>;

type GroupKey<'a> = Vec<(bool, &'a Constraint)>;

type Dependencies<'a> = Vec<(u8, &'a str, &'a Constraint)>;

pub fn optimize(pool: &Pool, request: &Request, policy: &Policy) -> Vec<usize> {
    let arena = &pool.arena;

    let mut requires = ByName::new();
    let mut conflicts = ByName::new();
    for (name, c) in &request.requires {
        add(&mut requires, name, c);
    }
    let mut aliases: HashMap<usize, Vec<usize>> = HashMap::new();
    for &p in &pool.ids {
        let package = &arena[p];
        for link in &package.requires {
            add(&mut requires, &link.target, &link.constraint);
        }
        for link in &package.conflicts {
            add(&mut conflicts, &link.target, &link.constraint);
        }
        if let Some(of) = package.alias_of {
            aliases.entry(of).or_default().push(p);
        }
    }

    let mut pinned: HashMap<&str, Vec<Constraint>> = HashMap::new();
    for &p in request.fixed.iter().chain(&request.locked) {
        let package = &arena[p];
        (pinned.entry(&package.name).or_default()).push(Constraint::exact(&package.version));
    }
    let mut irremovable = HashSet::new();
    for &p in &pool.ids {
        let Some(versions) = pinned.get(arena[p].name.as_str()) else {
            continue;
        };
        let version = Constraint::exact(&arena[p].version);
        if versions.iter().any(|v| v.matches(&version)) {
            let mut package = Some(p);
            while let Some(q) = package {
                irremovable.insert(q);
                irremovable.extend(aliases.get(&q).into_iter().flatten());
                package = arena[q].alias_of;
            }
        }
    }

    let grouped: Vec<(usize, Dependencies, Vec<(&str, GroupKey)>)> = (pool.ids.par_iter())
        .filter(|p| !irremovable.contains(*p))
        .map(|&p| {
            let package = &arena[p];
            let version = Constraint::exact(&package.version);
            let replaces: GroupKey = (package.replaces.iter())
                .filter(|l| l.constraint.matches(&version))
                .map(|l| (false, &l.constraint))
                .collect();
            let mut keys = vec![];
            for name in package.names(false) {
                let Some(required) = requires.get(name) else {
                    continue;
                };
                let mut invariant = replaces.clone();
                for &c in conflicts.get(name).into_iter().flatten() {
                    if c.matches(&version) {
                        invariant.push((true, c));
                    }
                }
                let mut in_invariant_group = false;
                for &c in required {
                    if c.matches(&version) {
                        keys.push((
                            name,
                            std::iter::once((false, c))
                                .chain(invariant.iter().copied())
                                .collect(),
                        ));
                    } else if !in_invariant_group && !invariant.is_empty() {
                        in_invariant_group = true;
                        keys.push((name, invariant.clone()));
                    }
                }
            }
            (p, dependencies(package), keys)
        })
        .collect();
    let mut remove: HashSet<usize> = HashSet::new();
    let mut same_dependencies: HashMap<Dependencies, usize> = HashMap::new();
    let mut groups: IndexMap<&str, IndexMap<GroupKey, IndexMap<usize, Vec<usize>>>> =
        IndexMap::new();
    for (p, dependencies, keys) in grouped {
        remove.insert(p);
        let next = same_dependencies.len();
        let dependencies = *same_dependencies.entry(dependencies).or_insert(next);
        for (name, key) in keys {
            (groups.entry(name).or_default().entry(key).or_default())
                .entry(dependencies)
                .or_default()
                .push(p);
        }
    }
    let mut keep = |p: usize| {
        if !remove.remove(&p) {
            return;
        }
        let of = arena[p].alias_of.inspect(|of| {
            remove.remove(of);
        });
        for alias in aliases.get(&of.unwrap_or(p)).into_iter().flatten() {
            remove.remove(alias);
        }
    };
    for same in groups
        .values()
        .flat_map(IndexMap::values)
        .flat_map(IndexMap::values)
    {
        if let [only] = same[..] {
            keep(only);
            continue;
        }
        let literals = (same.iter()).filter_map(|&p| pool.id(p)).collect();
        for literal in policy.select_preferred(pool, literals, None) {
            keep(pool.index(literal));
        }
    }

    if !request.locked.is_empty() {
        let mut candidates: HashMap<&str, Vec<usize>> = HashMap::new();
        for &p in &pool.ids {
            let package = &arena[p];
            if irremovable.contains(&p)
                || aliases.contains_key(&p)
                || package.alias_of.is_some()
                || request.is_fixed(p)
                || request.is_locked(p)
            {
                continue;
            }
            candidates.entry(&package.name).or_default().push(p);
        }
        for &locked in &request.locked {
            let package = &arena[locked];
            if !package
                .names(false)
                .iter()
                .any(|n| requires.contains_key(n))
            {
                continue;
            }
            for link in &package.requires {
                if let Some(versions) = candidates.get_mut(link.target.as_str()) {
                    versions.retain(|&p| {
                        let admitted = link
                            .constraint
                            .matches(&Constraint::exact(&arena[p].version));
                        if !admitted {
                            remove.insert(p);
                        }
                        admitted
                    });
                }
            }
        }
    }

    (pool.ids.iter())
        .copied()
        .filter(|p| !remove.contains(p))
        .collect()
}

fn add<'a>(into: &mut ByName<'a>, name: &'a str, c: &'a Constraint) {
    let set = into.entry(name).or_default();
    let mut stack = vec![c];
    while let Some(c) = stack.pop() {
        match c {
            Constraint::Multi(items, false) => stack.extend(items.iter().rev()),
            _ => {
                set.insert(c);
            }
        }
    }
}

fn dependencies(package: &Package) -> Dependencies<'_> {
    let kinds: [&[Link]; 4] = [
        &package.requires,
        &package.conflicts,
        &package.replaces,
        &package.provides,
    ];
    let mut out = vec![];
    for (kind, links) in (0u8..).zip(kinds) {
        let start = out.len();
        out.extend(
            links
                .iter()
                .map(|l| (kind, l.target.as_str(), &l.constraint)),
        );
        out[start..].sort_unstable_by(|a, b| a.1.cmp(b.1));
    }
    out
}
