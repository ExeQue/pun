use crate::Env;
use crate::error::Result;
use crate::lock::LockedPackage;
use crate::paths::shortest;
use crate::php::{export_str, json_pretty};
use crate::version::{branch_alias, is_platform, normalize, normalize_branch};
use pcre2::bytes::Regex;
use pun_tokens::php_compat::strnatcmp;
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;
use std::process::Command;
use std::sync::LazyLock;

const ORDER: [&str; 37] = [
    "name",
    "version",
    "version_normalized",
    "target-dir",
    "source",
    "dist",
    "require",
    "conflict",
    "provide",
    "replace",
    "require-dev",
    "suggest",
    "time",
    "published-time",
    "default-branch",
    "bin",
    "type",
    "extra",
    "installation-source",
    "autoload",
    "autoload-dev",
    "notification-url",
    "include-path",
    "php-ext",
    "archive",
    "scripts",
    "license",
    "authors",
    "description",
    "homepage",
    "keywords",
    "repositories",
    "support",
    "funding",
    "abandoned",
    "transport-options",
    "install-path",
];

#[derive(Debug, Clone)]
pub struct Installed {
    pub package: LockedPackage,
    pub path: Option<String>,
    pub dev: bool,
}

impl Installed {
    pub fn name(&self) -> String {
        self.package.name()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Root {
    pub name: String,
    pub pretty_version: String,
    pub version: String,
    pub reference: Option<String>,
    pub kind: String,
    pub aliases: Vec<String>,
}

pub fn root(manifest: &Map<String, Value>, dir: &Path, env: Env) -> Root {
    let name = manifest
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("__root__")
        .to_ascii_lowercase();
    let kind = manifest
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("library")
        .to_string();
    let (pretty, normalized, reference) = match (
        root_version(env),
        manifest.get("version").and_then(Value::as_str),
    ) {
        (Some(v), _) => (v.clone(), normalize(&v).unwrap_or(v), None),
        (None, Some(v)) => (
            v.to_string(),
            normalize(v).unwrap_or_else(|_| v.to_string()),
            None,
        ),
        (None, None) => match git_version(manifest, dir) {
            Some(g) => (g.pretty, g.version, g.commit),
            None => ("1.0.0+no-version-set".into(), "1.0.0.0".into(), None),
        },
    };
    let mut json = manifest.clone();
    json.insert("version".into(), Value::String(pretty.clone()));
    let aliases = branch_alias(&json).map(|a| a.1).into_iter().collect();
    Root {
        name,
        pretty_version: pretty,
        version: normalized,
        reference,
        kind,
        aliases,
    }
}

pub(crate) fn root_version(env: Env) -> Option<String> {
    static DEV: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)^(\d+(?:\.\d+)*)-dev$").unwrap());
    let v = env("COMPOSER_ROOT_VERSION").filter(|v| !v.is_empty())?;
    Some(match DEV.captures(v.as_bytes()).ok().flatten() {
        Some(c) => format!("{}.x-dev", String::from_utf8_lossy(&c[1])),
        None => v,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GitVersion {
    pub pretty: String,
    pub version: String,
    pub commit: Option<String>,
    pub feature: Option<(String, String)>,
}

pub(crate) fn git_version(manifest: &Map<String, Value>, dir: &Path) -> Option<GitVersion> {
    let git = |args: &[&str]| -> Option<String> {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env_remove("GIT_DIR")
            .output()
            .ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    static CURRENT: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?:\* ) *(\(no branch\)|\(detached from \S+\)|\(HEAD detached at \S+\)|\S+) *([a-f0-9]+) .*$").unwrap()
    });
    static LISTED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^(?:\* )? *((?:remotes/(?:origin|upstream)/)?[^\s/]+) *([a-f0-9]+) .*$")
            .unwrap()
    });
    static HEAD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^ *.+/HEAD ").unwrap());
    let text = |c: &pcre2::bytes::Captures, i: usize| String::from_utf8_lossy(&c[i]).into_owned();
    let output = git(&["branch", "-a", "--no-color", "--no-abbrev", "-v"])?;
    let (mut version, mut pretty, mut commit, mut feature, mut detached) =
        (None, None, None, false, false);
    let mut branches = vec![];
    for line in output.lines().filter(|l| !l.is_empty()) {
        if let Ok(Some(c)) = CURRENT.captures(line.as_bytes()) {
            let branch = text(&c, 1);
            if branch == "(no branch)"
                || branch.starts_with("(detached ")
                || branch.starts_with("(HEAD detached at")
            {
                let v = format!("dev-{}", text(&c, 2));
                (version, pretty, feature, detached) = (Some(v.clone()), Some(v), true, true);
            } else {
                feature = is_feature_branch(manifest, &branch);
                (version, pretty) = (
                    Some(normalize_branch(&branch)),
                    Some(format!("dev-{branch}")),
                );
            }
            commit = Some(text(&c, 2));
        }
        if !HEAD.is_match(line.as_bytes()).unwrap_or(false)
            && let Ok(Some(c)) = LISTED.captures(line.as_bytes())
        {
            branches.push(text(&c, 1));
        }
    }
    let mut feature_version = (feature)
        .then(|| version.clone().zip(pretty.clone()))
        .flatten();
    if feature && let Some(v) = version.clone() {
        let aliased = manifest
            .get("extra")
            .and_then(|e| e.get("branch-alias"))
            .and_then(|a| a.get(&v))
            .is_some();
        let self_version = Value::Object(manifest.clone())
            .to_string()
            .contains("\"self.version\"");
        let branch = v.strip_prefix("dev-").unwrap_or(&v).to_string();
        if (!aliased || self_version) && is_feature_branch(manifest, &branch) {
            branches.sort_by(|a, b| {
                let (ra, rb) = (a.starts_with("remotes/"), b.starts_with("remotes/"));
                if ra == rb {
                    strnatcmp_fold(b, a)
                } else {
                    ra.cmp(&rb)
                }
            });
            let mut best: Option<(usize, usize)> = None;
            for (index, candidate) in branches.iter().enumerate() {
                let name = candidate
                    .split_once("remotes/")
                    .map_or(candidate.as_str(), |(_, r)| {
                        r.split_once('/').map_or(r, |x| x.1)
                    });
                if *candidate == branch || is_feature_branch(manifest, name) {
                    continue;
                }
                if let Some(out) = git(&["rev-list", &format!("{candidate}..{branch}")])
                    && best.is_none_or(|(len, _)| out.len() < len)
                {
                    best = Some((out.len(), index));
                    version = Some(normalize_branch(name));
                    pretty = Some(format!("dev-{name}"));
                }
            }
        }
    }
    if (version.is_none() || detached)
        && let Some(tag) = git(&["describe", "--exact-match", "--tags"])
        && let Ok(v) = normalize(tag.trim())
    {
        (version, pretty) = (Some(v), Some(tag.trim().to_string()));
        feature_version = None;
    }
    let version = version?;
    let pretty = pretty.unwrap_or_else(|| version.clone());
    let feature = feature_version
        .filter(|(v, p)| *v != version || *p != pretty)
        .map(|(v, p)| (x_dev(&v, p), v));
    Some(GitVersion {
        pretty: x_dev(&version, pretty),
        version,
        commit,
        feature,
    })
}

