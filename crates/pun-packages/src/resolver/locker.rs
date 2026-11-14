use super::package::Package;
use super::platform::PLUGIN_API;
use super::{Options, Resolution};
use crate::error::Result;
use crate::lock::{Lock, content_hash};
use crate::version::is_platform;
use pcre2::bytes::Regex;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::sync::LazyLock;

pub fn lock(resolution: &Resolution, manifest: &Map<String, Value>, options: &Options) -> Lock {
    let dump_all = |list: &[usize]| -> Vec<Map<String, Value>> {
        let mut out: Vec<Map<String, Value>> =
            list.iter().map(|&p| dump(&resolution.arena[p])).collect();
        let s = |m: &Map<String, Value>, k: &str| m[k].as_str().unwrap_or_default().to_string();
        out.sort_by_key(|a| s(a, "name"));
        out
    };
    let aliases: Vec<Value> = (resolution.aliases.iter())
        .map(|a| {
            let mut a = a.clone();
            if let Some(v) = a.get("version").and_then(Value::as_str)
                && ["dev-master", "dev-trunk", "dev-default"].contains(&v)
            {
                a.insert("version".into(), "9999999-dev".into());
            }
            Value::Object(a)
        })
        .collect();
    let minimum = manifest
        .get("minimum-stability")
        .and_then(Value::as_str)
        .map_or("stable".to_string(), |s| {
            let s = s.to_ascii_lowercase();
            if s == "rc" { "RC".into() } else { s }
        });
    let mut flags: Vec<(&String, &u8)> = resolution.flags.iter().collect();
    flags.sort_by(|a, b| php_cmp(a.0, b.0));
    let platform = |kind: &str| -> Map<String, Value> {
        (manifest
            .get(kind)
            .and_then(Value::as_object)
            .into_iter()
            .flatten())
        .filter_map(|(k, v)| Some((k.to_ascii_lowercase(), v.as_str()?)))
        .filter(|(k, _)| is_platform(k))
        .map(|(k, v)| (k, v.into()))
        .collect()
    };
    let mut settings = Map::new();
    settings.insert("aliases".into(), Value::Array(aliases));
    settings.insert("minimum-stability".into(), minimum.into());
    let flags = flags.into_iter().map(|(k, v)| (k.clone(), (*v).into()));
    settings.insert("stability-flags".into(), Value::Object(flags.collect()));
    let prefer_stable =
        options.prefer_stable || manifest.get("prefer-stable") == Some(&Value::Bool(true));
    settings.insert("prefer-stable".into(), prefer_stable.into());
    settings.insert("prefer-lowest".into(), options.prefer_lowest.into());
    settings.insert("platform".into(), Value::Object(platform("require")));
    settings.insert(
        "platform-dev".into(),
        Value::Object(platform("require-dev")),
    );
    if let Some(Value::Object(o)) = manifest.get("config").and_then(|c| c.get("platform"))
        && !o.is_empty()
    {
        settings.insert("platform-overrides".into(), Value::Object(o.clone()));
    }
    settings.insert("plugin-api-version".into(), PLUGIN_API.into());
    Lock {
        content_hash: content_hash(manifest),
        settings,
        packages: dump_all(&resolution.packages),
        packages_dev: dump_all(&resolution.packages_dev),
    }
}

