pub mod composer_repo;
pub mod explain;
pub mod filters;
pub mod locker;
pub mod optimizer;
pub mod package;
pub mod platform;
pub mod policy;
pub mod pool;
pub mod repos;
pub mod solver;

use crate::Env;
use crate::constraint::{self, AND, Constraint, OR, split};
use crate::error::{Result, bail};
use crate::lock::Lock;
use crate::version::{ALIAS, IgnorePlatform, group, is_platform, normalize, stability_of};
use indexmap::IndexMap;
use package::{Origin, Package, load, stability_value};
use pcre2::bytes::Regex;
use policy::Policy;
use pool::{
    ArrayRepository, PoolBuilder, Repository, Request, RootAliases, Stabilities, Transitive,
};
use serde_json::{Map, Value};
use solver::{Problem, Solver};
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub allow_list: Vec<String>,
    pub transitive: Transitive,
    pub prefer_stable: bool,
    pub prefer_lowest: bool,
    pub minimal_changes: bool,
    pub ignore_platform: IgnorePlatform,
}

pub struct Resolution {
    pub arena: Vec<Package>,
    pub packages: Vec<usize>,
    pub packages_dev: Vec<usize>,
    pub aliases: Vec<Map<String, Value>>,
    pub flags: IndexMap<String, u8>,
}

pub struct Failure {
    pub arena: Vec<Package>,
    pub pool: Vec<usize>,
    pub problems: Vec<Problem>,
}

static ROOT_ALIAS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|\| *|, *)([^,\s#|]+)(?:#[^ ]+)? +as +([^,\s|]+)(?:$| *\|| *,)")
        .expect("valid pattern")
});
static FLAGGED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^[^@]*?@(stable|RC|beta|alpha|dev)$").expect("valid pattern")
});
static BARE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[^,\s@]+$").expect("valid pattern"));
static REFERENCE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^,\s@]+?#([a-f0-9]+)$").expect("valid pattern"));

fn root_aliases(links: &[(String, String)]) -> Result<Vec<Map<String, Value>>> {
    let mut out = vec![];
    for (name, constraint) in links {
        if let Some(m) = ROOT_ALIAS.captures(constraint.as_bytes())? {
            let mut alias = Map::new();
            alias.insert("package".into(), name.clone().into());
            alias.insert("version".into(), normalize(group(&m, 1))?.into());
            alias.insert("alias".into(), group(&m, 2).into());
            alias.insert("alias_normalized".into(), normalize(group(&m, 2))?.into());
            out.push(alias);
        } else if constraint.contains(" as ") {
            bail!(
                "Invalid alias definition in \"{name}\": \"{constraint}\". Aliases should be in the form \"exact-version as other-exact-version\"."
            );
        }
    }
    Ok(out)
}

fn stability_flags(
    links: &[(String, String)],
    minimum: u8,
    flags: &mut IndexMap<String, u8>,
) -> Result<()> {
    for (name, constraint) in links {
        let parts: Vec<&str> = split(&OR, constraint.trim())
            .into_iter()
            .flat_map(|o| split(&AND, o))
            .collect();
        let mut matched = false;
        for part in &parts {
            if let Some(m) = FLAGGED.captures(part.as_bytes())? {
                let stability = stability_value(group(&m, 1));
                if flags.get(name).is_some_and(|f| *f > stability) {
                    continue;
                }
                flags.insert(name.clone(), stability);
                matched = true;
            }
        }
        if matched {
            continue;
        }
        for part in parts {
            let version = match ALIAS.captures(part.as_bytes())? {
                Some(m) => group(&m, 1).to_string(),
                None => part.to_string(),
            };
            let stability = stability_of(&version);
            if BARE.is_match(version.as_bytes())? && stability != "stable" {
                let stability = stability_value(stability);
                if flags.get(name).is_some_and(|f| *f > stability) || minimum > stability {
                    continue;
                }
                flags.insert(name.clone(), stability);
            }
        }
    }
    Ok(())
}

fn references(links: &[(String, String)]) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    for (name, constraint) in links {
        let version = match ALIAS.captures(constraint.as_bytes())? {
            Some(m) => group(&m, 1).to_string(),
            None => constraint.clone(),
        };
        if let Some(m) = REFERENCE.captures(version.as_bytes())?
            && stability_of(&version) == "dev"
        {
            out.insert(name.clone(), group(&m, 1).to_string());
        }
    }
    Ok(out)
}

