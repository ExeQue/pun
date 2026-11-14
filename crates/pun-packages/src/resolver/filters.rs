//! The versions a project's `config.policy` (and the older `config.audit`) keeps out of an update before the solver
//! runs, as Composer's pool filters do: versions with a security advisory (on by default), versions on a filter
//! list such as Packagist's malware list (on by default), abandoned packages (opt-in), and versions published within
//! a cooldown period (opt-in). The root and the platform are never filtered; a locked package only for being
//! abandoned and by the filter lists that block installs.

use super::package::{Origin, Package};
use crate::constraint::{self, Constraint};
use crate::version::name_matches;
use anyhow::{Result, bail};
use serde_json::{Map, Value};

/// A security advisory a repository publishes.
#[derive(Debug, Clone)]
pub struct Advisory {
    pub package: String,
    pub id: String,
    pub affected: Constraint,
    pub severity: Option<String>,
    pub cve: Option<String>,
    pub remote_ids: Vec<String>,
}

impl Advisory {
    /// One entry of a repository's `security-advisories` (full or partial).
    pub fn parse(package: &str, data: &Map<String, Value>) -> Result<Advisory> {
        let s = |k: &str| data.get(k).and_then(Value::as_str).map(String::from);
        let affected = s("affectedVersions").unwrap_or_default();
        Ok(Advisory {
            package: package.to_ascii_lowercase(),
            id: s("advisoryId").unwrap_or_default(),
            affected: constraint::parse(&affected)?,
            severity: s("severity"),
            cve: s("cve"),
            remote_ids: (data
                .get("sources")
                .and_then(Value::as_array)
                .into_iter()
                .flatten())
            .filter_map(|src| src.get("remoteId")?.as_str().map(String::from))
            .collect(),
        })
    }
}

/// An entry of a filter list (`filter` in a repository): versions of a package a list names.
#[derive(Debug, Clone)]
pub struct ListEntry {
    pub list: String,
    pub package: String,
    pub constraint: Constraint,
    pub source: Option<String>,
}

impl ListEntry {
    pub fn parse(list: &str, data: &Map<String, Value>) -> Result<ListEntry> {
        let s = |k: &str| data.get(k).and_then(Value::as_str).map(String::from);
        Ok(ListEntry {
            list: list.to_string(),
            package: s("package").unwrap_or_default().to_ascii_lowercase(),
            constraint: constraint::parse(&s("constraint").unwrap_or_else(|| "*".into()))?,
            source: s("source"),
        })
    }
}

/// An ignore rule: a package name pattern and the versions it covers.
#[derive(Debug, Clone)]
struct Ignore {
    pattern: String,
    constraint: Constraint,
    on_block: bool,
}

/// Rules of the form `name`, `name: reason`, `name: {constraint, on-block}` or a list of those.
fn ignore_map(config: Option<&Value>) -> Result<Vec<Ignore>> {
    let mut rules = vec![];
    let rule = |name: &str, v: Option<&Map<String, Value>>| -> Result<Ignore> {
        let constraint = match v.and_then(|m| m.get("constraint")).and_then(Value::as_str) {
            Some(c) => constraint::parse(c)?,
            None => Constraint::Any,
        };
        Ok(Ignore {
            pattern: name.to_string(),
            constraint,
            on_block: v
                .and_then(|m| m.get("on-block"))
                .and_then(Value::as_bool)
                .unwrap_or(true),
        })
    };
    match config {
        None => {}
        Some(Value::Array(items)) => {
            for item in items.iter().filter_map(Value::as_str) {
                rules.push(rule(item, None)?);
            }
        }
        Some(Value::Object(map)) => {
            for (name, v) in map {
                match v {
                    // An empty list decodes to the same empty PHP array as `{}`: one rule for every version.
                    Value::Null | Value::String(_) => rules.push(rule(name, None)?),
                    Value::Array(list) if list.is_empty() => rules.push(rule(name, None)?),
                    Value::Object(o) => rules.push(rule(name, Some(o))?),
                    Value::Array(list) => {
                        for o in list {
                            // A list decodes to a PHP array too: a rule without settings.
                            if o.is_array() {
                                rules.push(rule(name, None)?);
                                continue;
                            }
                            let Value::Object(o) = o else {
                                bail!(
                                    "Invalid ignore rule for \"{name}\": expected an object, got {}.",
                                    debug_type(o)
                                );
                            };
                            rules.push(rule(name, Some(o))?);
                        }
                    }
                    _ => bail!(
                        "Invalid ignore entry at key \"{name}\": value of type {} is not a supported shape. Expected null, a reason string, a rule object, or a list of rule objects.",
                        debug_type(v)
                    ),
                }
            }
        }
        Some(_) => bail!("Invalid ignore list: expected a list or an object."),
    }
    Ok(rules)
}