fn x_dev(version: &str, pretty: String) -> String {
    if version.ends_with("-dev") && version.contains(".9999999") {
        return crate::version::branch_alias(
            serde_json::json!({"version": "dev-x", "extra": {"branch-alias": {"dev-x": version}}})
                .as_object()
                .expect("an object"),
        )
        .map_or(pretty, |a| a.1);
    }
    pretty
}

fn strnatcmp_fold(a: &str, b: &str) -> std::cmp::Ordering {
    pun_tokens::php_compat::strnatcasecmp(a.as_bytes(), b.as_bytes()).cmp(&0)
}

fn is_feature_branch(manifest: &Map<String, Value>, branch: &str) -> bool {
    let extra: Vec<&str> = (manifest
        .get("non-feature-branches")
        .and_then(Value::as_array)
        .into_iter()
        .flatten())
    .filter_map(Value::as_str)
    .collect();
    let pattern = format!(
        r"^({}|master|main|latest|next|current|support|tip|trunk|default|develop|\d+\..+)$",
        extra.join("|")
    );
    !Regex::new(&pattern).is_ok_and(|r| r.is_match(branch.as_bytes()).unwrap_or(false))
}

pub fn installed_json(packages: &[Installed], vendor: &str, dev_mode: bool) -> String {
    let repo = format!("{vendor}/composer");
    let mut entries: Vec<(String, Value)> = (packages.iter())
        .map(|p| {
            let mut m = p.package.0.clone();
            let version = m.get("version").and_then(Value::as_str).unwrap_or_default();
            let normalized = normalize(version).unwrap_or_else(|_| version.to_string());
            m.insert("version_normalized".into(), Value::String(normalized));
            if m.contains_key("dist") {
                m.insert("installation-source".into(), "dist".into());
            } else if m.contains_key("source") {
                m.insert("installation-source".into(), "source".into());
            }
            let path = p
                .path
                .as_ref()
                .map(|path| Value::String(shortest(&repo, path, true)));
            m.insert("install-path".into(), path.unwrap_or(Value::Null));
            let mut ordered = Map::new();
            for k in ORDER {
                if let Some(v) = m.shift_remove(k) {
                    ordered.insert(k.into(), v);
                }
            }
            (p.package.pretty_name().to_string(), Value::Object(ordered))
        })
        .collect();
    entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut dev_names: Vec<String> = packages
        .iter()
        .filter(|p| p.dev)
        .map(Installed::name)
        .collect();
    dev_names.sort();
    let json = serde_json::json!({
        "packages": entries.into_iter().map(|e| e.1).collect::<Vec<_>>(),
        "dev": dev_mode,
        "dev-package-names": dev_names,
    });
    format!("{}\n", json_pretty(&json))
}

