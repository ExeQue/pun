//! Which of several candidate packages the solver tries first (Composer's `DefaultPolicy`): the newest version (the
//! oldest with `--prefer-lowest`), stable before unstable with `prefer-stable`, the locked version with
//! `--minimal-changes`, root aliases before remote ones, and a package before the ones that replace it.

use super::package::Package;
use super::pool::Pool;
use crate::constraint::{Constraint, Op, match_specific_branches};
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Policy {
    pub prefer_stable: bool,
    pub prefer_lowest: bool,
    /// `COMPOSER_PREFER_DEV_OVER_PRERELEASE`: with `prefer_lowest`, a dev version counts as stable against a
    /// pre-release.
    pub prefer_dev_over_prerelease: bool,
    /// With `--minimal-changes`, the locked version of each package that is not being updated.
    pub preferred_versions: Option<HashMap<String, String>>,
}

/// A stable insertion sort: PHP's `usort` result for any consistent comparator, without requiring the total order
/// Rust's sorts check for.
fn usort<T: Copy>(items: &mut [T], mut cmp: impl FnMut(T, T) -> Ordering) {
    for i in 1..items.len() {
        let mut j = i;
        while j > 0 && cmp(items[j - 1], items[j]) == Ordering::Greater {
            items.swap(j - 1, j);
            j -= 1;
        }
    }
}

impl Policy {
    /// Whether `a` compares to `b` by `op` (`>`, `<` or `==`), stability first when `prefer_stable`.
    pub fn version_compare(&self, a: &Package, b: &Package, op: Op) -> bool {
        if self.prefer_stable && a.stability != b.stability {
            let (mut sa, mut sb) = (a.stability, b.stability);
            if self.prefer_lowest && self.prefer_dev_over_prerelease && sa != 0 && sb != 0 {
                sa = if sa == 20 { 0 } else { sa };
                sb = if sb == 20 { 0 } else { sb };
            }
            return sa < sb;
        }
        if (a.is_dev() && a.version.starts_with("dev-"))
            || (b.is_dev() && b.version.starts_with("dev-"))
        {
            return match_specific_branches(op, &b.version, Op::Eq, &a.version);
        }
        Constraint::Single(op, b.version.clone()).matches(&Constraint::exact(&a.version))
    }

    /// The literals of `literals` worth trying, best first (`selectPreferredPackages`).
    pub fn select_preferred(
        &self,
        pool: &Pool,
        mut literals: Vec<i32>,
        required: Option<&str>,
    ) -> Vec<i32> {
        literals.sort_unstable();
        let mut groups: indexmap::IndexMap<&str, Vec<i32>> = indexmap::IndexMap::new();
        for &l in &literals {
            groups
                .entry(pool.package(l).name.as_str())
                .or_default()
                .push(l);
        }
        let mut selected = vec![];
        for (_, mut group) in groups {
            usort(&mut group, |a, b| {
                self.compare_by_priority(pool, a, b, required, true)
            });
            let best = self.prune_to_best_version(pool, &group);
            selected.extend(prune_remote_aliases(pool, best));
        }
        usort(&mut selected, |a, b| {
            self.compare_by_priority(pool, a, b, required, false)
        });
        selected
    }

    fn compare_by_priority(
        &self,
        pool: &Pool,
        a: i32,
        b: i32,
        required: Option<&str>,
        ignore_replace: bool,
    ) -> Ordering {
        let (pa, pb) = (pool.package(a), pool.package(b));
        if pa.name == pb.name {
            match (pa.alias_of.is_some(), pb.alias_of.is_some()) {
                (true, false) => return Ordering::Less,
                (false, true) => return Ordering::Greater,
                _ => {}
            }
        }
        if !ignore_replace {
            let replaces = |s: &Package, t: &Package| s.replaces.iter().any(|l| l.target == t.name);
            if replaces(pa, pb) {
                return Ordering::Greater;
            }
            if replaces(pb, pa) {
                return Ordering::Less;
            }
            if let Some((vendor, _)) = required.and_then(|r| r.split_once('/')) {
                let (sa, sb) = (pa.name.starts_with(vendor), pb.name.starts_with(vendor));
                if sa != sb {
                    return if sa {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    };
                }
            }
        }
        a.cmp(&b)
    }

    fn prune_to_best_version(&self, pool: &Pool, literals: &[i32]) -> Vec<i32> {
        if let Some(preferred) = &self.preferred_versions
            && let Some(version) = preferred.get(&pool.package(literals[0]).name)
        {
            let best: Vec<i32> = (literals.iter().copied())
                .filter(|&l| pool.package(l).version == *version)
                .collect();
            if !best.is_empty() {
                return best;
            }
        }
        let op = if self.prefer_lowest { Op::Lt } else { Op::Gt };
        let mut best = vec![literals[0]];
        let mut best_package = pool.package(literals[0]);
        for &l in &literals[1..] {
            let p = pool.package(l);
            if self.version_compare(p, best_package, op) {
                best_package = p;
                best = vec![l];
            } else if self.version_compare(p, best_package, Op::Eq) {
                best.push(l);
            }
        }
        best
    }
}

/// Only the root aliases, when there are any among the candidates.
fn prune_remote_aliases(pool: &Pool, literals: Vec<i32>) -> Vec<i32> {
    let root = |l: &i32| {
        let p = pool.package(*l);
        p.alias_of.is_some() && p.root_alias
    };
    if literals.iter().any(root) {
        literals.into_iter().filter(root).collect()
    } else {
        literals
    }
}
