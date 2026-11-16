pub mod phpstan;
mod rector;

pub use rector::{Configured, Reconciled, Rector, RectorConfig, compose, from_rector};

use crate::{Rule, fixer_entries, preset_rules, rule_of, rule_set};
use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use std::fmt::Write;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Parts {
    pub files: String,
    pub extensions: String,
    pub format: String,
    pub format_rules: String,
    pub lint: String,
    pub lint_rules: Vec<(String, String)>,
    pub overrides: String,
    pub analyse: String,
    pub analyse_rules: Vec<(String, String)>,
    pub analyse_ignores: String,
}

pub(super) fn require_lint(parts: &mut Parts, id: &str, value: &str) -> bool {
    let ported = rector::ported(id);
    let name = &id[5..];
    parts.lint_rules.retain(|(k, _)| !(ported && k == name));
    (parts.lint_rules).extend(ported.then(|| (name.to_string(), value.to_string())));
    ported
}

impl Parts {
    pub fn render(&self) -> String {
        let pairs = |rules: &[(String, String)]| -> String {
            rules.iter().fold(String::new(), |mut out, (k, v)| {
                let _ = writeln!(out, "\"{k}\" = {v}");
                out
            })
        };
        let sections = [
            ("[files]\n", self.files.clone()),
            ("[extensions]\n", self.extensions.clone()),
            ("[format]\n", self.format.clone()),
            ("[format.rules]\n", self.format_rules.clone()),
            ("[lint]\n", self.lint.clone()),
            ("[lint.rules]\n", pairs(&self.lint_rules)),
            ("", self.overrides.clone()),
            ("[analyse]\n", self.analyse.clone()),
            ("[analyse.rules]\n", pairs(&self.analyse_rules)),
            ("", self.analyse_ignores.clone()),
        ];
        let shown: Vec<String> = (sections.into_iter())
            .filter(|(_, body)| !body.is_empty())
            .map(|(head, body)| format!("{head}{body}"))
            .collect();
        shown.join("\n")
    }
}

pub struct Migration {
    pub toml: String,
    pub parts: Parts,
    pub unsupported: Vec<String>,
    pub preset: &'static str,
    pub format_rules: usize,
    pub lint_rules: usize,
}

pub type Runs<'a> = &'a dyn Fn(&str) -> bool;

/// `[extensions]`'s body: each extension `composer` shows the project uses, turned on.
pub fn extensions(composer: &crate::composer::Composer) -> String {
    (crate::extensions::detected(composer).into_iter()).fold(String::new(), |mut out, e| {
        let _ = writeln!(out, "{} = \"on\"", e.id);
        out
    })
}

pub fn from_pint(path: &Path, preset: Option<&str>, runs: Runs) -> Result<Migration> {
    let pint = read_pint_json(path)?;
    let preset = preset
        .or_else(|| pint.get("preset").and_then(Value::as_str))
        .unwrap_or("laravel");
    let mut rules = preset_rules(preset).context("Preset not found.")?;
    if let Some(r) = pint.get("rules").and_then(Value::as_object) {
        rules.extend(r.clone());
    }
    for (name, v) in &rules {
        if !matches!(v, Value::Bool(_) | Value::Array(_) | Value::Object(_)) {
            let (what, kind) = if name.starts_with('@') {
                ("Set must be enabled (true) or disabled (false).", "set")
            } else {
                (
                    "Rule must be enabled (true), disabled (false) or configured (non-empty, assoc array).",
                    "rule",
                )
            };
            let null = if v.is_null() {
                format!(" To disable the {kind}, use \"FALSE\" instead of \"NULL\".")
            } else {
                String::new()
            };
            bail!("[{name}] {what} Other values are not allowed.{null}");
        }
    }
    let (paths, finder) = finder(&pint);
    write(preset, rule_set(rules)?, &paths, &finder, vec![], runs)
}

pub fn from_rules(
    rules: Map<String, Value>,
    finder_of: &Map<String, Value>,
    runs: Runs,
) -> Result<Migration> {
    let (paths, finder) = finder(finder_of);
    let unsupported = (finder_of
        .get("unsupported")
        .and_then(Value::as_array)
        .into_iter()
        .flatten())
    .filter_map(Value::as_str)
    .map(|s| format!("Finder {s}: no pun.toml key for it"))
    .collect();
    write(
        "empty",
        rule_set(rules)?,
        &paths,
        &finder,
        unsupported,
        runs,
    )
}

fn finder(config: &Map<String, Value>) -> (String, String) {
    let (mut paths, mut finder) = (String::new(), String::new());
    for (json, key) in [
        ("in", "paths"),
        ("exclude", "exclude"),
        ("notName", "not-name"),
        ("notPath", "not-path"),
    ] {
        let list: Vec<&str> = match config.get(json) {
            Some(Value::String(s)) => vec![s],
            Some(Value::Array(a)) if !a.is_empty() => a.iter().filter_map(Value::as_str).collect(),
            _ => continue,
        };
        let out = if matches!(json, "in" | "exclude") {
            &mut paths
        } else {
            &mut finder
        };
        writeln!(out, "{key} = {}", toml::Value::try_from(list).unwrap()).unwrap();
    }
    (paths, finder)
}