pub fn dump(package: &Package) -> Map<String, Value> {
    let data = &package.data();
    let get = |k: &str| data.get(k).filter(|v| !v.is_null());
    let empty = |v: &Value| match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::String(s) => s.is_empty() || s == "0",
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.is_empty(),
        Value::Number(n) => n.as_f64() == Some(0.0),
    };
    let present = |k: &str| get(k).filter(|v| !empty(v));
    let mut out = Map::new();
    out.insert("name".into(), package.pretty_name.clone().into());
    out.insert("version".into(), package.pretty_version.clone().into());
    if let Some(v) = get("target-dir") {
        out.insert("target-dir".into(), v.clone());
    }
    for kind in ["source", "dist"] {
        let Some(Value::Object(s)) = get(kind) else {
            continue;
        };
        let mut o = Map::new();
        o.insert(
            "type".into(),
            s.get("type").cloned().unwrap_or_else(|| "".into()),
        );
        let url = s.get("url").filter(|u| !u.is_null()).cloned();
        o.insert("url".into(), url.unwrap_or_else(|| "".into()));
        if let Some(r) = s.get("reference").filter(|r| !r.is_null()) {
            o.insert("reference".into(), stringify(r).into());
        }
        if kind == "dist"
            && let Some(sum) = s.get("shasum").filter(|r| !r.is_null())
        {
            o.insert("shasum".into(), sum.clone());
        }
        if let Some(m) = s.get("mirrors").filter(|m| !empty(m)) {
            o.insert("mirrors".into(), m.clone());
        }
        out.insert(kind.into(), Value::Object(o));
    }
    let name = package.name.as_str();
    for kind in ["require", "conflict", "provide", "replace", "require-dev"] {
        let Some(Value::Object(links)) = get(kind) else {
            continue;
        };
        let mut links: Vec<(String, Value)> = (links.iter())
            .filter(|(_, c)| c.is_string())
            .map(|(t, c)| (t.to_ascii_lowercase(), c.clone()))
            .filter(|(t, _)| t != name)
            .collect();
        if links.is_empty() {
            continue;
        }
        let mut unique: Vec<(String, Value)> = vec![];
        for (t, c) in links.drain(..) {
            match unique.iter_mut().find(|(u, _)| *u == t) {
                Some(slot) => slot.1 = c,
                None => unique.push((t, c)),
            }
        }
        unique.sort_by(|a, b| php_cmp(&a.0, &b.0));
        out.insert(kind.into(), Value::Object(unique.into_iter().collect()));
    }
    if let Some(Value::Object(s)) = get("suggest")
        && !s.is_empty()
    {
        let mut s: Vec<(String, Value)> = (s.iter())
            .map(|(k, v)| {
                let v = match v.as_str() {
                    Some(r) if r.trim() == "self.version" => package.pretty_version.clone().into(),
                    _ => v.clone(),
                };
                (k.clone(), v)
            })
            .collect();
        s.sort_by(|a, b| php_cmp(&a.0, &b.0));
        out.insert("suggest".into(), Value::Object(s.into_iter().collect()));
    }
    let time = present("time").and_then(rfc3339);
    let published = get("published-time")
        .filter(|v| v.is_i64() || v.as_str().is_some_and(|s| !s.is_empty()))
        .and_then(rfc3339);
    if get("default-branch") == Some(&Value::Bool(true)) {
        out.insert("default-branch".into(), true.into());
    }
    if let Some(bin) = get("bin") {
        let list = match bin {
            Value::Array(a) => a.clone(),
            other => vec![other.clone()],
        };
        let list: Vec<Value> = (list.into_iter())
            .map(|b| stringify(&b).trim_start_matches('/').to_string().into())
            .collect();
        if !list.is_empty() {
            out.insert("bin".into(), Value::Array(list));
        }
    }
    let kind = get("type").map_or("library".to_string(), |t| stringify(t).to_ascii_lowercase());
    out.insert("type".into(), kind.into());
    if let Some(e) = get("extra").filter(|e| (e.is_object() || e.is_array()) && !empty(e)) {
        out.insert("extra".into(), e.clone());
    }
    for key in ["autoload", "autoload-dev"] {
        if let Some(v) = get(key).filter(|v| !v.is_array() && !v.is_object() || !empty(v)) {
            out.insert(key.into(), v.clone());
        }
    }
    if let Some(v) = present("notification-url") {
        out.insert("notification-url".into(), v.clone());
    }
    for key in ["include-path", "php-ext"] {
        if let Some(v) = get(key).filter(|v| !v.is_array() && !v.is_object() || !empty(v)) {
            out.insert(key.into(), v.clone());
        }
    }
    let mut archive = Map::new();
    for key in ["name", "exclude"] {
        if let Some(v) = (get("archive").and_then(|a| a.get(key))).filter(|v| !empty(v)) {
            archive.insert(key.into(), v.clone());
        }
    }
    if !archive.is_empty() {
        out.insert("archive".into(), Value::Object(archive));
    }
    if let Some(Value::Object(scripts)) = get("scripts")
        && !scripts.is_empty()
    {
        let scripts: Map<String, Value> = (scripts.iter())
            .map(|(event, l)| {
                let l = match l {
                    Value::Array(_) | Value::Object(_) => l.clone(),
                    Value::Null => Value::Array(vec![]),
                    other => Value::Array(vec![other.clone()]),
                };
                (event.clone(), l)
            })
            .collect();
        out.insert("scripts".into(), Value::Object(scripts));
    }
    if let Some(l) = present("license") {
        let l = match l {
            Value::Array(_) | Value::Object(_) => l.clone(),
            other => Value::Array(vec![other.clone()]),
        };
        out.insert("license".into(), l);
    }
    if let Some(a) = present("authors").filter(|a| a.is_array() || a.is_object()) {
        out.insert("authors".into(), a.clone());
    }
    for key in ["description", "homepage"] {
        if let Some(v) = present(key).filter(|v| v.is_string()) {
            out.insert(key.into(), v.clone());
        }
    }
    if let Some(k) = present("keywords") {
        let mut words: Vec<String> = match k {
            Value::Array(a) => a.iter().map(stringify).collect(),
            Value::Object(o) => o.values().map(stringify).collect(),
            _ => vec![],
        };
        if !words.is_empty() {
            words.sort_by(|a, b| php_cmp(a, b));
            out.insert(
                "keywords".into(),
                words.into_iter().map(Value::from).collect(),
            );
        }
    }
    if let Some(s) = get("support").filter(|s| (s.is_object() || s.is_array()) && !empty(s)) {
        out.insert("support".into(), s.clone());
    }
    if let Some(f) = present("funding").filter(|f| f.is_array() || f.is_object()) {
        out.insert("funding".into(), f.clone());
    }
    match get("abandoned") {
        None | Some(Value::Bool(false)) => {}
        Some(Value::String(s)) if !s.is_empty() => {
            out.insert("abandoned".into(), s.clone().into());
        }
        Some(_) => {
            out.insert("abandoned".into(), true.into());
        }
    }
    if let Some(Value::Object(t)) = get("transport-options") {
        let mut t = t.clone();
        t.shift_remove("prevent_url_access_callable");
        t.shift_remove("prevent_ip_access_callable");
        if !t.is_empty() {
            out.insert("transport-options".into(), Value::Object(t));
        }
    }
    if let Some(t) = time {
        out.insert("time".into(), t.into());
    }
    if let Some(t) = published {
        out.insert("published-time".into(), t.into());
    }
    out
}