#[derive(Default)]
struct Entry {
    fields: Vec<(&'static str, Php)>,
    replaced: Vec<String>,
    provided: Vec<String>,
}

enum Php {
    Str(String),
    Null,
    Bool(bool),
    List(Vec<String>),
    Path(Option<String>),
}

pub fn installed_php(
    root: &Root,
    packages: &[Installed],
    vendor: &str,
    base: &str,
    dev_mode: bool,
) -> String {
    let repo = format!("{vendor}/composer");
    let root_path = shortest(&repo, base, true);
    let reference = |r: Option<&str>| r.map_or(Php::Null, |r| Php::Str(r.to_string()));
    let mut versions: BTreeMap<String, Entry> = BTreeMap::new();
    let mut add = |name: String,
                   pretty: &str,
                   version: &str,
                   refr: Php,
                   kind: &str,
                   path: Option<String>,
                   aliases: Vec<String>,
                   dev: bool| {
        versions.insert(
            name,
            Entry {
                fields: vec![
                    ("pretty_version", Php::Str(pretty.into())),
                    ("version", Php::Str(version.into())),
                    ("reference", refr),
                    ("type", Php::Str(kind.into())),
                    ("install_path", Php::Path(path)),
                    ("aliases", Php::List(aliases)),
                    ("dev_requirement", Php::Bool(dev)),
                ],
                ..Entry::default()
            },
        );
    };
    for p in packages {
        let pretty = p.package.version();
        let normalized = normalize(pretty).unwrap_or_else(|_| pretty.to_string());
        let kind = p.package.kind().unwrap_or("library");
        let path = p.path.as_ref().map(|path| shortest(&repo, path, true));
        let aliases = branch_alias(&p.package).map(|a| a.1).into_iter().collect();
        add(
            p.name(),
            pretty,
            &normalized,
            reference(p.package.reference()),
            kind,
            path,
            aliases,
            p.dev,
        );
    }
    add(
        root.name.clone(),
        &root.pretty_version,
        &root.version,
        reference(root.reference.as_deref()),
        &root.kind,
        Some(root_path.clone()),
        root.aliases.clone(),
        false,
    );
    let links = (packages.iter())
        .map(|p| (&p.package.0, p.dev, p.package.version().to_string()))
        .collect::<Vec<_>>();
    let mut sources: Vec<(&Map<String, Value>, bool, String)> = links;
    let root_links = Map::new();
    sources.push((&root_links, false, root.pretty_version.clone()));
    for (package, dev, pretty) in sources {
        for (kind, replaced) in [("replace", true), ("provide", false)] {
            for (target, constraint) in package
                .get(kind)
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                let target = target.to_ascii_lowercase();
                if is_platform(&target) {
                    continue;
                }
                let entry = versions.entry(target).or_default();
                match entry.fields.iter_mut().find(|f| f.0 == "dev_requirement") {
                    None => entry.fields.push(("dev_requirement", Php::Bool(dev))),
                    Some(f) if !dev => f.1 = Php::Bool(false),
                    Some(_) => {}
                }
                let constraint = constraint.as_str().unwrap_or_default();
                let value = if constraint == "self.version" {
                    pretty.clone()
                } else {
                    constraint.to_string()
                };
                let list = if replaced {
                    &mut entry.replaced
                } else {
                    &mut entry.provided
                };
                if !list.contains(&value) {
                    list.push(value);
                }
            }
        }
    }
    let mut out = String::from("<?php return array(\n    'root' => array(\n");
    let mut root_fields = vec![
        ("name", Php::Str(root.name.clone())),
        ("pretty_version", Php::Str(root.pretty_version.clone())),
        ("version", Php::Str(root.version.clone())),
        ("reference", reference(root.reference.as_deref())),
        ("type", Php::Str(root.kind.clone())),
        ("install_path", Php::Path(Some(root_path))),
        ("aliases", Php::List(root.aliases.clone())),
        ("dev", Php::Bool(dev_mode)),
    ];
    fields(&mut out, &mut root_fields, 2);
    out.push_str("    ),\n    'versions' => array(\n");
    for (name, mut entry) in versions {
        writeln!(out, "        {} => array(", export_str(&name)).unwrap();
        let sort = |list: &mut Vec<String>| {
            list.sort_by(|a, b| strnatcmp(a.as_bytes(), b.as_bytes()).cmp(&0));
        };
        for f in &mut entry.fields {
            if let Php::List(l) = &mut f.1 {
                sort(l);
            }
        }
        sort(&mut entry.replaced);
        sort(&mut entry.provided);
        if !entry.replaced.is_empty() {
            entry.fields.push(("replaced", Php::List(entry.replaced)));
        }
        if !entry.provided.is_empty() {
            entry.fields.push(("provided", Php::List(entry.provided)));
        }
        fields(&mut out, &mut entry.fields, 3);
        out.push_str("        ),\n");
    }
    out.push_str("    ),\n);\n");
    out
}

