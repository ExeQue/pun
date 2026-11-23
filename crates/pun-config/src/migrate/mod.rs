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
    pub php: String,
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
            ("[php]\n", self.php.clone()),
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
    if preset == "symfony" {
        bail!(
            "the symfony preset was removed: Pun supports the laravel, psr12, per and empty presets"
        );
    }
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
    let (paths, finder, not_name) = finder(&pint);
    let off = disabled(pint.get("rules").and_then(Value::as_object));
    let unsupported = (not_name.then(|| "notName: no pun.toml key for it".to_string())).into_iter();
    write(
        preset,
        rule_set(rules)?,
        &off,
        &paths,
        &finder,
        unsupported.collect(),
        runs,
    )
}

pub fn from_rules(
    rules: Map<String, Value>,
    finder_of: &Map<String, Value>,
    runs: Runs,
) -> Result<Migration> {
    let (paths, finder, not_name) = finder(finder_of);
    let unsupported = (not_name.then_some("notName()").into_iter())
        .chain(
            (finder_of.get("unsupported").and_then(Value::as_array))
                .into_iter()
                .flatten()
                .filter_map(Value::as_str),
        )
        .map(|s| format!("Finder {s}: no pun.toml key for it"))
        .collect();
    let off = disabled(Some(&rules));
    write(
        "empty",
        rule_set(rules)?,
        &off,
        &paths,
        &finder,
        unsupported,
        runs,
    )
}

/// The rules a config turns off itself (`"rule": false`), not through a set or preset.
fn disabled(rules: Option<&Map<String, Value>>) -> Vec<String> {
    (rules.into_iter().flatten())
        .filter(|(name, v)| !name.starts_with('@') && **v == Value::Bool(false))
        .map(|(name, _)| name.clone())
        .collect()
}

/// `[files]` and `[format]`'s finder keys, and whether the config sets `notName`, which pun.toml has no key for.
fn finder(config: &Map<String, Value>) -> (String, String, bool) {
    let (mut paths, mut finder) = (String::new(), String::new());
    let not_name = match config.get("notName") {
        Some(Value::String(_)) => true,
        Some(Value::Array(a)) => !a.is_empty(),
        _ => false,
    };
    for (json, key) in [
        ("in", "paths"),
        ("exclude", "exclude"),
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
    (paths, finder, not_name)
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
    off: &[String],
    paths: &str,
    finder: &str,
    mut unsupported: Vec<String>,
    runs: Runs,
) -> Result<Migration> {
    let preset = crate::FormatPreset::parse(preset).map_or("empty", crate::FormatPreset::name);
    resolve_proxies(&mut rules);
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
    // An extension brings every rule of its group (`phpunit/require-method-casing` for `php_unit_method_casing`), so
    // one the config turns off stays off, as in Pint.
    for name in off.iter().filter(|n| runs(n)) {
        let Some(id) = rule_of(name).and_then(|id| id.strip_prefix("lint/")) else {
            continue;
        };
        let owned = (id.split('/').next()).is_some_and(|g| crate::extensions::owner(g).is_some());
        if owned && !parts.lint_rules.iter().any(|(k, _)| k == id) {
            parts.lint_rules.push((id.to_string(), "\"off\"".into()));
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

/// PHP-CS-Fixer's deprecated proxy fixers run only their successor: with the proxy's own configuration, or with
/// the fixed one the proxy gives it (its `createProxyFixers()`). Beside the successor (from a set such as @PSR12)
/// both run, in name order at their equal priority (`Utils::sortFixers`): the later name's options win. A proxy
/// run within the successor's default run adds nothing; any other second run is one a pun.toml rule can't hold,
/// so the proxy stays and is named as skipped.
fn resolve_proxies(rules: &mut Vec<Rule>) {
    for (old, new, fixed, within_defaults) in PROXIES {
        let Some(i) = rules.iter().position(|r| r.name == *old) else {
            continue;
        };
        if let Some(j) = rules.iter().position(|r| r.name == *new) {
            match rules[i].options.clone() {
                Value::Object(own) if fixed.is_none() => {
                    let mut merged = rules[j].options.as_object().cloned().unwrap_or_default();
                    if old > new {
                        merged.extend(own);
                    } else {
                        let mut first = own;
                        first.extend(merged);
                        merged = first;
                    }
                    rules[j].options = Value::Object(merged);
                    rules.remove(i);
                }
                Value::Null if *within_defaults && rules[j].options.is_null() => {
                    rules.remove(i);
                }
                _ => {}
            }
            continue;
        }
        let proxy = rules.remove(i);
        let options = fixed.map_or(proxy.options, |f| serde_json::from_str(f).unwrap());
        rules.insert(
            i,
            Rule {
                name: (*new).into(),
                options,
            },
        );
    }
}

/// PHP-CS-Fixer's deprecated fixers that only run a successor: (name, successor, the successor's fixed configuration
/// when the proxy sets one instead of passing its own through, whether that run is within the successor's default
/// one). The successors' defaults: `no_trailing_comma_in_singleline` all four elements, `type_declaration_spaces`
/// function and property, `blank_lines_before_namespace` 2 and 2.
const PROXIES: &[(&str, &str, Option<&str>, bool)] = &[
    (
        "compact_nullable_typehint",
        "compact_nullable_type_declaration",
        None,
        true,
    ),
    ("curly_braces_position", "braces_position", None, true),
    (
        "function_typehint_space",
        "type_declaration_spaces",
        Some(r#"{"elements":["function"]}"#),
        true,
    ),
    (
        "native_function_type_declaration_casing",
        "native_type_declaration_casing",
        None,
        true,
    ),
    ("new_with_braces", "new_with_parentheses", None, true),
    (
        "no_blank_lines_before_namespace",
        "blank_lines_before_namespace",
        Some(r#"{"min_line_breaks":0,"max_line_breaks":1}"#),
        false,
    ),
    (
        "no_spaces_inside_parenthesis",
        "spaces_inside_parentheses",
        None,
        true,
    ),
    (
        "no_trailing_comma_in_list_call",
        "no_trailing_comma_in_singleline",
        Some(r#"{"elements":["array_destructuring"]}"#),
        true,
    ),
    (
        "no_trailing_comma_in_singleline_array",
        "no_trailing_comma_in_singleline",
        Some(r#"{"elements":["array"]}"#),
        true,
    ),
    (
        "no_trailing_comma_in_singleline_function_call",
        "no_trailing_comma_in_singleline",
        Some(r#"{"elements":["arguments","array_destructuring"]}"#),
        true,
    ),
    ("no_unneeded_curly_braces", "no_unneeded_braces", None, true),
    (
        "single_blank_line_before_namespace",
        "blank_lines_before_namespace",
        Some(r#"{"min_line_breaks":2,"max_line_breaks":2}"#),
        true,
    ),
];

fn toml_value(v: &Value) -> Option<String> {
    serde_json::from_value::<toml::Value>(v.clone())
        .ok()
        .map(|t| t.to_string())
}
