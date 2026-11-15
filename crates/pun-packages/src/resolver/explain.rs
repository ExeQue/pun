use super::Failure;
use super::package::{Origin, Package};
use super::solver::Reason;
use crate::constraint::Constraint;
use crate::version::is_platform;
use serde_json::{Map, Value};
use std::fmt::Write as _;

const SHOWN: usize = 4;

pub fn explain(failure: &Failure, manifest: &Map<String, Value>) -> Vec<Vec<String>> {
    let arena = &failure.arena;
    failure
        .problems
        .iter()
        .map(|problem| {
            let mut lines: Vec<String> = vec![];
            for reason in problem.reasons() {
                if let Some(line) = line(arena, &failure.pool, manifest, reason)
                    && !lines.contains(&line)
                {
                    lines.push(line);
                }
            }
            lines
        })
        .filter(|l| !l.is_empty())
        .collect()
}

fn line(
    arena: &[Package],
    pool: &[usize],
    manifest: &Map<String, Value>,
    reason: &Reason,
) -> Option<String> {
    let at = |p: usize| format!("{} {}", arena[p].pretty_name, arena[p].pretty_version);
    match reason {
        Reason::RootRequire { name, constraint } => {
            let pretty = ["require", "require-dev"]
                .iter()
                .filter_map(|k| manifest.get(*k)?.as_object())
                .flat_map(|m| m.iter())
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .and_then(|(_, v)| v.as_str())
                .unwrap_or("*");
            Some(format!(
                "your project requires {name} {pretty}{}",
                found(arena, pool, name, constraint)
            ))
        }
        Reason::Requires { source, link } => Some(format!(
            "{} requires {} {}{}",
            at(*source),
            link.target,
            link.pretty,
            found(arena, pool, &link.target, &link.constraint)
        )),
        Reason::Conflict { source, link } => Some(format!(
            "{} conflicts with {} {}",
            at(*source),
            link.target,
            link.pretty
        )),
        Reason::SameName(name) => Some(format!("only one version of {name} can be installed")),
        Reason::Fixed(_) | Reason::Learned(_) | Reason::Alias(_) | Reason::InverseAlias(_) => None,
    }
}

fn found(arena: &[Package], pool: &[usize], name: &str, constraint: &Constraint) -> String {
    let mut all: Vec<&Package> = (arena.iter())
        .filter(|p| p.name == name && p.alias_of.is_none() && p.origin != Origin::Root)
        .collect();
    all.dedup_by(|a, b| a.version == b.version);
    if is_platform(name) {
        let have = all.iter().find(|p| p.origin == Origin::Platform);
        return match have {
            Some(p) => format!(
                ", but the platform has {} {} (pass --ignore-platform-req={name} to skip the check)",
                p.pretty_name, p.pretty_version
            ),
            None if name.starts_with("ext-") => format!(
                ", but PHP does not have that extension (enable it, or pass --ignore-platform-req={name})"
            ),
            None => format!(", but the platform does not have {name}"),
        };
    }
    if let Some(p) =
        (pool.iter().map(|&p| &arena[p])).find(|p| p.name == name && p.origin == Origin::Locked)
        && !constraint.matches(&Constraint::exact(&p.version))
    {
        return format!(
            ", but {} {} is locked; add it to the packages to update (`pun update {}`) to let it change",
            p.pretty_name, p.pretty_version, p.pretty_name
        );
    }
    if all.is_empty() {
        return ", but no version of it matches; check the name, the constraint and minimum-stability".into();
    }
    let matching: Vec<&str> = (all.iter())
        .filter(|p| constraint.matches(&Constraint::exact(&p.version)))
        .map(|p| p.pretty_version.as_str())
        .collect();
    let list = |versions: &[&str]| {
        let mut s = versions[..versions.len().min(SHOWN)].join(", ");
        if versions.len() > SHOWN {
            let _ = write!(s, " and {} more", versions.len() - SHOWN);
        }
        s
    };
    if matching.is_empty() {
        let versions: Vec<&str> = all.iter().map(|p| p.pretty_version.as_str()).collect();
        match versions.len() {
            1 => format!(", but the only version found is {}", versions[0]),
            _ => format!(", but the only versions found are {}", list(&versions)),
        }
    } else {
        format!(", which {} can satisfy", list(&matching))
    }
}

#[cfg(test)]
mod tests {
    use super::super::pool::{ArrayRepository, Repository};
    use super::super::{Options, resolve};
    use super::*;
    use crate::lock::Lock;
    use serde_json::json;

    fn object(v: Value) -> Map<String, Value> {
        serde_json::from_value(v).unwrap()
    }