fn read_pint_json(path: &Path) -> Result<Map<String, Value>> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let invalid = || {
        format!(
            "The configuration file [{}] is not valid JSON.",
            path.display()
        )
    };
    let raw = std::fs::read(path).with_context(|| format!("can't read {}", path.display()))?;
    let mut v: Value = serde_json::from_slice(&raw).with_context(invalid)?;
    if let Some(ext) = v.get("extend").and_then(Value::as_str) {
        let ext_path = path.parent().unwrap().join(ext);
        let raw =
            std::fs::read(&ext_path).map_err(|_| anyhow::anyhow!("Path must not be empty"))?;
        let base: Value = serde_json::from_slice(&raw).unwrap_or(Value::Null);
        let given = match &base {
            Value::Object(_) | Value::Array(_) => None,
            Value::Null => Some("null"),
            Value::Bool(true) => Some("true"),
            Value::Bool(false) => Some("false"),
            Value::Number(n) if n.is_i64() => Some("int"),
            Value::Number(_) => Some("float"),
            Value::String(_) => Some("string"),
        };
        if let Some(t) = given {
            bail!(
                "array_replace_recursive(): Argument #1 ($array) must be of type array, {t} given"
            );
        }
        if base.get("extend").is_some() {
            bail!("Pint configuration cannot extend from more than 1 file.");
        }
        let mut merged = base;
        replace_recursive(&mut merged, v);
        v = merged;
    }
    match v {
        Value::Object(m) => Ok(m),
        Value::Array(_) => Ok(Map::new()),
        _ => bail!(invalid()),
    }
}

fn replace_recursive(base: &mut Value, over: Value) {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(slot) => replace_recursive(slot, v),
                    None => {
                        b.insert(k, v);
                    }
                }
            }
        }
        (Value::Array(b), Value::Array(o)) => {
            for (i, v) in o.into_iter().enumerate() {
                match b.get_mut(i) {
                    Some(slot) => replace_recursive(slot, v),
                    None => b.push(v),
                }
            }
        }
        (b, o) => *b = o,
    }
}

fn write(
    preset: &str,
    mut rules: Vec<Rule>,
    paths: &str,
    finder: &str,
    mut unsupported: Vec<String>,
    runs: Runs,
) -> Result<Migration> {
    let preset = crate::FormatPreset::parse(preset).map_or("empty", crate::FormatPreset::name);
    if let Some(i) = rules.iter().position(|r| r.name == "visibility_required") {
        let vr = rules.remove(i).options;
        match rules.iter_mut().find(|r| r.name == "modifier_keywords") {
            None => rules.push(Rule {
                name: "modifier_keywords".into(),
                options: vr,
            }),
            Some(mk) if vr.is_null() || mk.options.is_null() => mk.options = Value::Null,
            Some(mk) => {
                let mut all = mk.options["elements"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                for e in vr["elements"].as_array().into_iter().flatten() {
                    if !all.contains(e) {
                        all.push(e.clone());
                    }
                }
                mk.options = serde_json::json!({ "elements": all });
            }
        }
    }
    let base = rule_set(preset_rules(preset).unwrap())?;
    let mut parts = Parts {
        files: paths.to_string(),
        format: format!("preset = \"{preset}\"\n{finder}"),
        ..Parts::default()
    };
    let mut format_rules = 0;
    for r in &rules {
        let id = rule_of(&r.name);
        let lint = id.is_some_and(|id| id.starts_with("lint/"));
        let Some(id) = id.filter(|_| runs(&r.name)) else {
            let entry = fixer_entries().get(&r.name);
            let why = entry
                .and_then(|e| e.get("drop").or(e.get("unsupported")))
                .and_then(toml::Value::as_str)
                .unwrap_or(match id {
                    Some(_) => "not supported by Pun yet",
                    None => {
                        "not a PHP-CS-Fixer or Pint rule Pun knows (a custom fixer or rule set)"
                    }
                });
            unsupported.push(format!("{}: {why}", r.name));
            continue;
        };
        if !lint
            && base
                .iter()
                .any(|b| b.name == r.name && b.options == r.options)
        {
            continue;
        }
        let name = &id[id.find('/').unwrap() + 1..];
        let v = match crate::options::to_pun(&r.name, id, &r.options)? {
            Value::Bool(true) if lint => Value::from("error"),
            v => v,
        };
        let Some(v) = toml_value(&v) else {
            unsupported.push(format!(
                "{}: an option is null, which TOML can't hold",
                r.name
            ));
            continue;
        };
        if lint {
            parts.lint_rules.push((name.to_string(), v));
        } else {
            writeln!(parts.format_rules, "\"{name}\" = {v}").unwrap();
            format_rules += 1;
        }
    }
    for b in &base {
        let off = !rules.iter().any(|r| r.name == b.name) || !runs(&b.name);
        if let Some(name) = rule_of(&b.name).and_then(|id| id.strip_prefix("format/"))
            && off
        {
            writeln!(parts.format_rules, "\"{name}\" = false").unwrap();
            format_rules += 1;
        }
    }
    Ok(Migration {
        toml: parts.render(),
        lint_rules: parts.lint_rules.len(),
        parts,
        unsupported,
        preset,
        format_rules,
    })
}

fn toml_value(v: &Value) -> Option<String> {
    serde_json::from_value::<toml::Value>(v.clone())
        .ok()
        .map(|t| t.to_string())
}