/// PHP's `get_debug_type` of a decoded JSON value other than `null`, which every caller accepts.
fn debug_type(v: &Value) -> &'static str {
    if v.is_boolean() {
        "bool"
    } else if v.is_f64() {
        "float"
    } else if v.is_number() {
        "int"
    } else if v.is_string() {
        "string"
    } else {
        "array"
    }
}

/// `apply` of the legacy `audit.ignore` entry `key`: whether it applies when blocking.
fn legacy_on_block(key: &str, v: &Value) -> Result<bool> {
    match v.get("apply") {
        None | Some(Value::Null) => Ok(true),
        Some(Value::String(s)) if s == "block" || s == "all" => Ok(true),
        Some(Value::String(s)) if s == "audit" => Ok(false),
        Some(other) => {
            let shown = other.as_str().unwrap_or_else(|| debug_type(other));
            bail!(
                "Invalid 'apply' value for '{key}': {shown}. Expected 'audit', 'block', or 'all'."
            )
        }
    }
}

/// Names (`config.audit.ignore` style): a list of names, or a map from name to a reason or `{apply, reason}`.
fn legacy_names(config: Option<&Value>) -> Result<Vec<String>> {
    let mut out = vec![];
    match config {
        Some(Value::Array(items)) => {
            out.extend(items.iter().filter_map(Value::as_str).map(String::from));
        }
        Some(Value::Object(map)) => {
            for (k, v) in map {
                if legacy_on_block(k, v)? {
                    out.push(k.clone());
                }
            }
        }
        _ => {}
    }
    Ok(out)
}

#[derive(Debug, Clone)]
struct List {
    name: String,
    block: bool,
    /// `all`, `update` or `install`.
    scope: String,
    ignore: Vec<Ignore>,
    ignore_source: Vec<String>,
}

/// The parts of `config.policy` that remove versions before solving.
#[derive(Debug, Clone, Default)]
pub struct Filters {
    advisories: bool,
    /// Advisory ids, CVEs, remote ids and package names to ignore.
    ignore_advisories: Vec<String>,
    ignore_severities: Vec<String>,
    lists: Vec<List>,
    abandoned: bool,
    ignore_abandoned: Vec<String>,
    /// Seconds.
    cooldown: Option<i64>,
    ignore_cooldown: Vec<Ignore>,
}

/// `policy.cooldown.period`: seconds, or `7 days`, `24 hours`, `1 week`.
fn duration(v: Option<&Value>) -> Result<Option<i64>> {
    let text = match v {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        Some(_) => bail!("Invalid policy.cooldown.period format."),
    };
    if text.is_empty() || text == "0" {
        return Ok(None);
    }
    if let Ok(n) = text.parse::<i64>() {
        if n < 0 {
            bail!("Invalid policy.cooldown.period: duration cannot be negative ({text}).");
        }
        return Ok(Some(n));
    }
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    let unit = text[digits.len()..].trim().trim_end_matches('s');
    let seconds = match unit {
        "second" => 1,
        "minute" => 60,
        "hour" => 3600,
        "day" => 86400,
        "week" => 604800,
        _ => 0,
    };
    if digits.is_empty() || seconds == 0 {
        bail!(
            "Invalid policy.cooldown.period format: {text}. Use an integer number of seconds or a duration like '7 days', '24 hours', '30 minutes' or '1 week'."
        );
    }
    Ok(Some(digits.parse::<i64>()? * seconds))
}