fn stringify(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "1".into(),
        Value::Bool(false) | Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn php_cmp(a: &str, b: &str) -> Ordering {
    match (numeric(a), numeric(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => a.cmp(b),
    }
}

fn numeric(s: &str) -> Option<f64> {
    static NUMERIC: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^[ \t\n\r\v\f]*[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?[ \t\n\r\v\f]*$")
            .expect("valid pattern")
    });
    NUMERIC
        .is_match(s.as_bytes())
        .unwrap_or(false)
        .then(|| s.trim().parse().ok())
        .flatten()
}

fn rfc3339(v: &Value) -> Option<String> {
    let raw = match v {
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => return None,
    };
    if !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_digit()) {
        let secs: i64 = raw.parse().ok()?;
        return Some(from_unix(secs));
    }
    static DATE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(
            r"^(\d{4})-(\d{2})-(\d{2})(?:[T ](\d{2}):(\d{2})(?::(\d{2})(?:\.\d+)?)?)? *(Z|UTC|GMT|[+-]\d{2}:?\d{2})?$",
        )
        .expect("valid pattern")
    });
    let c = DATE.captures(raw.as_bytes()).ok()??;
    let g = |i: usize| {
        c.get(i)
            .map(|m| String::from_utf8_lossy(m.as_bytes()).into_owned())
    };
    let zone = match g(7) {
        None => "+00:00".to_string(),
        Some(z) if z == "Z" || z == "UTC" || z == "GMT" => "+00:00".to_string(),
        Some(z) => {
            let digits = z.replace(':', "");
            format!("{}:{}", &digits[..3], &digits[3..])
        }
    };
    Some(format!(
        "{}-{}-{}T{}:{}:{}{zone}",
        g(1)?,
        g(2)?,
        g(3)?,
        g(4).unwrap_or_else(|| "00".into()),
        g(5).unwrap_or_else(|| "00".into()),
        g(6).unwrap_or_else(|| "00".into()),
    ))
}

fn from_unix(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}+00:00",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

pub fn update_hash(
    lock: &mut Lock,
    manifest: &Map<String, Value>,
    picked: &[(String, String)],
) -> Result<()> {
    lock.content_hash = content_hash(manifest);
    let minimum = super::package::stability_value(
        manifest
            .get("minimum-stability")
            .and_then(Value::as_str)
            .unwrap_or("stable"),
    );
    let mut flags = indexmap::IndexMap::new();
    super::stability_flags(picked, minimum, &mut flags)?;
    let mut all: Vec<(String, Value)> = (lock
        .settings
        .get("stability-flags")
        .and_then(Value::as_object))
    .into_iter()
    .flatten()
    .map(|(k, v)| (k.clone(), v.clone()))
    .collect();
    for (name, flag) in flags {
        match all.iter_mut().find(|(k, _)| *k == name) {
            Some(slot) => slot.1 = flag.into(),
            None => all.push((name, flag.into())),
        }
    }
    all.sort_by(|a, b| php_cmp(&a.0, &b.0));
    lock.settings.insert(
        "stability-flags".into(),
        Value::Object(all.into_iter().collect()),
    );
    Ok(())
}