fn links(manifest: &Map<String, Value>, kind: &str) -> Vec<(String, String)> {
    (manifest
        .get(kind)
        .and_then(Value::as_object)
        .into_iter()
        .flatten())
    .filter_map(|(k, v)| Some((k.to_ascii_lowercase(), v.as_str()?.to_string())))
    .collect()
}

fn ignored_by(options: &Options) -> impl Fn(&str) -> bool + '_ {
    move |name: &str| is_platform(name) && options.ignore_platform.ignores(name)
}

pub fn resolve(
    manifest: &Map<String, Value>,
    platform: Vec<Map<String, Value>>,
    repositories: Vec<Box<dyn Repository + '_>>,
    locked: Option<&Lock>,
    options: &Options,
    env: Env,
) -> Result<std::result::Result<Resolution, Failure>> {
    let mut arena: Vec<Package> = vec![];
    let mut root_data = manifest.clone();
    root_data.entry("name").or_insert_with(|| "__root__".into());
    root_data.entry("version").or_insert_with(|| "1.0.0".into());
    let root_name = root_data["name"]
        .as_str()
        .unwrap_or("__root__")
        .to_ascii_lowercase();
    let requires = links(manifest, "require");
    let dev_requires = links(manifest, "require-dev");
    let minimum = stability_value(
        manifest
            .get("minimum-stability")
            .and_then(Value::as_str)
            .unwrap_or("stable"),
    );
    let mut flags = IndexMap::new();
    let mut aliases = vec![];
    let mut refs = HashMap::new();
    for l in [&requires, &dev_requires] {
        if l.is_empty() {
            continue;
        }
        aliases.extend(root_aliases(l)?);
        stability_flags(l, minimum, &mut flags)?;
        refs.extend(references(l)?);
    }
    let mut fixed_root = root_data.clone();
    fixed_root.remove("require");
    fixed_root.remove("require-dev");
    let root_packages = load(fixed_root, Origin::Root, &mut arena)?;
    let root = *root_packages.last().expect("the root package");
    let root_flags = flags.clone();
    flags.insert(root_name, arena[root].stability);
    let stabilities = Stabilities {
        minimum,
        flags: flags.iter().map(|(k, v)| (k.clone(), *v)).collect(),
    };
    let mut root_alias_map: RootAliases = HashMap::new();
    for a in &aliases {
        let s = |k: &str| a[k].as_str().unwrap_or_default().to_string();
        (root_alias_map.entry(s("package")).or_default())
            .insert(s("version"), (s("alias"), s("alias_normalized")));
    }

    let mut request = Request::default();
    request.fixed.extend(&root_packages);
    let root_provides = arena[root].provides.clone();
    for data in platform {
        for p in load(data, Origin::Platform, &mut arena)? {
            let package = &arena[p];
            let provided = (root_provides.iter())
                .find(|l| l.target == package.name)
                .is_some_and(|l| l.constraint.matches(&Constraint::exact(&package.version)));
            if !provided {
                request.fixed.push(p);
            }
        }
    }
    if !options.allow_list.is_empty() && locked.is_none() {
        bail!(
            "Cannot update only a partial set of packages without a lock file present. Run `pun update` to generate a lock file."
        );
    }
    let mut locked_names = vec![];
    if let Some(locked) = locked {
        let mut lock = vec![];
        let mut by_name: HashMap<String, usize> = HashMap::new();
        for data in locked.all() {
            for p in load(data.0.clone(), Origin::Locked, &mut arena)? {
                by_name.insert(arena[p].name.clone(), arena[p].alias_of.unwrap_or(p));
                lock.push(p);
            }
        }
        let lock_aliases = locked
            .settings
            .get("aliases")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for a in lock_aliases {
            let s = |k: &str| {
                a.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            if let Some(&of) = by_name.get(&s("package")) {
                lock.push(package::alias_package(
                    &mut arena,
                    of,
                    s("alias_normalized"),
                    s("alias"),
                    true,
                ));
            }
        }
        locked_names = lock
            .iter()
            .map(|&p| {
                (
                    arena[p].name.clone(),
                    arena[p].version.clone(),
                    arena[p].alias_of.is_some(),
                )
            })
            .collect();
        request.lock = Some(lock);
    }
    for (name, c) in requires.iter().chain(&dev_requires) {
        request.requires.insert(name.clone(), constraint::parse(c)?);
    }
    request.allow_list = options
        .allow_list
        .iter()
        .map(|n| n.to_ascii_lowercase())
        .collect();
    request.transitive = options.transitive;
    let mut policy = Policy {
        prefer_stable: options.prefer_stable
            || manifest.get("prefer-stable") == Some(&Value::Bool(true)),
        prefer_lowest: options.prefer_lowest,
        prefer_dev_over_prerelease: env("COMPOSER_PREFER_DEV_OVER_PRERELEASE")
            .is_some_and(|v| !v.is_empty() && v != "0"),
        preferred_versions: None,
    };
    if options.minimal_changes && request.lock.is_some() {
        policy.preferred_versions = Some(
            (locked_names.iter())
                .filter(|(name, _, alias)| !alias && !request.in_allow_list(name))
                .map(|(n, v, _)| (n.clone(), v.clone()))
                .collect(),
        );
    }
    let ignored = ignored_by(options);
    for (name, _) in requires.iter().chain(&dev_requires) {
        if ignored(name) {
            request.requires.shift_remove(name);
        }
    }

    let mut repos: Vec<Box<dyn Repository + '_>> = vec![Box::new(ArrayRepository {
        packages: root_packages,
        ..ArrayRepository::default()
    })];
    repos.extend(repositories);
    let ignored_types = ["php-ext", "php-ext-zend"];
    let config = manifest.get("config").and_then(Value::as_object);
    let filters = filters::Filters::from_config(config, env)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let builder = PoolBuilder::new(
        &stabilities,
        &root_alias_map,
        &refs,
        &ignored_types,
        &filters,
        now,
    );
    let mut pool = builder.build(arena, &mut repos, &mut request)?;
    let all = pool.ids.clone();
    let kept = optimizer::optimize(&pool, &request, &policy);
    let optimized = kept.len() < all.len();
    if optimized {
        pool = pool.with_packages(kept);
    }
    let mut result = Solver::new(&pool, &policy).solve(&request, &ignored);
    if result.is_err() && optimized {
        pool = pool.with_packages(all);
        result = Solver::new(&pool, &policy).solve(&request, &ignored);
    }
    let solution = match result {
        Ok(s) => s,
        Err(problems) => {
            return Ok(Err(Failure {
                arena: pool.arena,
                pool: pool.ids,
                problems,
            }));
        }
    };
    let fixed: HashSet<usize> = request.fixed.iter().copied().collect();
    let chosen: Vec<usize> = solution
        .packages
        .iter()
        .copied()
        .filter(|p| !fixed.contains(p))
        .collect();
    let used_aliases = {
        let mut used: Vec<Map<String, Value>> = vec![];
        let mut rest = aliases.clone();
        for &p in &solution.packages {
            if pool.arena[p].alias_of.is_some() {
                rest.retain(|a| {
                    if a["package"].as_str() == Some(pool.arena[p].name.as_str()) {
                        used.push(a.clone());
                        false
                    } else {
                        true
                    }
                });
            }
        }
        used.sort_by(|a, b| a["package"].as_str().cmp(&b["package"].as_str()));
        used
    };
    let mut arena = pool.arena;
    let real: Vec<usize> = chosen
        .iter()
        .copied()
        .filter(|&p| arena[p].alias_of.is_none())
        .collect();
    let (packages, packages_dev) = if dev_requires.is_empty() {
        (real, vec![])
    } else {
        let non_dev = non_dev_names(
            &mut arena,
            &request.fixed,
            &chosen,
            &requires,
            &root_alias_map,
            &policy,
            &ignored,
        )?;
        let Some(non_dev) = non_dev else {
            bail!(
                "Unable to find a compatible set of packages based on your non-dev requirements alone."
            );
        };
        real.into_iter()
            .partition(|p| non_dev.contains(&arena[*p].name))
    };
    let sort = |list: Vec<usize>| {
        let mut list = list;
        list.sort_by(|&a, &b| {
            (arena[a].pretty_name.cmp(&arena[b].pretty_name))
                .then(arena[a].pretty_version.cmp(&arena[b].pretty_version))
        });
        list
    };
    let (packages, packages_dev) = (sort(packages), sort(packages_dev));
    Ok(Ok(Resolution {
        arena,
        packages,
        packages_dev,
        aliases: used_aliases,
        flags: root_flags,
    }))
}