static EMPTY: std::sync::LazyLock<Map<String, Value>> = std::sync::LazyLock::new(Map::new);

/// A policy section's settings: `false` turns it off, anything but an object means the defaults.
fn enabled(v: Option<&Value>) -> Option<&Map<String, Value>> {
    match v {
        Some(Value::Bool(false)) => None,
        Some(Value::Object(m)) => Some(m),
        _ => Some(&EMPTY),
    }
}

/// The environment `Filters::from_config` reads, by variable name.
pub type Env<'a> = &'a dyn Fn(&str) -> Option<String>;

/// `Platform::getBoolEnv`: unset or empty is `None`.
fn env_bool(env: Env, name: &str) -> Result<Option<bool>> {
    match env(name).as_deref() {
        None | Some("") => Ok(None),
        Some("1" | "true" | "on") => Ok(Some(true)),
        Some("0" | "false" | "off") => Ok(Some(false)),
        Some(v) => {
            bail!("Invalid value for {name}: {v}. Expected 0, 1, false, true, off, or on.")
        }
    }
}

impl Filters {
    /// From the manifest's `config` (`policy` and `audit`) and the `COMPOSER_POLICY*` variables in `env`.
    pub fn from_config(config: Option<&Map<String, Value>>, env: Env) -> Result<Filters> {
        let get = |k: &str| config.and_then(|c| c.get(k));
        let policy = match env_bool(env, "COMPOSER_POLICY")? {
            Some(false) => return Ok(Filters::default()),
            Some(true) if get("policy") == Some(&Value::Bool(false)) => None,
            _ => match get("policy") {
                Some(Value::Bool(false)) => return Ok(Filters::default()),
                Some(Value::Object(m)) => Some(m),
                _ => None,
            },
        };
        let empty = Map::new();
        let policy = policy.unwrap_or(&empty);
        let audit = get("audit").and_then(Value::as_object);
        let mut f = Filters::default();
        if !policy.contains_key("advisories")
            && let Some(audit) = audit.filter(|a| !a.is_empty())
        {
            f.advisories = audit
                .get("block-insecure")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            f.ignore_advisories = legacy_names(audit.get("ignore"))?;
            f.ignore_severities = legacy_names(audit.get("ignore-severity"))?;
        } else if let Some(a) = enabled(policy.get("advisories")) {
            f.advisories = a.get("block").and_then(Value::as_bool).unwrap_or(true);
            f.ignore_advisories = (ignore_map(a.get("ignore"))?.into_iter())
                .filter(|r| r.on_block)
                .map(|r| r.pattern)
                .collect();
            f.ignore_advisories
                .extend(legacy_names(a.get("ignore-id"))?);
            f.ignore_severities = legacy_names(a.get("ignore-severity"))?;
        }
        if let Some(v) = env_bool(env, "COMPOSER_POLICY_ADVISORIES_BLOCK")? {
            f.advisories = v;
        }
        if let Some(m) = enabled(policy.get("malware")) {
            f.lists.push(List {
                name: "malware".into(),
                block: env_bool(env, "COMPOSER_POLICY_MALWARE_BLOCK")?
                    .unwrap_or_else(|| m.get("block").and_then(Value::as_bool).unwrap_or(true)),
                scope: m
                    .get("block-scope")
                    .and_then(Value::as_str)
                    .unwrap_or("all")
                    .into(),
                ignore: ignore_map(m.get("ignore"))?,
                ignore_source: (m
                    .get("ignore-source")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten())
                .filter_map(Value::as_str)
                .map(String::from)
                .collect(),
            });
        }
        for (name, v) in policy {
            if matches!(
                name.as_str(),
                "advisories" | "malware" | "abandoned" | "cooldown" | "ignore-unreachable"
            ) {
                continue;
            }
            let Some(m) = (match v {
                Value::Bool(true) => Some(&empty),
                Value::Object(m) => Some(m),
                _ => None,
            }) else {
                continue;
            };
            let scope = m
                .get("block-scope")
                .and_then(Value::as_str)
                .unwrap_or("update");
            if !["update", "install", "all"].contains(&scope) {
                bail!(
                    "Invalid block-scope for policy list \"{name}\": expected one of \"update\", \"install\", \"all\", got \"{scope}\"."
                );
            }
            f.lists.push(List {
                name: name.clone(),
                block: m.get("block").and_then(Value::as_bool).unwrap_or(true),
                scope: scope.into(),
                ignore: ignore_map(m.get("ignore"))?,
                ignore_source: vec![],
            });
        }
        if !policy.contains_key("abandoned")
            && let Some(audit) = audit.filter(|a| !a.is_empty())
        {
            f.abandoned = audit
                .get("block-abandoned")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            f.ignore_abandoned = legacy_names(audit.get("ignore-abandoned"))?;
        } else if let Some(a) = enabled(policy.get("abandoned")) {
            f.abandoned = a.get("block").and_then(Value::as_bool).unwrap_or(false);
            f.ignore_abandoned = (ignore_map(a.get("ignore"))?.into_iter())
                .filter(|r| r.on_block)
                .map(|r| r.pattern)
                .collect();
        }
        let abandoned = match env_bool(env, "COMPOSER_POLICY_ABANDONED_BLOCK")? {
            None => env_bool(env, "COMPOSER_SECURITY_BLOCKING_ABANDONED")?,
            set => set,
        };
        if let Some(v) = abandoned {
            f.abandoned = v;
        }
        let cooldown = policy.get("cooldown");
        let env_period = env("COMPOSER_POLICY_COOLDOWN_PERIOD").filter(|v| !v.is_empty());
        if cooldown != Some(&Value::Bool(false)) || env_period.is_some() {
            let c = cooldown.and_then(Value::as_object);
            let mut period = duration(c.and_then(|c| c.get("period")))?;
            if let Some(p) = &env_period {
                period = duration(Some(&Value::String(p.clone()))).map_err(|_| {
                    anyhow::anyhow!(
                        "Invalid value for COMPOSER_POLICY_COOLDOWN_PERIOD: {p}. Use formats like '7 days', '24 hours', or an integer number of seconds."
                    )
                })?;
            }
            let block = env_bool(env, "COMPOSER_POLICY_COOLDOWN_BLOCK")?.unwrap_or_else(|| {
                c.and_then(|c| c.get("block"))
                    .and_then(Value::as_bool)
                    .unwrap_or(true)
            });
            f.cooldown = period.filter(|p| *p > 0 && block);
            f.ignore_cooldown = ignore_map(c.and_then(|c| c.get("ignore")))?;
        }
        Ok(f)
    }

