use super::Parts;
use crate::composer::Composer;
use crate::{Config, mapping, rules};
use anyhow::Result;
use serde::Deserialize;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write;
use std::path::Path;

#[derive(Debug, Default, Deserialize)]
pub struct RectorConfig {
    pub rules: Vec<String>,
    #[serde(default)]
    pub configured: BTreeMap<String, Configured>,
    #[serde(default)]
    pub skip_rules: Map<String, Value>,
    #[serde(default)]
    pub skip_paths: Vec<String>,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub sets: Vec<String>,
    #[serde(default)]
    pub php_sets: bool,
}

#[derive(Debug, Default, Deserialize)]
pub struct Configured {
    pub configuration: Value,
    #[serde(default)]
    pub constants: Vec<String>,
}

fn kebab(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_uppercase() && !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
        out.push(if c == '_' {
            '-'
        } else {
            c.to_ascii_lowercase()
        });
    }
    out
}

fn toml_value(v: &Value) -> Result<toml::Value, String> {
    Ok(match v {
        Value::Null => return Err("its configuration holds a null, which pun.toml can't".into()),
        Value::Bool(b) => toml::Value::Boolean(*b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => toml::Value::Integer(i),
            None => toml::Value::Float(n.as_f64().unwrap()),
        },
        Value::String(s) => toml::Value::String(s.clone()),
        Value::Array(a) => toml::Value::Array(a.iter().map(toml_value).collect::<Result<_, _>>()?),
        Value::Object(o) => {
            if let Some(t) = o.get("@unsupported").and_then(Value::as_str) {
                return Err(format!(
                    "its configuration holds a {t}, which pun.toml can't"
                ));
            }
            let mut table = toml::Table::new();
            match o.get("@props").and_then(Value::as_object) {
                Some(props) => {
                    for (k, v) in props.iter().filter(|(_, v)| !v.is_null()) {
                        table.insert(kebab(k), toml_value(v)?);
                    }
                }
                None => {
                    for (k, v) in o {
                        table.insert(k.clone(), toml_value(v)?);
                    }
                }
            }
            toml::Value::Table(table)
        }
    })
}

fn rule_options(c: &Configured) -> Result<toml::Table, String> {
    let named = matches!(&c.configuration, Value::Object(o)
        if !o.is_empty() && o.keys().all(|k| c.constants.contains(k)));
    let table: toml::Table = match toml_value(&c.configuration)? {
        toml::Value::Table(t) => (t.into_iter())
            .map(|(k, v)| (if named { kebab(&k) } else { k }, v))
            .collect(),
        v => toml::Table::from_iter([("items".to_string(), v)]),
    };
    match ["level", "fix"]
        .into_iter()
        .find(|k| table.contains_key(*k))
    {
        Some(k) => Err(format!(
            "its configuration has a `{k}` key, which pun.toml keeps for the rule's level"
        )),
        None => Ok(table),
    }
}

#[derive(Debug, Default)]
pub struct Rector {
    pub sets: Vec<&'static str>,
    pub want: Vec<(String, String)>,
    pub overrides: Vec<(Vec<String>, String)>,
    pub exclude: Vec<String>,
    pub paths: Vec<String>,
    pub unsupported: Vec<(String, String)>,
    pub options: BTreeMap<String, toml::Table>,
}

const PREPARED: [(&str, &str); 7] = [
    ("dead-code.php", "dead-code"),
    ("code-quality.php", "code-quality"),
    ("coding-style.php", "coding-style"),
    ("type-declaration.php", "types"),
    ("privatization.php", "privatization"),
    ("naming.php", "naming"),
    ("early-return.php", "early-return"),
];

/// The Pun options that keep a rule merged with a PHP-CS-Fixer fixer to what its Rector rules check, so a project
/// clean under Rector is clean under Pun. The fixer's own checks run only when it is named in pint.json.
const AS_RECTOR: [(&str, &str); 3] = [
    ("lint/control-flow/no-return-assignment", "skip-commented"),
    ("lint/style/prefer-class-constant", "namespaced-only"),
    (
        "lint/style/prefer-empty-string-compare",
        "identical-or-greater-only",
    ),
];