pub fn recommended(resolution: &Resolution, name: &str) -> Option<String> {
    let arena = &resolution.arena;
    let package = (resolution.packages.iter().chain(&resolution.packages_dev))
        .map(|&p| &arena[p])
        .find(|p| p.name.eq_ignore_ascii_case(name))?;
    let transform = |version: &str, pretty: &str, stability: &str| -> String {
        let mut parts: Vec<&str> = version.split('.').collect();
        if parts.len() != 4 || !parts[3].starts_with(|c: char| c.is_ascii_digit()) {
            return pretty.to_string();
        }
        parts.truncate(if parts[0] == "0" { 3 } else { 2 });
        let mut v = format!("^{}", parts.join("."));
        if stability != "stable" {
            v.push('@');
            v.push_str(stability);
        }
        v
    };
    if !package.is_dev() {
        let stability = crate::version::stability_of(&package.version);
        return Some(transform(
            &package.version,
            &package.pretty_version,
            stability,
        ));
    }
    let mut data = package.data();
    data.insert("version".into(), package.pretty_version.clone().into());
    if let Some((alias, _)) = crate::version::branch_alias(&data)
        && alias != "9999999-dev"
        && let Some(stem) = alias.strip_suffix(".9999999-dev")
        && stem.split('.').count() == 3
    {
        let extra = format!("{stem}.0").replace(".9999999", ".0");
        return Some(transform(&extra, &extra, "dev"));
    }
    Some(package.pretty_version.clone())
}

#[cfg(test)]
mod tests {
    use super::super::package::{Origin, load};
    use super::super::pool::{ArrayRepository, Repository};
    use super::*;
    use serde_json::json;