    /// Whether any filter needs the repositories' advisories.
    pub fn wants_advisories(&self) -> bool {
        self.advisories
    }

    /// Whether any filter list blocks in an update.
    pub fn wants_lists(&self) -> bool {
        self.lists.iter().any(|l| l.block && l.scope != "install")
    }

    fn advisory_ignored(&self, a: &Advisory) -> bool {
        let hit = |k: &str| self.ignore_advisories.iter().any(|i| i == k);
        hit(&a.package)
            || hit(&a.id)
            || a.severity
                .as_deref()
                .is_some_and(|s| self.ignore_severities.iter().any(|i| i == s))
            || a.cve.as_deref().is_some_and(hit)
            || a.remote_ids.iter().any(|r| hit(r))
    }

    /// Why a package is kept out of the pool, or `None` to keep it. `locked_equivalent` is a version the lock
    /// already has, which only install-scope lists block; a `locked` package is checked against the lists only.
    #[expect(
        clippy::too_many_arguments,
        reason = "the package and everything it is checked against"
    )]
    pub fn removal(
        &self,
        p: &Package,
        base: &Package,
        advisories: &[Advisory],
        entries: &[ListEntry],
        locked_equivalent: bool,
        locked: bool,
        now: i64,
    ) -> Option<String> {
        if matches!(p.origin, Origin::Root | Origin::Platform)
            || crate::version::is_platform(&p.name)
        {
            return None;
        }
        let exact = Constraint::exact(&p.version);
        // Composer's SecurityAdvisoryPoolFilter blocks abandoned packages, locked ones and aliases too, and only
        // while it blocks advisories.
        if self.advisories
            && self.abandoned
            && p.abandoned
            && !self
                .ignore_abandoned
                .iter()
                .any(|i| name_matches(i, &p.name))
        {
            return Some("abandoned".into());
        }
        if self.advisories && !locked {
            let target = if p.root_alias { base } else { p };
            let unversioned = target.version.starts_with("dev-") || target.version == "9999999-dev";
            if !unversioned {
                let exact = Constraint::exact(&target.version);
                let hit = (advisories.iter())
                    .filter(|a| target.names(false).contains(&a.package.as_str()))
                    .find(|a| !self.advisory_ignored(a) && a.affected.matches(&exact));
                if let Some(a) = hit {
                    return Some(format!("advisory {}", a.id));
                }
            }
        }
        for list in &self.lists {
            let scope_ok = list.scope == "all"
                || list.scope
                    == if locked_equivalent {
                        "install"
                    } else {
                        "update"
                    };
            if !list.block || !scope_ok {
                continue;
            }
            for name in p.names(false) {
                let ignored = list.ignore.iter().any(|r| {
                    r.on_block && name_matches(&r.pattern, name) && r.constraint.matches(&exact)
                });
                if ignored {
                    continue;
                }
                let hit = entries.iter().any(|e| {
                    e.list == list.name
                        && e.package == name
                        && !e
                            .source
                            .as_ref()
                            .is_some_and(|s| list.ignore_source.contains(s))
                        && e.constraint.matches(&exact)
                });
                if hit {
                    return Some(format!("on the {} list", list.name));
                }
            }
        }
        if let Some(period) = self.cooldown
            && !locked
            && !base.is_dev()
            && !(self.ignore_cooldown.iter()).any(|r| {
                r.on_block
                    && name_matches(&r.pattern, &base.name)
                    && r.constraint.matches(&Constraint::exact(&base.version))
            })
        {
            let data = base.data();
            let date = ["published-time", "time"]
                .iter()
                .find_map(|k| data.get(*k).and_then(Value::as_str).and_then(timestamp));
            if date.is_some_and(|d| d > now - period) {
                return Some("cooldown".into());
            }
        }
        None
    }
}

