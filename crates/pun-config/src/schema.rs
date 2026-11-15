use crate::composer::{Composer, Locked};
use crate::extensions::{self, ExtensionMeta};
use crate::{fixer_of, options, renamed, rules};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

pub use pun_tokens::PhpVersion;

pub const LINT_SETS: [&str; 8] = [
    "dead-code",
    "modernize",
    "code-quality",
    "types",
    "early-return",
    "privatization",
    "naming",
    "coding-style",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatPreset {
    Laravel,
    Psr12,
    Empty,
}

impl FormatPreset {
    pub const ALL: [Self; 3] = [Self::Laravel, Self::Psr12, Self::Empty];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Laravel => "laravel",
            Self::Psr12 => "psr12",
            Self::Empty => "empty",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == s)
    }
}

impl std::ops::Deref for FormatPreset {
    type Target = str;

    fn deref(&self) -> &str {
        self.name()
    }
}

impl PartialEq<&str> for FormatPreset {
    fn eq(&self, other: &&str) -> bool {
        self.name() == *other
    }
}

impl std::fmt::Display for FormatPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

impl Serialize for FormatPreset {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.name())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixSafety {
    Safe,
    Unsafe,
    Mixed,
    None,
}

impl FixSafety {
    pub(crate) fn parse(s: &str) -> Self {
        match s {
            "safe" => Self::Safe,
            "unsafe" => Self::Unsafe,
            "mixed" => Self::Mixed,
            _ => Self::None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Safe => "safe",
            Self::Unsafe => "unsafe",
            Self::Mixed => "mixed",
            Self::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AnalysePreset {
    Basic,
    Essential,
    Recommended,
    Strict,
    Max,
}

impl AnalysePreset {
    pub const ALL: [Self; 5] = [
        Self::Basic,
        Self::Essential,
        Self::Recommended,
        Self::Strict,
        Self::Max,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Basic => "basic",
            Self::Essential => "essential",
            Self::Recommended => "recommended",
            Self::Strict => "strict",
            Self::Max => "max",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.name() == s)
    }

    pub fn of_level(level: usize) -> Self {
        Self::ALL[[0, 0, 1, 1, 2, 2, 3, 3, 3, 4, 4][level]]
    }
}

impl std::fmt::Display for AnalysePreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

const SWITCHES: [(AnalysePreset, &str); 4] = [
    (AnalysePreset::Strict, "union-types"),
    (AnalysePreset::Strict, "nullables"),
    (AnalysePreset::Max, "explicit-mixed"),
    (AnalysePreset::Max, "implicit-mixed"),
];

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Files {
    pub paths: Vec<String>,
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LintRule {
    pub id: String,
    pub level: Level,
    pub fix: bool,
    pub options: Value,
    #[serde(skip)]
    pub fixer: Option<&'static str>,
    #[serde(skip)]
    pub named: bool,
}

impl LintRule {
    pub fn value(&self) -> Value {
        lint_value(&self.id, Some(self.level), self.fix, &self.options)
    }
}

impl LintSetting {
    pub fn value(&self) -> Value {
        lint_value(&self.id, self.level, self.fix, &self.options)
    }
}

fn lint_value(id: &str, level: Option<Level>, fix: bool, options: &Value) -> Value {
    let level = Value::from(match level {
        None => "off",
        Some(Level::Warn) => "warn",
        Some(Level::Error) => "error",
    });
    if fix && options.is_null() {
        return level;
    }
    let mut t = serde_json::Map::new();
    if level != "error" {
        t.insert("level".into(), level);
    }
    if !fix {
        t.insert("fix".into(), "off".into());
    }
    let options = match fixer_of("lint", id) {
        Some(f) if options.is_object() => options::to_pun(f, id, options).unwrap(),
        _ => options.clone(),
    };
    if let Value::Object(o) = options {
        t.extend(o);
    }
    Value::Object(t)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LintSetting {
    pub id: String,
    pub level: Option<Level>,
    pub fix: bool,
    pub options: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Override {
    pub files: Vec<String>,
    pub rules: Vec<LintSetting>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Lint {
    pub sets: Vec<String>,
    pub exclude: Vec<String>,
    pub ecosystems: Vec<&'static ExtensionMeta>,
    pub rules: Vec<LintRule>,
    pub overrides: Vec<Override>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnalyseRule {
    pub id: String,
    pub options: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Analyse {
    pub preset: AnalysePreset,
    pub paths: Vec<String>,
    pub exclude: Vec<String>,
    pub baseline: Option<String>,
    pub stubs: Vec<String>,
    pub ignore: Vec<Ignore>,
    pub switches: Vec<&'static str>,
    pub ecosystems: Vec<&'static ExtensionMeta>,
    pub larastan: bool,
    pub rules: Vec<AnalyseRule>,
    pub bleeding_edge: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Ignore {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_message: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct FilesToml {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct LintToml {
    #[serde(default)]
    sets: Vec<String>,
    #[serde(default)]
    exclude: Vec<String>,
    #[serde(default)]
    groups: toml::Table,
    #[serde(default)]
    rules: toml::Table,
    #[serde(default)]
    overrides: Vec<OverrideToml>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OverrideToml {
    files: Vec<String>,
    #[serde(default)]
    rules: toml::Table,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnalyseToml {
    preset: Option<toml::Value>,
    paths: Option<Vec<String>>,
    #[serde(default)]
    exclude: Vec<String>,
    baseline: Option<String>,
    #[serde(default)]
    stubs: Vec<String>,
    #[serde(default)]
    ignore: Vec<Ignore>,
    #[serde(default)]
    rules: toml::Table,
    #[serde(default, rename = "bleeding-edge")]
    bleeding_edge: bool,
    level: Option<toml::Value>,
}

impl Files {
    pub(crate) fn from(t: FilesToml) -> Files {
        Files {
            paths: t.paths,
            exclude: t.exclude,
        }
    }
}

pub(crate) struct Gates<'a> {
    pub php: PhpVersion,
    pub composer: &'a Composer,
    pub switches: BTreeMap<&'static str, extensions::Switch>,
}

impl Gates<'_> {
    fn locked(&self, e: &ExtensionMeta) -> Option<&Locked> {
        self.composer.locked(e.packages.first()?)
    }

    /// The extensions `[extensions]` and the lock turn on.
    pub(crate) fn extensions(&self) -> Vec<&'static ExtensionMeta> {
        (extensions::ALL.into_iter())
            .filter(|e| match self.switches.get(e.id) {
                Some(extensions::Switch::On) => true,
                Some(extensions::Switch::Off) => false,
                Some(extensions::Switch::Auto) | None => self.locked(e).is_some(),
            })
            .collect()
    }

    pub(crate) fn ecosystems(&self, off: impl Fn(&str) -> bool) -> Vec<&'static ExtensionMeta> {
        let mut on = self.extensions();
        on.retain(|e| !off(e.id));
        on
    }

    pub(crate) fn allows(&self, group: &str, on: &[&ExtensionMeta]) -> bool {
        if let Some(v) = group.strip_prefix("php-") {
            return v.parse().is_ok_and(|v: PhpVersion| self.php >= v);
        }
        match extensions::owner(group) {
            Some((e, None)) => on.contains(&e),
            Some((e, Some(v))) => {
                let locked = self.locked(e).and_then(|l| l.semver);
                on.contains(&e)
                    && version(v)
                        .zip(locked)
                        .is_some_and(|(v, l)| (l.major, l.minor) >= v)
            }
            None => true,
        }
    }
}

pub(crate) fn version(v: &str) -> Option<(u64, u64)> {
    let (major, minor) = v.split_once('.').unwrap_or((v, "0"));
    Some((major.parse().ok()?, minor.parse().ok()?))
}

pub fn groups() -> &'static Vec<String> {
    static T: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
        (crate::mapping::section(&["meta", "groups"]).table().keys())
            .cloned()
            .collect()
    });
    &T
}

fn group_of(id: &str) -> &str {
    id.split('/').nth(1).unwrap()
}

pub(crate) fn rule_id(path: &Path, command: &str, name: &str) -> Result<String> {
    let id = match name.strip_prefix(command).and_then(|r| r.strip_prefix('/')) {
        Some(_) => name.to_string(),
        None => format!("{command}/{name}"),
    };
    if fixer_of(command, name).is_some() || rules().iter().any(|(r, _)| *r == id) {
        return Ok(id);
    }
    let other = if command == "format" {
        "lint"
    } else {
        "format"
    };
    if let Some(id) = crate::merged_into(name) {
        bail!(
            "{}: `{name}` was merged into `{id}`: use `{id}` in pun.toml",
            path.display()
        );
    }
    let found = (fixer_of(other, name).and_then(crate::rule_of)).or_else(|| renamed(name));
    if let Some(id) = found {
        bail!("{}: `{name}` is `{id}` in pun.toml", path.display())
    }
    let a = if command == "analyse" { "an" } else { "a" };
    bail!("{}: `{name}` is not {a} {command} rule", path.display())
}

fn level(path: &Path, key: &str, s: &str) -> Result<Option<Level>> {
    Ok(match s {
        "off" => None,
        "warn" => Some(Level::Warn),
        "error" => Some(Level::Error),
        "info" => bail!(
            "{}: `{key}` = \"info\": the `info` level was removed; use \"warn\"",
            path.display()
        ),
        _ => bail!(
            "{}: `{key}` = \"{s}\": expected \"error\", \"warn\" or \"off\"",
            path.display()
        ),
    })
}

fn setting(path: &Path, name: &str, v: toml::Value) -> Result<LintSetting> {
    let id = rule_id(path, "lint", name)?;
    let shown = |v: &toml::Value| v.to_string();
    let (level, fix, opts) = match v {
        toml::Value::String(s) => (level(path, name, &s)?, true, toml::Table::new()),
        toml::Value::Boolean(b) => bail!(
            "{}: `{name}` = {b}: lint rules take a level; use \"{}\"",
            path.display(),
            if b { "error" } else { "off" }
        ),
        toml::Value::Table(mut t) => {
            let level = match t.remove("level") {
                None => Some(Level::Error),
                Some(toml::Value::String(s)) => level(path, name, &s)?,
                Some(v) => bail!(
                    "{}: `{name}`: level = {}: expected \"error\", \"warn\" or \"off\"",
                    path.display(),
                    shown(&v)
                ),
            };
            if t.contains_key("severity") {
                bail!(
                    "{}: `{name}`: `severity` was removed; use `level` (\"error\", \"warn\" or \"off\")",
                    path.display()
                );
            }
            let fix = match t.remove("fix") {
                None => true,
                Some(f) => match f.as_str() {
                    Some("off") => false,
                    Some("apply") => bail!(
                        "{}: `{name}`: fix = \"apply\" was removed; `--fix` applies the fix of every enabled rule, so leave `fix` out",
                        path.display()
                    ),
                    Some("safe-only") => bail!(
                        "{}: `{name}`: fix = \"safe-only\" was removed; `--fix` applies every fix, and fix = \"off\" keeps a rule report-only",
                        path.display()
                    ),
                    _ => bail!(
                        "{}: `{name}`: fix = {}: the only value is \"off\"",
                        path.display(),
                        shown(&f)
                    ),
                },
            };
            (level, fix, t)
        }
        v => bail!(
            "{}: `{name}` = {}: expected \"error\", \"warn\", \"off\" or a table",
            path.display(),
            shown(&v)
        ),
    };
    let options = if opts.is_empty() {
        Value::Null
    } else if fixer_of("lint", &id).is_some() {
        options::to_fixer(name, &id, toml::Value::Table(opts))
            .map_err(|e| anyhow::anyhow!("{}: {e}", path.display()))?
    } else {
        serde_json::to_value(opts).unwrap()
    };
    Ok(LintSetting {
        id,
        level,
        fix,
        options,
    })
}

pub(crate) fn lint(path: &Path, t: LintToml, files: &Files, gates: &Gates) -> Result<Lint> {
    for s in &t.sets {
        if !LINT_SETS.contains(&s.as_str()) {
            bail!(
                "{}: [lint] sets: `{s}` is not a lint set; the sets are {}",
                path.display(),
                list(&LINT_SETS)
            );
        }
    }
    let mut groups = BTreeMap::new();
    for (g, v) in t.groups {
        if !groups_contains(&g) {
            bail!(
                "{}: [lint.groups]: `{g}` is not a group (see [meta.groups] in crates/pun-config/rules/mapping.toml)",
                path.display()
            );
        }
        let toml::Value::String(s) = v else {
            bail!(
                "{}: [lint.groups] `{g}` = {v}: expected \"error\", \"warn\" or \"off\"",
                path.display()
            );
        };
        groups.insert(g.clone(), level(path, &g, &s)?);
    }
    let named = (t.rules.into_iter())
        .map(|(name, v)| setting(path, &name, v))
        .collect::<Result<Vec<_>>>()?;
    let overrides = (t.overrides.into_iter())
        .map(|o| {
            Ok(Override {
                files: o.files,
                rules: (o.rules.into_iter())
                    .map(|(name, v)| setting(path, &name, v))
                    .collect::<Result<_>>()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;

    let ecosystems = gates.ecosystems(|e| groups.get(e) == Some(&None));
    let rule = |id: String, level, fix, options, named| LintRule {
        fixer: fixer_of("lint", &id),
        id,
        level,
        fix,
        options,
        named,
    };
    let mut on: BTreeMap<String, LintRule> = BTreeMap::new();
    if !t.sets.is_empty() || !groups.is_empty() {
        let ported =
            |e: &toml::Value| e.get("ported").and_then(toml::Value::as_bool) != Some(false);
        for (id, e) in rules()
            .iter()
            .filter(|(id, e)| id.starts_with("lint/") && ported(e))
        {
            let group = group_of(id);
            let level = if let Some(l) = groups.get(group) {
                *l
            } else {
                let in_set = (e.get("sets").and_then(toml::Value::as_array)).is_some_and(|s| {
                    s.iter()
                        .any(|s| t.sets.iter().any(|t| s.as_str() == Some(t)))
                });
                (in_set && gates.allows(group, &ecosystems)).then_some(Level::Error)
            };
            if let Some(level) = level {
                on.insert(
                    id.clone(),
                    rule(id.clone(), level, true, Value::Null, false),
                );
            }
        }
    }
    for s in named {
        match s.level {
            Some(level) => drop(on.insert(s.id.clone(), rule(s.id, level, s.fix, s.options, true))),
            None => drop(on.remove(&s.id)),
        }
    }
    Ok(Lint {
        sets: t.sets,
        exclude: [&files.exclude[..], &t.exclude[..]].concat(),
        ecosystems,
        rules: on.into_values().collect(),
        overrides,
    })
}

fn groups_contains(g: &str) -> bool {
    groups().iter().any(|x| x == g)
}

fn list(items: &[&str]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| format!("`{s}`")).collect();
    quoted.join(", ")
}

#[expect(
    clippy::too_many_lines,
    reason = "long; to be split (code audit: functions over 100 lines)"
)]
pub(crate) fn analyse(
    path: &Path,
    t: AnalyseToml,
    files: &Files,
    gates: &Gates,
) -> Result<Analyse> {
    if let Some(level) = t.level {
        let hint = level
            .as_integer()
            .and_then(|l| usize::try_from(l).ok())
            .filter(|l| *l <= 10)
            .map_or_default(|l| {
                format!(": level {l} is preset = \"{}\"", AnalysePreset::of_level(l))
            });
        let presets: Vec<String> = (AnalysePreset::ALL.iter())
            .map(|p| format!("\"{p}\""))
            .collect();
        bail!(
            "{}: [analyse] level: PHPStan's levels are presets in Pun; use preset = {}{hint}",
            path.display(),
            presets.join(" | ")
        );
    }
    let preset = match t.preset {
        None => AnalysePreset::Recommended,
        Some(v) if let Some(p) = v.as_str().and_then(AnalysePreset::parse) => p,
        Some(toml::Value::Integer(l)) if let Ok(level @ 0..=10) = usize::try_from(l) => bail!(
            "{}: [analyse] preset = {l}: PHPStan level {l} is preset = \"{}\"",
            path.display(),
            AnalysePreset::of_level(level)
        ),
        Some(v) => bail!(
            "{}: [analyse] preset = {v}: expected {}",
            path.display(),
            list(&AnalysePreset::ALL.map(AnalysePreset::name))
        ),
    };

    let mut groups = BTreeMap::new();
    let mut named = vec![];
    for (name, v) in t.rules {
        let on = match v {
            toml::Value::String(s) if s == "on" => Some(Value::Null),
            toml::Value::String(s) if s == "off" => None,
            toml::Value::Table(t) => Some(serde_json::to_value(t).unwrap()),
            v => bail!(
                "{}: [analyse.rules] `{name}` = {v}: analyse rules are \"on\" or \"off\" (or a table of options); there are no levels",
                path.display()
            ),
        };
        if groups_contains(&name) {
            if !matches!(on, None | Some(Value::Null)) {
                bail!(
                    "{}: [analyse.rules] `{name}` is a group: it takes \"on\" or \"off\"",
                    path.display()
                );
            }
            groups.insert(name, on.is_some());
        } else {
            named.push((rule_id(path, "analyse", &name)?, on));
        }
    }
    let ecosystems = gates.ecosystems(|e| groups.get(e) == Some(&false));
    let mut on: BTreeMap<String, Value> = BTreeMap::new();
    for (id, e) in rules().iter().filter(|(id, _)| id.starts_with("analyse/")) {
        let group = group_of(id);
        let enabled = match groups.get(group) {
            Some(g) => *g,
            None => {
                (e.get("preset").and_then(toml::Value::as_str))
                    .and_then(AnalysePreset::parse)
                    .is_some_and(|p| p <= preset)
                    && gates.allows(group, &ecosystems)
            }
        };
        if enabled {
            on.insert(id.clone(), Value::Null);
        }
    }
    for (id, v) in named {
        match v {
            Some(v) => drop(on.insert(id, v)),
            None => drop(on.remove(&id)),
        }
    }
    let mut ignore = t.ignore;
    for (n, i) in ignore.iter_mut().enumerate() {
        let at = || format!("{}: [[analyse.ignore]] #{}", path.display(), n + 1);
        if let Some(r) = &i.rule {
            i.rule = Some(rule_id(path, "analyse", r)?);
        }
        if i.message.is_some() && i.raw_message.is_some() {
            bail!("{}: message and raw-message: give one", at());
        }
        if i.rule.is_none()
            && i.identifier.is_none()
            && i.message.is_none()
            && i.raw_message.is_none()
        {
            bail!("{}: name a rule, an identifier or a message", at());
        }
    }
    Ok(Analyse {
        switches: (SWITCHES.iter())
            .filter(|(p, _)| *p <= preset)
            .map(|(_, s)| *s)
            .collect(),
        preset,
        paths: t.paths.unwrap_or_else(|| files.paths.clone()),
        exclude: [&files.exclude[..], &t.exclude[..]].concat(),
        baseline: t.baseline,
        stubs: t.stubs,
        ignore,
        larastan: ecosystems.contains(&&extensions::LARAVEL)
            && crate::phpstan_parameters::larastan_supports(
                gates.locked(&extensions::LARAVEL).and_then(|l| l.semver),
            ),
        ecosystems,
        rules: (on.into_iter())
            .map(|(id, options)| AnalyseRule { id, options })
            .collect(),
        bleeding_edge: t.bleeding_edge,
    })
}