/// A rule merged from several Rector rules runs only the rewrites of the ones rector.php runs (`classes`, short names):
/// the rule's Pun-only list option whose values name its Rector rules, set to those, when it leaves one out.
fn only_these(id: &str, classes: &HashSet<String>) -> Option<(String, toml::Value)> {
    let option = crate::options::rule_options(id).iter().find(|o| {
        o.pun
            && o.kind == crate::options::Kind::List
            && (o.values.iter()).any(|(_, rector)| classes.contains(&format!("{rector}Rector")))
    })?;
    let parts: Vec<toml::Value> = (option.values.iter())
        .filter(|(_, rector)| classes.contains(&format!("{rector}Rector")))
        .map(|(pun, _)| toml::Value::String(pun.clone()))
        .collect();
    (parts.len() < option.values.len()).then(|| (option.name.clone(), toml::Value::Array(parts)))
}

fn by_class() -> &'static BTreeMap<String, toml::Value> {
    static T: std::sync::LazyLock<BTreeMap<String, toml::Value>> = std::sync::LazyLock::new(|| {
        let mut out = BTreeMap::new();
        for (tool, s) in mapping::under("upstream") {
            if !(tool.starts_with("rector") || tool == "pest-plugin-rector") {
                continue;
            }
            for e in s.table().values() {
                if let Some(c) = e.get("class").and_then(toml::Value::as_str) {
                    out.insert(c.to_string(), e.clone());
                }
            }
        }
        out
    });
    &T
}

fn short(class: &str) -> String {
    class.rsplit('\\').next().unwrap().to_string()
}

fn relative(root: &Path, path: &str) -> String {
    let p = Path::new(path);
    match p.strip_prefix(root) {
        Ok(r) if r.as_os_str().is_empty() => ".".into(),
        Ok(r) => r.display().to_string(),
        Err(_) => path.trim_end_matches('/').to_string(),
    }
}

pub fn from_rector(root: &Path, r: &RectorConfig) -> Rector {
    let mut out = Rector::default();
    for set in &r.sets {
        let file = set.rsplit('/').next().unwrap();
        if let Some((_, pun)) = PREPARED.iter().find(|(f, _)| *f == file)
            && set.contains("/rector/rector/")
            && !out.sets.contains(pun)
        {
            out.sets.push(pun);
        }
    }
    if r.php_sets {
        out.sets.push("modernize");
    }
    let mut seen = HashSet::new();
    let mut runs: BTreeMap<String, HashSet<String>> = BTreeMap::new();
    for class in &r.rules {
        let skip = r.skip_rules.get(class);
        if skip.is_some_and(Value::is_null) {
            continue;
        }
        let entry = by_class().get(class);
        let pun = entry
            .and_then(|e| e.get("pun"))
            .and_then(toml::Value::as_str);
        let why = match (entry, pun) {
            (None, _) => Some("not a Rector rule Pun knows (a custom rule or another package's)"),
            (Some(e), None) => e
                .get("unsupported")
                .or(e.get("drop"))
                .and_then(toml::Value::as_str),
            _ => None,
        };
        if let Some(why) = why {
            out.unsupported.push((short(class), why.to_string()));
            continue;
        }
        let id = pun.unwrap().to_string();
        match r.configured.get(class).map(rule_options) {
            Some(Err(why)) => {
                out.unsupported
                    .push((short(class), format!("configured; {why}")));
                continue;
            }
            Some(Ok(options)) => {
                out.options.insert(id.clone(), options);
            }
            None => {}
        }
        if let Some((_, key)) = AS_RECTOR.iter().find(|(r, _)| *r == id) {
            (out.options.entry(id.clone()).or_default())
                .insert((*key).to_string(), toml::Value::Boolean(true));
        }
        if let Some(paths) = skip.and_then(Value::as_array)
            && id.starts_with("lint/")
        {
            let paths = (paths.iter().filter_map(Value::as_str))
                .map(|p| relative(root, p))
                .collect();
            out.overrides.push((paths, id.clone()));
        }
        (runs.entry(id.clone()).or_default()).insert(short(class));
        if seen.insert(id.clone()) {
            out.want.push((id, short(class)));
        }
    }
    for (id, classes) in &runs {
        if let Some((key, parts)) = only_these(id, classes) {
            (out.options.entry(id.clone()).or_default()).insert(key, parts);
        }
    }
    out.exclude = r.skip_paths.iter().map(|p| relative(root, p)).collect();
    out.paths = r.paths.iter().map(|p| relative(root, p)).collect();
    out
}