    fn object(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn dumps_as_composer() {
        for (data, want) in [
            (
                json!({"name": "acme/empties", "version": "1.0.0", "time": false, "license": [], "authors": {}, "description": "0", "homepage": "", "keywords": "solo", "bin": [], "funding": [], "notification-url": "", "scripts": {"x": null}, "extra": [], "support": {}, "autoload": {}, "php-ext": {"extension-name": "acme"}, "transport-options": {"symlink": false}, "dist": {"type": "zip", "url": "https://d.test/e.zip", "reference": 123}, "source": {"type": "svn", "url": "https://s.test/e", "reference": false, "mirrors": null}, "published-time": 1700000000}),
                r#"{"name":"acme/empties","version":"1.0.0","source":{"type":"svn","url":"https://s.test/e","reference":""},"dist":{"type":"zip","url":"https://d.test/e.zip","reference":"123"},"type":"library","php-ext":{"extension-name":"acme"},"scripts":{"x":[]},"transport-options":{"symlink":false},"published-time":"2023-11-14T22:13:20+00:00"}"#,
            ),
            (
                json!({"name": "acme/zero", "version": "1.0.0", "time": 0, "dist": {"type": "zip", "url": "https://d.test/z.zip", "reference": true}, "keywords": ["b", "a"], "transport-options": {}, "published-time": "2024-01-02 03:04:05"}),
                r#"{"name":"acme/zero","version":"1.0.0","dist":{"type":"zip","url":"https://d.test/z.zip","reference":"1"},"type":"library","keywords":["a","b"],"published-time":"2024-01-02T03:04:05+00:00"}"#,
            ),
            (
                json!({"name": "acme/stamp", "version": "1.0.0", "time": 1709210710, "published-time": "", "dist": {"type": "zip", "url": "https://d.test/s.zip"}}),
                r#"{"name":"acme/stamp","version":"1.0.0","dist":{"type":"zip","url":"https://d.test/s.zip"},"type":"library","time":"2024-02-29T12:45:10+00:00"}"#,
            ),
            (
                json!({"name": "acme/day", "version": "1.0.0", "time": "2023-01-01", "published-time": "2022-06-01T10:20:30.123456-0530"}),
                r#"{"name":"acme/day","version":"1.0.0","type":"library","time":"2023-01-01T00:00:00+00:00","published-time":"2022-06-01T10:20:30-05:30"}"#,
            ),
            (
                json!({"name": "acme/garbage", "version": "1.0.0", "time": "garbage"}),
                r#"{"name":"acme/garbage","version":"1.0.0","type":"library"}"#,
            ),
            (
                json!({"name": "acme/flag", "version": "1.0.0", "time": true}),
                r#"{"name":"acme/flag","version":"1.0.0","type":"library"}"#,
            ),
            (
                json!({"name": "acme/old", "version": "1.0.0", "time": "1969-07-20 20:17:40"}),
                r#"{"name":"acme/old","version":"1.0.0","type":"library","time":"1969-07-20T20:17:40+00:00"}"#,
            ),
        ] {
            let mut arena = vec![];
            load(object(data), Origin::Repository(1), &mut arena).unwrap();
            assert_eq!(serde_json::to_string(&dump(&arena[0])).unwrap(), want);
        }
    }

    fn repository() -> Value {
        json!([
            {"name": "acme/stable", "version": "1.2.3"},
            {"name": "acme/zero", "version": "0.3.4"},
            {"name": "acme/beta", "version": "2.0.0-beta2"},
            {"name": "acme/branch", "version": "dev-main", "extra": {"branch-alias": {"dev-main": "1.0.x-dev"}}},
            {"name": "acme/wide", "version": "dev-next", "extra": {"branch-alias": {"dev-next": "2.x-dev"}}},
            {"name": "acme/default", "version": "dev-trunk", "default-branch": true},
            {"name": "acme/plain", "version": "dev-feature"},
            {"name": "acme/numbered", "version": "1.0.x-dev"},
            {"name": "acme/date", "version": "2024.01"},
            {"name": "acme/dated", "version": "2024-01-01"}
        ])
    }

    #[test]
    fn recommends_as_composer_require() {
        let names = [
            ("acme/stable", "^1.2"),
            ("acme/zero", "^0.3.4"),
            ("acme/beta", "^2.0@beta"),
            ("Acme/Branch", "^1.0@dev"),
            ("acme/wide", "^2.0@dev"),
            ("acme/default", "dev-trunk"),
            ("acme/plain", "dev-feature"),
            ("acme/numbered", "1.0.x-dev"),
            ("acme/date", "^2024.01"),
            ("acme/dated", "2024-01-01"),
        ];
        let mut require: Map<String, Value> = (names.iter())
            .map(|(n, _)| (n.to_string(), "*".into()))
            .collect();
        require.insert("ext-json".into(), "*".into());
        let manifest = object(json!({"minimum-stability": "dev", "require": require}));
        let packages = repository().as_array().unwrap().clone();
        let repo: Box<dyn Repository> = Box::new(ArrayRepository::of(
            packages.into_iter().map(object).collect(),
        ));
        let platform = vec![
            object(json!({"name": "php", "version": "8.5.12"})),
            object(json!({"name": "ext-json", "version": "8.5.12"})),
        ];
        let options = Options::default();
        let resolved =
            super::super::resolve(&manifest, platform, vec![repo], None, &options, &|_| None);
        let resolution = resolved.unwrap().unwrap_or_else(|_| panic!("no solution"));
        for (name, want) in names {
            assert_eq!(
                recommended(&resolution, name).as_deref(),
                Some(want),
                "{name}"
            );
        }
        assert_eq!(recommended(&resolution, "ext-json"), None);
    }

    #[test]
    fn updates_the_hash_as_composer_require() {
        let manifest = object(json!({
            "name": "acme/app",
            "minimum-stability": "dev",
            "require": {"acme/zero": "0.3.4@dev", "acme/branch": "^1.0@dev", "acme/beta": "^2.0@beta", "acme/dated": "2024-01-01", "acme/stable": "^1.2"},
            "repositories": [{"type": "package", "package": repository()}, {"packagist.org": false}]
        }));
        let mut lock = Lock {
            content_hash: String::new(),
            settings: object(json!({"stability-flags": {"acme/zero": 20, "acme/beta": 15}})),
            packages: vec![],
            packages_dev: vec![],
        };
        let picked: Vec<(String, String)> = [
            ("acme/branch", "^1.0@dev"),
            ("acme/beta", "^2.0@beta"),
            ("acme/dated", "2024-01-01"),
            ("acme/stable", "^1.2"),
        ]
        .iter()
        .map(|(n, c)| (n.to_string(), c.to_string()))
        .collect();
        update_hash(&mut lock, &manifest, &picked).unwrap();
        assert_eq!(lock.content_hash, "2297c4c8b0ed5fa1f5e4f2bb9f6eb06e");
        assert_eq!(
            serde_json::to_string(&lock.settings["stability-flags"]).unwrap(),
            r#"{"acme/beta":10,"acme/branch":20,"acme/zero":20}"#
        );
    }
}