fn non_dev_names(
    arena: &mut Vec<Package>,
    fixed: &[usize],
    chosen: &[usize],
    requires: &[(String, String)],
    root_aliases: &RootAliases,
    policy: &Policy,
    ignored: &dyn Fn(&str) -> bool,
) -> Result<Option<HashSet<String>>> {
    let mut all: Vec<usize> = fixed.to_vec();
    for &p in chosen {
        if arena[p].alias_of.is_some() {
            continue;
        }
        let data = arena[p].data();
        let origin = arena[p].origin;
        for q in load(data, origin, arena)? {
            all.push(q);
            let (name, version) = (arena[q].name.clone(), arena[q].version.clone());
            if let Some((pretty, normalized)) =
                root_aliases.get(&name).and_then(|v| v.get(&version))
            {
                let base = arena[q].alias_of.unwrap_or(q);
                all.push(package::alias_package(
                    arena,
                    base,
                    normalized.clone(),
                    pretty.clone(),
                    true,
                ));
            }
        }
    }
    let pool = pool::Pool::new(std::mem::take(arena), all, vec![]);
    let mut request = Request {
        fixed: fixed.to_vec(),
        ..Request::default()
    };
    for (name, c) in requires {
        if !ignored(name) {
            request.requires.insert(name.clone(), constraint::parse(c)?);
        }
    }
    let mut solver = Solver::new(&pool, policy);
    let result = solver.solve(&request, ignored);
    let names = result.ok().map(|s| {
        (s.packages.iter())
            .filter(|p| !fixed.contains(p))
            .map(|&p| pool.arena[p].name.clone())
            .collect()
    });
    *arena = pool.arena;
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::{Auth, Fetcher};
    use serde_json::json;

    fn update(manifest: Value, lock: Option<Value>, options: &Options) -> Result<Value, String> {
        update_in(manifest, lock, options, &|_| None)
    }

    fn update_in(
        manifest: Value,
        lock: Option<Value>,
        options: &Options,
        env: Env,
    ) -> Result<Value, String> {
        let manifest = manifest.as_object().unwrap().clone();
        let fetcher = Fetcher::new(None, Auth::default(), false);
        let root = std::path::Path::new(".");
        let repositories = repos::open(root, &manifest, None, &fetcher, env).unwrap();
        let platform = ["php", "ext-json"].map(|n| {
            json!({"name": n, "version": "8.5.12"})
                .as_object()
                .unwrap()
                .clone()
        });
        let lock = lock.map(|l| Lock::from_composer_lock(&l.to_string()).unwrap());
        let resolved = resolve(
            &manifest,
            platform.to_vec(),
            repositories,
            lock.as_ref(),
            options,
            env,
        );
        let r = resolved
            .map_err(|e| e.to_string())?
            .map_err(|_| "no solution".to_string())?;
        let list = |ids: &[usize]| -> Vec<Value> {
            (ids.iter())
                .map(|&p| json!([r.arena[p].pretty_name, r.arena[p].pretty_version]))
                .collect()
        };
        let flags: serde_json::Map<String, Value> = (r.flags.iter())
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect();
        Ok(json!([list(&r.packages), list(&r.packages_dev), flags]))
    }

    fn repositories(packages: Value) -> Value {
        json!([{"type": "package", "package": packages}, {"packagist.org": false}])
    }

    fn versions() -> Value {
        repositories(json!([
            {"name": "a/a", "version": "1.0.0"},
            {"name": "a/a", "version": "1.0.0-beta1"},
            {"name": "a/a", "version": "dev-main"},
            {"name": "b/b", "version": "1.0.0", "require": {"ext-json": "^8"}}
        ]))
    }

    #[test]
    fn reads_the_root_as_composer() {
        let none = &Options::default();
        let m = json!({"require": {"a/a": "1.0 as 2.0 3.0"}, "repositories": versions()});
        assert_eq!(
            update(m, None, none).unwrap_err(),
            "Invalid alias definition in \"a/a\": \"1.0 as 2.0 3.0\". Aliases should be in the form \"exact-version as other-exact-version\"."
        );
        let m = json!({"minimum-stability": "beta", "require": {"a/a": "1.0.0-beta1@dev || 1.0.0-beta1@beta"}, "repositories": versions()});
        assert_eq!(
            update(m, None, none).unwrap(),
            json!([[["a/a", "1.0.0-beta1"]], [], {"a/a": 20}])
        );
        let m = json!({"minimum-stability": "dev", "require": {"a/a": "1.0.0-beta1"}, "repositories": versions()});
        assert_eq!(
            update(m, None, none).unwrap(),
            json!([[["a/a", "1.0.0-beta1"]], [], {}])
        );
        let m = json!({"provide": {"ext-json": "8.5.12"}, "require": {"b/b": "*"}, "repositories": versions()});
        assert_eq!(
            update(m, None, none).unwrap(),
            json!([[["b/b", "1.0.0"]], [], {}])
        );
        let m = json!({"require": {"ext-absent": "*", "a/a": "^1.0"}, "require-dev": {"b/b": "*"}, "repositories": versions()});
        let ignore = Options {
            ignore_platform: IgnorePlatform::Listed(vec!["ext-absent".into()]),
            ..Options::default()
        };
        assert_eq!(
            update(m, None, &ignore).unwrap(),
            json!([[["a/a", "1.0.0"]], [["b/b", "1.0.0"]], {}])
        );
    }

    #[test]
    fn needs_the_non_dev_requirements_alone() {
        let m = json!({
            "minimum-stability": "dev",
            "require": {"a/a": "2.0.x-dev"},
            "require-dev": {"b/b": "*"},
            "repositories": repositories(json!([
                {"name": "a/a", "version": "dev-main", "source": {"type": "git", "url": "https://x/a", "reference": "1"}},
                {"name": "b/b", "version": "1.0.0"}
            ]))
        });
        let lock = json!({
            "content-hash": "x",
            "packages": [{"name": "a/a", "version": "dev-main", "source": {"type": "git", "url": "https://x/a", "reference": "1"}, "type": "library"}],
            "packages-dev": [{"name": "b/b", "version": "1.0.0", "type": "library"}],
            "aliases": [
                {"package": "a/a", "version": "dev-main", "alias": "2.0.x-dev", "alias_normalized": "2.0.9999999.9999999-dev"},
                {"package": "c/c", "version": "dev-main", "alias": "1.0.x-dev", "alias_normalized": "1.0.9999999.9999999-dev"}
            ],
            "minimum-stability": "dev"
        });
        let options = Options {
            allow_list: vec!["b/b".into()],
            ..Options::default()
        };
        assert_eq!(
            update(m, Some(lock), &options).unwrap_err(),
            "Unable to find a compatible set of packages based on your non-dev requirements alone."
        );
    }

    #[test]
    fn prefers_dev_over_prerelease_as_composer() {
        let m = json!({"require": {"a/a": "*@dev"}, "repositories": repositories(json!([
            {"name": "a/a", "version": "dev-master"},
            {"name": "a/a", "version": "1.0.0-alpha1"}
        ]))});
        let options = Options {
            prefer_stable: true,
            prefer_lowest: true,
            ..Options::default()
        };
        for (value, want) in [("1", "dev-master"), ("0", "1.0.0-alpha1")] {
            let env =
                |k: &str| (k == "COMPOSER_PREFER_DEV_OVER_PRERELEASE").then(|| value.to_string());
            let got = update_in(m.clone(), None, &options, &env);
            assert_eq!(got.unwrap()[0], json!([["a/a", want]]), "{value}");
        }
    }

    #[test]
    fn blocks_abandoned_packages_as_composer() {
        let none = &Options::default();
        let policy = |advisories: bool| json!({"policy": {"abandoned": {"block": true}, "advisories": {"block": advisories}}});
        let packages = repositories(json!([
            {"name": "a/a", "version": "1.0.0", "abandoned": true},
            {"name": "b/b", "version": "1.0.0"},
            {"name": "b/b", "version": "1.2.0"}
        ]));
        let m = json!({"require": {"a/a": "*"}, "config": policy(false), "repositories": packages});
        assert_eq!(update(m, None, none).unwrap()[0], json!([["a/a", "1.0.0"]]));
        let m = json!({"require": {"a/a": "*", "b/b": "*"}, "config": policy(true), "repositories": packages});
        let lock = json!({"content-hash": "x", "packages-dev": [], "packages": [
            {"name": "a/a", "version": "1.0.0", "abandoned": true, "type": "library"},
            {"name": "b/b", "version": "1.0.0", "type": "library"}
        ]});
        let options = Options {
            allow_list: vec!["b/b".into()],
            ..Options::default()
        };
        assert_eq!(update(m, Some(lock), &options).unwrap_err(), "no solution");
        let m = json!({"minimum-stability": "dev", "require": {"a/a": "1.0.x-dev"}, "config": policy(true),
            "repositories": repositories(json!([{"name": "a/a", "version": "dev-main", "abandoned": true,
                "extra": {"branch-alias": {"dev-main": "1.0.x-dev"}}}]))});
        let manifest = m.as_object().unwrap().clone();
        let fetcher = Fetcher::new(None, Auth::default(), false);
        let repositories = repos::open(
            std::path::Path::new("."),
            &manifest,
            None,
            &fetcher,
            &|_| None,
        )
        .unwrap();
        let failure = (resolve(&manifest, vec![], repositories, None, none, &|_| None).unwrap())
            .err()
            .expect("no solution");
        assert!(failure.pool.iter().all(|&p| failure.arena[p].name != "a/a"));
    }

    #[test]
    fn needs_the_package_an_alias_installs() {
        let m = json!({"minimum-stability": "dev", "require": {"a/a": "1.0.x-dev"}, "repositories": [
            {"type": "package", "package": [{"name": "a/a", "version": "dev-main", "extra": {"branch-alias": {"dev-main": "1.0.x-dev"}}}],
             "filter": {"malware": [{"package": "a/a", "constraint": "dev-main"}]}},
            {"packagist.org": false}
        ]});
        assert_eq!(
            update(m, None, &Options::default()).unwrap_err(),
            "no solution"
        );
    }

    #[test]
    fn sets_root_references_as_composer() {
        let m = json!({"minimum-stability": "dev", "require": {"a/a": "dev-main#abc", "b/b": "dev-main#def"},
        "repositories": repositories(json!([
            {"name": "a/a", "version": "dev-main",
             "source": {"type": "git", "url": "https://example.org/a.git", "reference": "1"},
             "dist": {"type": "zip", "url": "https://example.org/a.zip", "reference": "old"}},
            {"name": "b/b", "version": "dev-main", "dist": {"type": "zip", "url": "https://example.org/b.zip", "reference": ""}}
        ]))});
        let manifest = m.as_object().unwrap().clone();
        let fetcher = Fetcher::new(None, Auth::default(), false);
        let repositories = repos::open(
            std::path::Path::new("."),
            &manifest,
            None,
            &fetcher,
            &|_| None,
        )
        .unwrap();
        let r = (resolve(
            &manifest,
            vec![],
            repositories,
            None,
            &Options::default(),
            &|_| None,
        )
        .unwrap())
        .map_err(|_| "no solution")
        .unwrap();
        let got: Vec<Value> = (r.packages.iter())
            .map(|&p| {
                let d = r.arena[p].data();
                json!([d.get("source"), d["dist"]])
            })
            .collect();
        assert_eq!(
            got,
            [
                json!([{"type": "git", "url": "https://example.org/a.git", "reference": "abc"},
                       {"type": "zip", "url": "https://example.org/a.zip", "reference": "abc"}]),
                json!([null, {"type": "zip", "url": "https://example.org/b.zip", "reference": ""}]),
            ]
        );
    }

    #[test]
    fn rejects_bad_repository_filters() {
        for (filter, want) in [
            (
                json!({"only": "a/a"}),
                "\"only\" key for repository should be an array",
            ),
            (
                json!({"exclude": {}}),
                "\"exclude\" key for repository should be an array",
            ),
            (
                json!({"only": [], "exclude": []}),
                "Only one of \"only\" and \"exclude\" can be specified for a repository",
            ),
            (
                json!({"canonical": "no"}),
                "\"canonical\" key for repository should be a boolean",
            ),
        ] {
            let mut repo =
                json!({"type": "package", "package": [{"name": "a/a", "version": "1.0.0"}]});
            repo.as_object_mut()
                .unwrap()
                .extend(filter.as_object().unwrap().clone());
            let m =
                json!({"require": {"a/a": "*"}, "repositories": [repo, {"packagist.org": false}]});
            let manifest = m.as_object().unwrap().clone();
            let fetcher = Fetcher::new(None, Auth::default(), false);
            let got = repos::open(
                std::path::Path::new("."),
                &manifest,
                None,
                &fetcher,
                &|_| None,
            );
            assert_eq!(got.err().map(|e| e.to_string()).as_deref(), Some(want));
        }
    }

    #[test]
    fn filters_repositories_as_composer() {
        let m = json!({"require": {"a/a": "*"}, "repositories": [
            {"type": "package", "package": [{"name": "a/a", "version": "1.0.0"}], "only": ["x/*"]},
            {"type": "package", "package": [{"name": "a/a", "version": "2.0.0"}]},
            {"packagist.org": false}
        ]});
        assert_eq!(
            update(m, None, &Options::default()).unwrap()[0],
            json!([["a/a", "2.0.0"]])
        );
        let m = json!({"require": {"a/a": "*"}, "repositories": repositories(json!([
            {"name": "a/a", "version": "1.0.0", "type": "php-ext"}
        ]))});
        assert_eq!(
            update(m, None, &Options::default()).unwrap_err(),
            "no solution"
        );
    }
}
