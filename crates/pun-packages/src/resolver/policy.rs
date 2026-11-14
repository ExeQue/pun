use super::package::Package;
use super::pool::Pool;
use crate::constraint::{Constraint, Op, match_specific_branches};
use std::cmp::Ordering;
use std::collections::HashMap;

#[derive(Debug, Clone, Default)]
pub struct Policy {
    pub prefer_stable: bool,
    pub prefer_lowest: bool,
    pub prefer_dev_over_prerelease: bool,
    pub preferred_versions: Option<HashMap<String, String>>,
}

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

#[cfg(test)]
mod tests {
    use super::super::package::{Origin, alias_package, load};
    use super::*;
    use serde_json::json;

    #[test]
    fn selects_as_composer() {
        let mut arena = vec![];
        for data in [
            json!({"name": "a/a", "version": "dev-master"}),
            json!({"name": "a/a", "version": "1.0.0-alpha1"}),
            json!({"name": "a/a", "version": "1.0.0-RC1"}),
            json!({"name": "b/b", "version": "1.0"}),
            json!({"name": "c/c", "version": "2.0", "replace": {"b/b": "==1.0"}}),
            json!({"name": "vendor-b/replacer", "version": "1.0", "replace": {"vendor-a/package": "==1.0"}}),
            json!({"name": "vendor-a/replacer", "version": "1.0", "replace": {"vendor-a/package": "==1.0"}}),
            json!({"name": "d/d", "version": "dev-main"}),
        ] {
            load(
                data.as_object().unwrap().clone(),
                Origin::Repository(1),
                &mut arena,
            )
            .unwrap();
        }
        let alias = |arena: &mut Vec<Package>, of, version: &str, pretty: &str, root| {
            alias_package(arena, of, version.into(), pretty.into(), root);
        };
        alias(&mut arena, 7, "2.1.9999999.9999999-dev", "2.1.x-dev", false);
        let feature = json!({"name": "d/d", "version": "dev-feature"});
        load(
            feature.as_object().unwrap().clone(),
            Origin::Repository(1),
            &mut arena,
        )
        .unwrap();
        alias(&mut arena, 9, "2.1.9999999.9999999-dev", "2.1.x-dev", true);
        let main = json!({"name": "e/e", "version": "dev-main"});
        load(
            main.as_object().unwrap().clone(),
            Origin::Repository(1),
            &mut arena,
        )
        .unwrap();
        alias(
            &mut arena,
            11,
            "1.0.9999999.9999999-dev",
            "1.0.x-dev",
            false,
        );
        for data in [
            json!({"name": "f/replacer", "version": "1.0", "replace": {"g/pkg": "==1.0"}}),
            json!({"name": "g/pkg", "version": "1.0"}),
            json!({"name": "vendor-a/second", "version": "1.0", "replace": {"vendor-a/package": "==1.0"}}),
            json!({"name": "vendor-c/other", "version": "1.0", "replace": {"vendor-a/package": "==1.0"}}),
        ] {
            load(
                data.as_object().unwrap().clone(),
                Origin::Repository(1),
                &mut arena,
            )
            .unwrap();
        }
        let all = (0..arena.len()).collect();
        let pool = Pool::new(arena, all, vec![]);
        for (stable, lowest, dev_over_prerelease, literals, required, want) in [
            (true, true, true, vec![1, 2], None, vec![1]),
            (true, true, true, vec![1, 3], None, vec![1]),
            (true, true, false, vec![2, 1], None, vec![2]),
            (false, false, false, vec![4, 5], None, vec![4, 5]),
            (false, false, false, vec![5, 4], None, vec![4, 5]),
            (
                false,
                false,
                false,
                vec![6, 7],
                Some("vendor-a/package"),
                vec![7, 6],
            ),
            (
                false,
                false,
                false,
                vec![7, 6],
                Some("vendor-a/package"),
                vec![7, 6],
            ),
            (false, false, false, vec![9, 11], None, vec![11]),
            (false, false, false, vec![12, 13], None, vec![13]),
            (false, false, false, vec![14, 15], None, vec![15, 14]),
            (
                false,
                false,
                false,
                vec![16, 17],
                Some("vendor-a/package"),
                vec![16, 17],
            ),
            (
                false,
                false,
                false,
                vec![7, 16],
                Some("vendor-a/package"),
                vec![7, 16],
            ),
            (
                false,
                false,
                false,
                vec![16, 17],
                Some("nonvendor"),
                vec![16, 17],
            ),
        ] {
            let policy = Policy {
                prefer_stable: stable,
                prefer_lowest: lowest,
                prefer_dev_over_prerelease: dev_over_prerelease,
                preferred_versions: None,
            };
            assert_eq!(
                policy.select_preferred(&pool, literals.clone(), required),
                want,
                "{literals:?}"
            );
        }
    }
}