/// Seconds since the epoch of `2024-01-31T12:00:00+02:00`, `2024-01-31 12:00:00` (UTC) or `2024-01-31`.
pub fn timestamp(s: &str) -> Option<i64> {
    let s = s.trim();
    let num = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    if s.as_bytes().get(4) != Some(&b'-') || s.as_bytes().get(7) != Some(&b'-') {
        return None;
    }
    let (mut hh, mut mm, mut ss, mut offset) = (0, 0, 0, 0);
    if s.len() >= 19 {
        hh = num(11, 13)?;
        mm = num(14, 16)?;
        ss = num(17, 19)?;
        let rest = s[19..].trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
        if let Some(sign) = rest.chars().next().filter(|c| *c == '+' || *c == '-') {
            let tz = rest[1..].replace(':', "");
            let h = tz.get(0..2)?.parse::<i64>().ok()?;
            let mi = tz
                .get(2..4)
                .and_then(|x| x.parse::<i64>().ok())
                .unwrap_or(0);
            offset = (h * 3600 + mi * 60) * if sign == '+' { 1 } else { -1 };
        }
    }
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hh * 3600 + mm * 60 + ss - offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Expected values from PHP 8.5's `(new DateTime($s))->getTimestamp()` with the UTC timezone.
    #[test]
    fn timestamps_match_php() {
        for (s, want) in [
            ("2020-01-01T00:00:00+00:00", 1577836800),
            ("2999-01-01T00:00:00+00:00", 32472144000),
            ("2024-02-29 13:14:15", 1709212455),
            ("2024-03-01T01:00:00+02:00", 1709247600),
            ("1999-12-31", 946598400),
            ("2024-07-01T00:00:00.123-05:30", 1719811800),
        ] {
            assert_eq!(timestamp(s), Some(want), "{s}");
        }
        assert_eq!(timestamp("yesterday"), None);
    }

    #[test]
    fn durations_parse_as_composer() {
        let d = |s: &str| duration(Some(&Value::String(s.into())));
        assert_eq!(d("1 week").unwrap(), Some(604800));
        assert_eq!(d("3days").unwrap(), Some(259200));
        assert_eq!(d("90").unwrap(), Some(90));
        assert_eq!(d("0").unwrap(), None);
        assert!(d("1 fortnight").is_err());
        assert!(d("-5").is_err());
        // Derived by hand: a period that is neither a number nor a string.
        assert_eq!(
            duration(Some(&Value::Bool(true))).unwrap_err().to_string(),
            "Invalid policy.cooldown.period format."
        );
        assert_eq!(timestamp("2024x01x01"), None);
    }

    fn parse(env: &[(&str, &str)], config: Value) -> Result<Filters, String> {
        let env = |k: &str| {
            (env.iter())
                .find(|(n, _)| *n == k)
                .map(|(_, v)| v.to_string())
        };
        Filters::from_config(config.as_object(), &env).map_err(|e| e.to_string())
    }

    /// The parts of [`Filters`] Composer's `PolicyConfig` has: advisories blocking and the names, ids and CVEs
    /// ignored when blocking, the same for abandoned, whether malware blocks, the cooldown period that blocks, and
    /// the custom lists with their block flag and scope.
    fn summary(f: &Filters) -> Value {
        let malware = f.lists.iter().find(|l| l.name == "malware");
        let lists: Vec<Value> = (f.lists.iter())
            .filter(|l| l.name != "malware")
            .map(|l| serde_json::json!([l.name, l.block, l.scope]))
            .collect();
        serde_json::json!({
            "advisories": f.advisories,
            "ignore": f.ignore_advisories,
            "abandoned": f.abandoned,
            "ignore-abandoned": f.ignore_abandoned,
            "malware": malware.is_some_and(|m| m.block),
            "cooldown": f.cooldown,
            "lists": lists,
        })
    }

    /// Expected values from Composer 2.11.2's `PolicyConfig::fromConfig` on the same config and environment, in
    /// PHP 8.5; where PHP fails with a type error instead (an ignore list that is a number), Pun's message is its own.
    #[test]
    fn reads_policy_as_composer() {
        use serde_json::json;
        let base = json!({"advisories": true, "ignore": [], "abandoned": false, "ignore-abandoned": [], "malware": true, "cooldown": null, "lists": []});
        let with = |changes: Value| {
            let mut v = base.clone();
            (v.as_object_mut().unwrap()).extend(changes.as_object().unwrap().clone());
            v
        };
        let off = json!({"advisories": false, "ignore": [], "abandoned": false, "ignore-abandoned": [], "malware": false, "cooldown": null, "lists": []});
        for (env, config, want) in [
            (
                vec![],
                json!({"policy": {"advisories": {"ignore": ["acme/a", "acme/b"]}}}),
                with(json!({"ignore": ["acme/a", "acme/b"]})),
            ),
            (
                vec![],
                json!({"policy": {"advisories": {"ignore": {"acme/a": null, "acme/b": "why", "acme/c": [{"constraint": "^1.0"}, {"on-block": false}], "acme/d": {"constraint": "^2"}}}}}),
                with(json!({"ignore": ["acme/a", "acme/b", "acme/c", "acme/d"]})),
            ),
            (
                vec![],
                json!({"policy": {"advisories": {"ignore": {"acme/e": []}}}}),
                with(json!({"ignore": ["acme/e"]})),
            ),
            (
                vec![],
                json!({"policy": {"advisories": {"ignore": {"acme/c": [[1]]}}}}),
                with(json!({"ignore": ["acme/c"]})),
            ),
            (
                vec![],
                json!({"policy": {"abandoned": false}}),
                base.clone(),
            ),
            (
                vec![],
                json!({"audit": {"ignore": {"CVE-1": {"apply": "audit"}, "CVE-2": {"apply": "block"}, "CVE-3": {}, "CVE-4": "reason"}}}),
                with(json!({"ignore": ["CVE-2", "CVE-3", "CVE-4"]})),
            ),
            (
                vec![],
                json!({"policy": {"cooldown": {"period": 3600}}}),
                with(json!({"cooldown": 3600})),
            ),
            (
                vec![],
                json!({"policy": {"cooldown": {"period": "30 seconds"}}}),
                with(json!({"cooldown": 30})),
            ),
            (
                vec![],
                json!({"policy": {"cooldown": {"period": "2 minutes"}}}),
                with(json!({"cooldown": 120})),
            ),
            (
                vec![],
                json!({"policy": {"cooldown": {"period": "5 hours"}}}),
                with(json!({"cooldown": 18000})),
            ),
            (
                vec![],
                json!({"policy": {"custom": 5, "other": true}}),
                with(json!({"lists": [["other", true, "update"]]})),
            ),
            (
                vec![],
                json!({"policy": {"abandoned": {"block": true, "ignore": {"acme/a": {"on-block": false}, "acme/b": null}}}}),
                with(json!({"abandoned": true, "ignore-abandoned": ["acme/b"]})),
            ),
            (vec![], json!({"policy": false}), off.clone()),
            (
                vec![],
                json!({"policy": {"malware": false, "cooldown": false}}),
                with(json!({"malware": false})),
            ),
            (vec![("COMPOSER_POLICY", "0")], json!({}), off),
            (
                vec![("COMPOSER_POLICY", "1")],
                json!({"policy": false}),
                base.clone(),
            ),
            (vec![("COMPOSER_POLICY", "")], json!({}), base.clone()),
            (
                vec![("COMPOSER_POLICY_ADVISORIES_BLOCK", "off")],
                json!({}),
                with(json!({"advisories": false})),
            ),
            (
                vec![("COMPOSER_POLICY_ABANDONED_BLOCK", "on")],
                json!({}),
                with(json!({"abandoned": true})),
            ),
            (
                vec![("COMPOSER_SECURITY_BLOCKING_ABANDONED", "1")],
                json!({}),
                with(json!({"abandoned": true})),
            ),
            (
                vec![("COMPOSER_POLICY_COOLDOWN_PERIOD", "2 days")],
                json!({"policy": {"cooldown": false}}),
                with(json!({"cooldown": 172800})),
            ),
            (
                vec![("COMPOSER_POLICY_COOLDOWN_BLOCK", "false")],
                json!({"policy": {"cooldown": {"period": 60}}}),
                base.clone(),
            ),
            (
                vec![("COMPOSER_POLICY_MALWARE_BLOCK", "0")],
                json!({}),
                with(json!({"malware": false})),
            ),
        ] {
            assert_eq!(
                summary(&parse(&env, config.clone()).unwrap()),
                want,
                "{env:?} {config}"
            );
        }
        for (env, config, want) in [
            (
                vec![],
                json!({"policy": {"advisories": {"ignore": {"acme/c": [5]}}}}),
                "Invalid ignore rule for \"acme/c\": expected an object, got int.",
            ),
            (
                vec![],
                json!({"policy": {"advisories": {"ignore": {"acme/c": 5}}}}),
                "Invalid ignore entry at key \"acme/c\": value of type int is not a supported shape. Expected null, a reason string, a rule object, or a list of rule objects.",
            ),
            (
                vec![],
                json!({"policy": {"advisories": {"ignore": {"acme/c": true}}}}),
                "Invalid ignore entry at key \"acme/c\": value of type bool is not a supported shape. Expected null, a reason string, a rule object, or a list of rule objects.",
            ),
            (
                vec![],
                json!({"policy": {"advisories": {"ignore": 5}}}),
                "Invalid ignore list: expected a list or an object.",
            ),
            (
                vec![],
                json!({"policy": {"advisories": {"ignore": {"acme/c": ["x"]}}}}),
                "Invalid ignore rule for \"acme/c\": expected an object, got string.",
            ),
            (
                vec![],
                json!({"policy": {"advisories": {"ignore": {"acme/c": 1.5}}}}),
                "Invalid ignore entry at key \"acme/c\": value of type float is not a supported shape. Expected null, a reason string, a rule object, or a list of rule objects.",
            ),
            (
                vec![],
                json!({"audit": {"ignore": {"CVE-1": {"apply": ["x"]}}}}),
                "Invalid 'apply' value for 'CVE-1': array. Expected 'audit', 'block', or 'all'.",
            ),
            (
                vec![],
                json!({"audit": {"ignore": {"CVE-1": {"apply": "never"}}}}),
                "Invalid 'apply' value for 'CVE-1': never. Expected 'audit', 'block', or 'all'.",
            ),
            (
                vec![],
                json!({"audit": {"ignore": {"CVE-1": {"apply": 5}}}}),
                "Invalid 'apply' value for 'CVE-1': int. Expected 'audit', 'block', or 'all'.",
            ),
            (
                vec![],
                json!({"policy": {"custom": {"block-scope": "sometimes"}}}),
                "Invalid block-scope for policy list \"custom\": expected one of \"update\", \"install\", \"all\", got \"sometimes\".",
            ),
            (
                vec![("COMPOSER_POLICY", "yes")],
                json!({}),
                "Invalid value for COMPOSER_POLICY: yes. Expected 0, 1, false, true, off, or on.",
            ),
            (
                vec![("COMPOSER_POLICY_COOLDOWN_PERIOD", "soon")],
                json!({}),
                "Invalid value for COMPOSER_POLICY_COOLDOWN_PERIOD: soon. Use formats like '7 days', '24 hours', or an integer number of seconds.",
            ),
        ] {
            assert_eq!(
                parse(&env, config.clone()).unwrap_err(),
                want,
                "{env:?} {config}"
            );
        }
    }

    fn package(data: Value) -> Package {
        let mut arena = vec![];
        let data = data.as_object().unwrap().clone();
        super::super::package::load(data, Origin::Repository(1), &mut arena).unwrap();
        arena.pop().unwrap()
    }

    /// Derived by hand from Composer's pool filters: an ignored abandoned package stays; a list that blocks only
    /// updates keeps the version the lock already has; a cooldown ignore rule covers only the versions its
    /// constraint matches.
    #[test]
    fn removes_as_composer() {
        use serde_json::json;
        // 2026-11-14T00:00:00Z.
        let now = 1_794_614_400;
        let removal = |f: &Filters, p: &Package, entries: &[ListEntry], equivalent: bool| {
            f.removal(p, p, &[], entries, equivalent, false, now)
        };
        let f = parse(
            &[],
            json!({"policy": {"abandoned": {"block": true, "ignore": ["acme/old"]}}}),
        )
        .unwrap();
        let old = package(json!({"name": "acme/old", "version": "1.0.0", "abandoned": true}));
        let gone =
            package(json!({"name": "acme/gone", "version": "1.0.0", "abandoned": "acme/new"}));
        assert_eq!(removal(&f, &old, &[], false), None);
        assert_eq!(removal(&f, &gone, &[], false).as_deref(), Some("abandoned"));

        let f = parse(
            &[],
            json!({"policy": {"vetted": {"block-scope": "update"}}}),
        )
        .unwrap();
        let entry = ListEntry::parse(
            "vetted",
            json!({"package": "acme/old", "constraint": "*"})
                .as_object()
                .unwrap(),
        )
        .unwrap();
        let entries = [entry];
        assert_eq!(removal(&f, &old, &entries, true), None);
        assert_eq!(
            removal(&f, &old, &entries, false).as_deref(),
            Some("on the vetted list")
        );

        let config = json!({"policy": {"cooldown": {"period": "7 days", "ignore": {"acme/a": {"constraint": "^1.1"}}}}});
        let f = parse(&[], config).unwrap();
        let fresh = |name: &str, version: &str| {
            package(json!({"name": name, "version": version, "time": "2026-11-13 00:00:00"}))
        };
        assert_eq!(removal(&f, &fresh("acme/a", "1.1.0"), &[], false), None);
        assert_eq!(
            removal(&f, &fresh("acme/a", "2.0.0"), &[], false).as_deref(),
            Some("cooldown")
        );
        assert_eq!(
            removal(&f, &fresh("acme/b", "1.1.0"), &[], false).as_deref(),
            Some("cooldown")
        );
    }
}