fn fields(out: &mut String, fields: &mut [(&str, Php)], level: usize) {
    let pad = "    ".repeat(level);
    for (k, v) in fields.iter() {
        write!(out, "{pad}{} => ", export_str(k)).unwrap();
        match v {
            Php::Str(s) => writeln!(out, "{},", export_str(s)).unwrap(),
            Php::Null | Php::Path(None) => out.push_str("null,\n"),
            Php::Bool(b) => writeln!(out, "{b},").unwrap(),
            Php::Path(Some(p)) if p.starts_with('/') => {
                writeln!(out, "{},", export_str(p)).unwrap();
            }
            Php::Path(Some(p)) => {
                writeln!(out, "__DIR__ . {},", export_str(&format!("/{p}"))).unwrap();
            }
            Php::List(items) if items.is_empty() => out.push_str("array(),\n"),
            Php::List(items) => {
                out.push_str("array(\n");
                for (i, item) in items.iter().enumerate() {
                    writeln!(out, "{pad}    {i} => {},", export_str(item)).unwrap();
                }
                writeln!(out, "{pad}),").unwrap();
            }
        }
    }
}

pub fn read_installed(vendor: &Path) -> Result<Vec<LockedPackage>> {
    let path = vendor.join("composer/installed.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(vec![]);
    };
    let json: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    let list = match &json {
        Value::Object(m) => m.get("packages").cloned().unwrap_or(Value::Null),
        v => v.clone(),
    };
    Ok(list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p.as_object().cloned().map(LockedPackage))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn installed(p: Value, path: Option<&str>, dev: bool) -> Installed {
        Installed {
            package: LockedPackage(p.as_object().unwrap().clone()),
            path: path.map(String::from),
            dev,
        }
    }

    #[test]
    fn reads_both_installed_json_shapes() {
        let d = std::env::temp_dir().join(format!("pun-read-installed-{}", std::process::id()));
        std::fs::create_dir_all(d.join("composer")).unwrap();
        assert!(read_installed(&d).unwrap().is_empty());
        for text in [
            r#"[{"name": "a/b"}]"#,
            r#"{"packages": [{"name": "a/b"}], "dev": true}"#,
        ] {
            std::fs::write(d.join("composer/installed.json"), text).unwrap();
            let names: Vec<Value> = read_installed(&d)
                .unwrap()
                .into_iter()
                .map(|p| p["name"].clone())
                .collect();
            assert_eq!(names, [json!("a/b")]);
        }
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn root_version_matches_composer() {
        let d = std::env::temp_dir().join(format!("pun-root-version-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .args(["-c", "user.email=a@b", "-c", "user.name=a"])
                .args(args)
                .current_dir(&d)
                .env_remove("GIT_DIR")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}");
            String::from_utf8(out.stdout).unwrap().trim().to_string()
        };
        let manifest = |v: Value| v.as_object().unwrap().clone();
        let plain = manifest(json!({"name": "acme/app", "non-feature-branches": ["stable-.*"]}));
        let guess = |m: &Map<String, Value>| {
            let r = root(m, &d, &|_| None);
            (r.pretty_version, r.version, r.reference)
        };
        let at = |v: &str, n: &str, commit: &str| {
            Some((v.to_string(), n.to_string(), Some(commit.to_string())))
        };
        assert_eq!(git_version(&plain, &d), None, "not a repository");
        git(&["init", "-q", "-b", "main"]);
        git(&["commit", "-q", "--allow-empty", "-m", "one"]);
        let one = git(&["rev-parse", "HEAD"]);
        git(&["branch", "2.x"]);
        git(&["commit", "-q", "--allow-empty", "-m", "two"]);
        let two = git(&["rev-parse", "HEAD"]);
        git(&["tag", "v1.4.0"]);
        git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
        git(&[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ]);
        git(&["checkout", "-qb", "feature-x"]);
        git(&["commit", "-q", "--allow-empty", "-m", "three"]);
        let three = git(&["rev-parse", "HEAD"]);
        git(&["branch", "stable-1", "2.x"]);

        assert_eq!(
            Some(guess(&plain)),
            at("dev-main", "dev-main", &three),
            "feature branch"
        );
        assert_eq!(
            git_version(&plain, &d).unwrap().feature,
            Some(("dev-feature-x".into(), "dev-feature-x".into())),
            "guessVersion's feature_pretty_version and feature_version"
        );
        git(&["checkout", "-q", "main"]);
        assert_eq!(Some(guess(&plain)), at("dev-main", "dev-main", &two));
        git(&["checkout", "-q", "2.x"]);
        assert_eq!(
            Some(guess(&plain)),
            at("2.x-dev", "2.9999999.9999999.9999999-dev", &one)
        );
        git(&["checkout", "-q", "stable-1"]);
        assert_eq!(
            Some(guess(&plain)),
            at("dev-stable-1", "dev-stable-1", &one)
        );
        git(&["checkout", "-q", "v1.4.0"]);
        assert_eq!(Some(guess(&plain)), at("v1.4.0", "1.4.0.0", &two), "tag");
        git(&["checkout", "-q", &three]);
        assert_eq!(
            Some(guess(&plain)),
            at("dev-main", "dev-main", &three),
            "detached"
        );
        git(&["checkout", "-q", "feature-x"]);
        let aliased =
            json!({"name": "acme/app", "extra": {"branch-alias": {"dev-feature-x": "3.x-dev"}}});
        assert_eq!(
            Some(guess(&manifest(aliased.clone()))),
            at("dev-feature-x", "dev-feature-x", &three),
            "a branch alias keeps the feature branch"
        );
        let mut self_version = aliased;
        self_version["require"] = json!({"acme/x": "self.version"});
        assert_eq!(
            Some(guess(&manifest(self_version))),
            at("dev-main", "dev-main", &three),
            "unless something requires self.version"
        );

        let versioned = manifest(json!({"version": "1.0"}));
        assert_eq!(guess(&versioned), ("1.0".into(), "1.0.0.0".into(), None));
        for (env, pretty, normalized) in [
            ("1.2-dev", "1.2.x-dev", "1.2.9999999.9999999-dev"),
            ("dev-main", "dev-main", "dev-main"),
            ("v2.0", "v2.0", "2.0.0.0"),
        ] {
            let env = |k: &str| (k == "COMPOSER_ROOT_VERSION").then(|| env.to_string());
            let r = root(&versioned, &d, &env);
            assert_eq!(
                (r.pretty_version, r.version, r.reference),
                (pretty.into(), normalized.into(), None)
            );
        }
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn installed_files_match_composer() {
        let packages = [
            installed(
                json!({"name": "acme/lib", "version": "v1.2", "dist": {"type": "zip", "url": "u", "reference": "abc", "shasum": ""},
                "replace": {"acme/old": "self.version", "php": "*"}, "provide": {"psr/log-implementation": "3.0"}, "type": "library", "time": "2024-01-01T00:00:00+00:00"}),
                Some("/p/vendor/acme/lib"),
                false,
            ),
            installed(
                json!({"name": "acme/meta", "version": "1.0.0", "type": "metapackage", "provide": {"psr/log-implementation": "1.0|2.0"}}),
                None,
                false,
            ),
            installed(
                json!({"name": "acme/Dev", "version": "dev-main", "source": {"type": "git", "url": "g", "reference": "def"},
                "extra": {"branch-alias": {"dev-main": "2.x-dev"}}, "replace": {"acme/old": "^1"}}),
                Some("/p/vendor/acme/dev"),
                true,
            ),
        ];
        let json = installed_json(&packages, "/p/vendor", true);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["dev-package-names"], json!(["acme/dev"]));
        assert_eq!(parsed["packages"][0]["name"], "acme/Dev");
        let keys: Vec<&String> = parsed["packages"][1].as_object().unwrap().keys().collect();
        assert_eq!(
            keys,
            [
                "name",
                "version",
                "version_normalized",
                "dist",
                "provide",
                "replace",
                "time",
                "type",
                "installation-source",
                "install-path"
            ]
        );
        assert_eq!(parsed["packages"][1]["install-path"], "../acme/lib");
        assert_eq!(parsed["packages"][2]["install-path"], Value::Null);
        assert_eq!(parsed["packages"][0]["installation-source"], "source");
        let root = Root {
            name: "acme/app".into(),
            pretty_version: "1.0.0+no-version-set".into(),
            version: "1.0.0.0".into(),
            reference: None,
            kind: "project".into(),
            aliases: vec![],
        };
        assert_eq!(
            installed_php(&root, &packages, "/p/vendor", "/p", true),
            include_str!("../tests/fixtures/installed.php")
        );
        let elsewhere = [installed(
            json!({"name": "acme/far", "version": "1.0.0"}),
            Some("/opt/far"),
            false,
        )];
        let php = installed_php(&root, &elsewhere, "/p/vendor", "/p", true);
        assert!(
            php.contains("            'install_path' => '/opt/far',\n"),
            "{php}"
        );
    }
}