    #[expect(
        clippy::needless_pass_by_value,
        reason = "a test helper called with temporaries"
    )]
    fn problems(
        require: Value,
        packages: Value,
        lock: Option<Value>,
        options: &Options,
    ) -> Vec<Vec<String>> {
        let manifest = object(json!({ "require": require }));
        let platform = vec![
            object(json!({"name": "php", "version": "8.5.0"})),
            object(json!({"name": "ext-json", "version": "8.5.0"})),
        ];
        let packages = packages.as_array().unwrap().iter().cloned().map(object);
        let repo: Box<dyn Repository> = Box::new(ArrayRepository::of(packages.collect()));
        let lock = lock.map(|l| Lock::from_composer_lock(&l.to_string()).unwrap());
        let resolved = resolve(
            &manifest,
            platform,
            vec![repo],
            lock.as_ref(),
            options,
            &|_| None,
        )
        .unwrap();
        let mut lines = explain(&resolved.err().expect("no solution"), &manifest);
        for l in &mut lines {
            l.sort();
        }
        lines
    }

    fn package(name: &str, version: &str, links: Value) -> Value {
        let mut p = object(json!({"name": name, "version": version}));
        p.extend(object(links));
        Value::Object(p)
    }

    #[test]
    fn root_requirements_nothing_satisfies() {
        let none = &Options::default();
        let repo = json!([package("acme/a", "1.0.0", json!({}))]);
        assert_eq!(
            problems(json!({"Acme/A": "^2.0"}), repo.clone(), None, none),
            [["your project requires acme/a ^2.0, but the only version found is 1.0.0"]]
        );
        assert_eq!(
            problems(json!({"acme/b": "^1.0"}), repo.clone(), None, none),
            [[
                "your project requires acme/b ^1.0, but no version of it matches; check the name, the constraint and minimum-stability"
            ]]
        );
        assert_eq!(
            problems(
                json!({"ext-foo": "*", "php": "^9.0", "lib-foo": "^1"}),
                repo,
                None,
                none
            ),
            [
                [
                    "your project requires ext-foo *, but PHP does not have that extension (enable it, or pass --ignore-platform-req=ext-foo)"
                ],
                [
                    "your project requires php ^9.0, but the platform has php 8.5.0 (pass --ignore-platform-req=php to skip the check)"
                ],
                ["your project requires lib-foo ^1, but the platform does not have lib-foo"],
            ]
        );
    }

    #[test]
    fn conflicts_and_requirements() {
        let none = &Options::default();
        let repo = json!([
            package("acme/a", "1.0.0", json!({"conflict": {"acme/b": "<2"}})),
            package("acme/b", "1.0.0", json!({})),
        ]);
        assert_eq!(
            problems(
                json!({"acme/a": "^1.0", "acme/b": "^1.0"}),
                repo,
                None,
                none
            ),
            [[
                "acme/a 1.0.0 conflicts with acme/b <2",
                "your project requires acme/a ^1.0, which 1.0.0 can satisfy",
                "your project requires acme/b ^1.0, which 1.0.0 can satisfy",
            ]]
        );
        let mut repo = vec![package(
            "acme/a",
            "1.0.0",
            json!({"require": {"acme/b": "^2.0"}}),
        )];
        for minor in 0..5 {
            repo.push(package("acme/b", &format!("1.{minor}.0"), json!({})));
        }
        assert_eq!(
            problems(
                json!({"acme/a": "*", "acme/b": "^1.0"}),
                Value::Array(repo),
                None,
                none
            ),
            [[
                "acme/a 1.0.0 requires acme/b ^2.0, but the only versions found are 1.0.0, 1.1.0, 1.2.0, 1.3.0 and 1 more",
                "your project requires acme/a *, which 1.0.0 can satisfy",
            ]]
        );
        let repo = json!([
            package("acme/a", "1.0.0", json!({"require": {"acme/c": "1.0.0"}})),
            package("acme/b", "1.0.0", json!({"require": {"acme/c": "2.0.0"}})),
            package("acme/c", "1.0.0", json!({})),
            package("acme/c", "2.0.0", json!({})),
        ]);
        assert_eq!(
            problems(json!({"acme/a": "*", "acme/b": "*"}), repo, None, none),
            [[
                "acme/a 1.0.0 requires acme/c 1.0.0, which 1.0.0 can satisfy",
                "acme/b 1.0.0 requires acme/c 2.0.0, which 2.0.0 can satisfy",
                "only one version of acme/c can be installed",
                "your project requires acme/a *, which 1.0.0 can satisfy",
                "your project requires acme/b *, which 1.0.0 can satisfy",
            ]]
        );
    }

    #[test]
    fn conflicts_with_the_platform() {
        let repo = json!([package(
            "acme/a",
            "1.0.0",
            json!({"conflict": {"php": ">=8"}})
        )]);
        assert_eq!(
            problems(json!({"acme/a": "*"}), repo, None, &Options::default()),
            [[
                "acme/a 1.0.0 conflicts with php >=8",
                "your project requires acme/a *, which 1.0.0 can satisfy",
            ]]
        );
    }

    #[test]
    fn locked_packages() {
        let repo = json!([
            package("acme/a", "1.0.0", json!({})),
            package("acme/a", "2.0.0", json!({})),
            package("acme/b", "1.0.0", json!({"require": {"acme/a": "^2.0"}})),
        ]);
        let lock = json!({"content-hash": "", "packages": [{"name": "acme/a", "version": "1.0.0"}], "packages-dev": []});
        let options = Options {
            allow_list: vec!["acme/b".into()],
            ..Options::default()
        };
        assert_eq!(
            problems(
                json!({"acme/a": "*", "acme/b": "*"}),
                repo,
                Some(lock),
                &options
            ),
            [[
                "acme/b 1.0.0 requires acme/a ^2.0, but acme/a 1.0.0 is locked; add it to the packages to update (`pun update acme/a`) to let it change",
                "your project requires acme/b *, which 1.0.0 can satisfy",
            ]]
        );
    }
}