#[derive(Debug, Default, PartialEq)]
pub struct Reconciled {
    pub on: usize,
    pub off: usize,
    pub waiting: usize,
    pub unsupported: Vec<(String, String)>,
}

pub fn compose(root: &Path, parts: &mut Parts, rector: &Rector) -> Result<Reconciled> {
    let quoted = |v: &[String]| toml::Value::try_from(v).unwrap().to_string();
    if !rector.sets.is_empty() {
        let sets: Vec<String> = rector
            .sets
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        writeln!(parts.lint, "sets = {}", quoted(&sets)).unwrap();
    }
    if !rector.exclude.is_empty() {
        writeln!(parts.lint, "exclude = {}", quoted(&rector.exclude)).unwrap();
    }
    let mut out = Reconciled::default();
    let cfg = resolve(root, &parts.render())?;
    let on: Vec<&str> = cfg.lint.rules.iter().map(|r| r.id.as_str()).collect();
    let given: HashSet<String> = (parts.lint_rules.iter())
        .map(|(k, _)| format!("lint/{k}"))
        .collect();
    let want: HashSet<&str> = rector.want.iter().map(|w| w.0.as_str()).collect();
    for (id, class) in &rector.want {
        if !id.starts_with("lint/") {
            let runs = (cfg.rules.iter()).any(|r| crate::rule_of(&r.name) == Some(id.as_str()));
            let why = format!("the format rule `{id}` in Pun; turn it on in [format.rules]");
            out.unsupported
                .extend((!runs).then(|| (class.clone(), why)));
            continue;
        }
        if given.contains(id) {
            continue;
        }
        let options = rector.options.get(id);
        if on.contains(&id.as_str()) && options.is_none() {
            out.waiting += usize::from(!ported(id));
        } else {
            let value = options.map_or("\"error\"".into(), |t| {
                toml::Value::Table(t.clone()).to_string()
            });
            let turned_on = super::require_lint(parts, id, &value);
            out.on += usize::from(turned_on);
            let why = "not supported by Pun yet".to_string();
            out.unsupported
                .extend((!turned_on).then(|| (class.clone(), why)));
        }
    }
    for id in on {
        if !want.contains(id) && !given.contains(id) {
            parts
                .lint_rules
                .push((id[5..].to_string(), "\"off\"".into()));
            out.off += 1;
        }
    }
    let mut by_paths: Vec<(&Vec<String>, Vec<&str>)> = vec![];
    for (paths, id) in &rector.overrides {
        match by_paths.iter_mut().find(|(p, _)| *p == paths) {
            Some((_, ids)) => ids.push(&id[5..]),
            None => by_paths.push((paths, vec![&id[5..]])),
        }
    }
    for (paths, ids) in by_paths {
        let rules: Vec<String> = ids.iter().map(|id| format!("\"{id}\" = \"off\"")).collect();
        write!(
            parts.overrides,
            "{}[[lint.overrides]]\nfiles = {}\nrules = {{ {} }}\n",
            if parts.overrides.is_empty() { "" } else { "\n" },
            quoted(paths),
            rules.join(", ")
        )
        .unwrap();
    }
    Ok(out)
}

pub(super) fn ported(id: &str) -> bool {
    (rules().iter().find(|(r, _)| r == id))
        .and_then(|(_, e)| e.get("ported"))
        .and_then(toml::Value::as_bool)
        == Some(true)
}

pub(super) fn resolve(root: &Path, text: &str) -> Result<Config> {
    let composer: Composer = crate::composer::detect(root)?;
    crate::parse(
        root,
        &root.join("pun.toml"),
        text,
        &crate::Options::default(),
        &composer,
        vec![],
    )
}
