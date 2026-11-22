use crate::finder::Exclusions;
use anyhow::{Result, bail};
use pun_lint::problem::Level;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::LazyLock;

pub(crate) const UNUSED_IGNORE: &str = "lint/comments/no-unused-ignore";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CliLevel {
    Allow,
    Warn,
    Deny,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Engine {
    Native,
    Token(&'static str),
    Builtin,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct On {
    pub(crate) level: Level,
    pub(crate) fix: bool,
    pub(crate) options: Value,
}

pub(crate) fn token_rules() -> &'static HashMap<&'static str, &'static str> {
    static T: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
        let ported: BTreeSet<&'static str> = crate::extensions::fixers()
            .iter()
            .map(|f| f.name())
            .collect();
        (pun_config::fixer_entries().iter())
            .filter_map(|(fixer, e)| {
                let id = e.get("pun")?.as_str()?;
                let fixer = ported.get(fixer.as_str())?;
                id.starts_with("lint/").then_some((id, *fixer))
            })
            .collect()
    });
    &T
}

pub(crate) fn native_rules() -> &'static BTreeSet<&'static str> {
    static T: LazyLock<BTreeSet<&'static str>> = LazyLock::new(|| {
        crate::extensions::lint_rules()
            .iter()
            .map(|r| r.id())
            .collect()
    });
    &T
}

pub(crate) fn engine(id: &str) -> Option<Engine> {
    if native_rules().contains(id) {
        return Some(Engine::Native);
    }
    if id == UNUSED_IGNORE {
        return Some(Engine::Builtin);
    }
    token_rules().get(id).map(|f| Engine::Token(f))
}

fn level(l: pun_config::Level) -> Level {
    match l {
        pun_config::Level::Warn => Level::Warning,
        pun_config::Level::Error => Level::Error,
    }
}

pub(crate) struct Plan {
    base: BTreeMap<String, On>,
    overrides: Vec<(Exclusions, Vec<pun_config::LintSetting>)>,
    cli: Vec<(CliLevel, Vec<String>)>,
}

impl Plan {
    pub(crate) fn new(
        cfg: &pun_config::Config,
        cli: &[(CliLevel, String)],
    ) -> Result<std::result::Result<Plan, Vec<String>>> {
        let mut unported = vec![];
        let mut base = BTreeMap::new();
        for r in &cfg.lint.rules {
            // Only a named rule can lack an engine: sets and groups bring in only `ported` rules, and
            // `mapping_ported_matches_engine` keeps `ported` and the engines in step.
            if engine(&r.id).is_none() {
                unported.push(r.id.clone());
                continue;
            }
            let on = On {
                level: level(r.level),
                fix: r.fix,
                options: r.options.clone(),
            };
            base.insert(r.id.clone(), on);
        }
        let mut overrides = vec![];
        for o in &cfg.lint.overrides {
            for s in &o.rules {
                if s.level.is_some() && engine(&s.id).is_none() {
                    unported.push(s.id.clone());
                }
            }
            overrides.push((
                Exclusions::globs(&cfg.root, o.files.clone())?,
                o.rules.clone(),
            ));
        }
        let mut levels = vec![];
        for (l, name) in cli {
            let ids = targets(name)?;
            if *l != CliLevel::Allow {
                unported.extend(ids.iter().filter(|id| engine(id).is_none()).cloned());
            }
            levels.push((*l, ids));
        }
        if !unported.is_empty() {
            unported.dedup();
            return Ok(Err(unported));
        }
        Ok(Ok(Plan {
            base,
            overrides,
            cli: levels,
        }))
    }

    pub(crate) fn mentioned(&self) -> BTreeSet<&str> {
        let overridden = (self.overrides.iter()).flat_map(|(_, s)| {
            s.iter()
                .filter(|s| s.level.is_some())
                .map(|s| s.id.as_str())
        });
        let cli = (self.cli.iter())
            .filter(|(l, _)| *l != CliLevel::Allow)
            .flat_map(|(_, ids)| ids.iter().map(String::as_str));
        (self.base.keys().map(String::as_str))
            .chain(overridden)
            .chain(cli)
            .filter(|id| engine(id).is_some())
            .collect()
    }

    pub(crate) fn for_file(&self, path: &Path) -> BTreeMap<String, On> {
        let mut on = self.base.clone();
        for (globs, settings) in &self.overrides {
            if !globs.is_excluded(path) {
                continue;
            }
            for s in settings {
                match s.level {
                    None => drop(on.remove(&s.id)),
                    Some(l) => {
                        let options = on.get(&s.id).map_or(Value::Null, |o| o.options.clone());
                        let rule = On {
                            level: level(l),
                            fix: s.fix,
                            options,
                        };
                        on.insert(s.id.clone(), rule);
                    }
                }
            }
        }
        for (l, ids) in &self.cli {
            for id in ids.iter().filter(|id| engine(id).is_some()) {
                let level = match l {
                    CliLevel::Allow => {
                        on.remove(id);
                        continue;
                    }
                    CliLevel::Warn => Level::Warning,
                    CliLevel::Deny => Level::Error,
                };
                on.entry(id.clone())
                    .and_modify(|o| o.level = level)
                    .or_insert(On {
                        level,
                        fix: true,
                        options: Value::Null,
                    });
            }
        }
        on
    }
}

fn targets(name: &str) -> Result<Vec<String>> {
    let id = match name.strip_prefix("lint/") {
        Some(_) => name.to_string(),
        None => format!("lint/{name}"),
    };
    if pun_config::rule(&id).is_some() {
        return Ok(vec![id]);
    }
    let prefix = format!("{id}/");
    let in_group: Vec<String> = (native_rules().iter().copied())
        .chain(token_rules().keys().copied())
        .chain([UNUSED_IGNORE])
        .filter(|r| r.starts_with(&prefix) && r.matches('/').count() == 2)
        .map(str::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if let Some(merged) = pun_config::merged_into(&id) {
        bail!("`{name}` was merged into `{merged}`: use `{merged}`");
    }
    if in_group.is_empty() && !is_group(&id) {
        bail!("`{name}` is not a lint rule or group");
    }
    Ok(in_group)
}

fn is_group(id: &str) -> bool {
    (id.strip_prefix("lint/")).is_some_and(|group| {
        !group.contains('/') && pun_config::groups().iter().any(|g| g == group)
    })
}

#[cfg(test)]
mod tests {
    use super::engine;
    use std::collections::BTreeSet;

    #[test]
    fn mapping_ported_matches_engine() {
        let mapping: toml::Table =
            toml::from_str(include_str!("../../pun-config/rules/mapping.toml"))
                .expect("mapping.toml parses");
        let fixers: BTreeSet<&str> = pun_fixers::all().iter().map(|f| f.name()).collect();
        let mut wrong = vec![];
        for command in ["format", "lint"] {
            for (rule, e) in mapping["rules"][command].as_table().expect("rules table") {
                let id = format!("{command}/{rule}");
                let runs = match command {
                    "lint" => engine(&id).is_some(),
                    _ => pun_config::fixer_of("format", &id).is_some_and(|f| fixers.contains(f)),
                };
                // Read as the lint sets read it: only `ported = false` keeps a rule out.
                let ported = e.get("ported").and_then(toml::Value::as_bool) != Some(false);
                wrong.extend(
                    (runs != ported).then_some(format!("{id}: ported = {ported}, runs = {runs}")),
                );
            }
        }
        assert!(
            wrong.is_empty(),
            "mapping.toml `ported` is out of date:\n{}",
            wrong.join("\n")
        );
    }
}
