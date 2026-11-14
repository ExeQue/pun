//! Why an update found no solution, in plain sentences: for each problem the solver reports, one line per rule
//! that took part, naming the packages and versions involved and what would fix it when that is clear.

use super::Failure;
use super::package::{Origin, Package};
use super::solver::Reason;
use crate::constraint::Constraint;
use crate::version::is_platform;
use serde_json::{Map, Value};

/// How many matching versions a line names before it shortens the list.
const SHOWN: usize = 4;

/// One list of lines per problem.
pub fn explain(failure: &Failure, manifest: &Map<String, Value>) -> Vec<Vec<String>> {
    let arena = &failure.arena;
    failure
        .problems
        .iter()
        .map(|problem| {
            let mut lines: Vec<String> = vec![];
            for reason in problem.reasons() {
                if let Some(line) = line(arena, manifest, reason)
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

fn line(arena: &[Package], manifest: &Map<String, Value>, reason: &Reason) -> Option<String> {
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
                found(arena, name, constraint)
            ))
        }
        Reason::Requires { source, link } => Some(format!(
            "{} requires {} {}{}",
            at(*source),
            link.target,
            link.pretty,
            found(arena, &link.target, &link.constraint)
        )),
        Reason::Conflict { source, link } => Some(format!(
            "{} conflicts with {} {}",
            at(*source),
            link.target,
            link.pretty
        )),
        Reason::SameName(name) => Some(format!("only one version of {name} can be installed")),
        Reason::Fixed(p) if arena[*p].origin == Origin::Locked => Some(format!(
            "{} is locked; add it to the packages to update (`pun update {}`) to let it change",
            at(*p),
            arena[*p].pretty_name
        )),
        Reason::Fixed(_) | Reason::Learned(_) | Reason::Alias(_) | Reason::InverseAlias(_) => None,
    }
}

/// What the repositories (or the platform) have for `name`, against `constraint`.
fn found(arena: &[Package], name: &str, constraint: &Constraint) -> String {
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
    // The pool loads only the versions a constraint admits, so none found means none that match.
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
            s.push_str(&format!(" and {} more", versions.len() - SHOWN));
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
