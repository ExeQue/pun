use super::composer_repo::{ComposerRepository, PACKAGIST};
use super::pool::{ArrayRepository, Repository, filtered};
use crate::Env;
use crate::error::{Context, Error, Result, bail};
use crate::fetch::Fetcher;
use crate::installed::{git_version, root_version};
use crate::php::{Key, serialize, sha1};
use pcre2::bytes::Regex;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

pub fn configs(sources: &[&Value]) -> Result<Vec<Map<String, Value>>> {
    let mut repos: Vec<(Key, Value)> = vec![(
        Key::Str("packagist.org".into()),
        serde_json::json!({"type": "composer", "url": PACKAGIST}),
    )];
    for source in sources {
        let new: Vec<(Key, Value)> = match source {
            Value::Array(list) => (list.iter().enumerate())
                .map(|(i, v)| (Key::Int(i as i64), v.clone()))
                .collect(),
            Value::Object(m) if !m.is_empty() => {
                m.iter().map(|(k, v)| (Key::of(k), v.clone())).collect()
            }
            _ => continue,
        };
        repos.reverse();
        for (name, repo) in new.into_iter().rev() {
            if repo == Value::Bool(false) {
                disable(&mut repos, &key_name(&name));
                continue;
            }
            if let Value::Object(m) = &repo
                && m.len() == 1
                && m.values().next() == Some(&Value::Bool(false))
            {
                let name = m.keys().next().cloned().unwrap_or_default();
                disable(&mut repos, &name);
                continue;
            }
            if repo.get("type").and_then(Value::as_str) == Some("composer")
                && let Some(url) = repo.get("url").and_then(Value::as_str)
                && PACKAGIST_URL.is_match(url.as_bytes()).unwrap_or(false)
            {
                disable(&mut repos, "packagist.org");
            }
            match name {
                Key::Int(i) => {
                    let taken = repos.iter().any(|(k, _)| *k == Key::Int(i));
                    let key = if taken {
                        let next = (repos.iter())
                            .filter_map(|(k, _)| match k {
                                Key::Int(n) => Some(n + 1),
                                Key::Str(_) => None,
                            })
                            .max()
                            .unwrap_or(0);
                        Key::Int(next)
                    } else {
                        Key::Int(i)
                    };
                    repos.push((key, repo));
                }
                Key::Str(s) => {
                    let s = if s == "packagist" {
                        "packagist.org".into()
                    } else {
                        s
                    };
                    match repos.iter_mut().find(|(k, _)| *k == Key::Str(s.clone())) {
                        Some(slot) => slot.1 = repo,
                        None => repos.push((Key::Str(s), repo)),
                    }
                }
            }
        }
        repos.reverse();
    }
    (repos.into_iter())
        .map(|(name, repo)| match repo {
            Value::Object(m) if m.get("type").is_some_and(Value::is_string) => Ok(m),
            Value::Object(_) => bail!(
                "Repository \"{}\" ({}) must have a type defined",
                key_name(&name),
                repo
            ),
            Value::String(_) => bail!(
                "\"repositories\" should be an array of repository definitions, only a single repository was given"
            ),
            _ => bail!("Repository \"{}\" ({repo}) should be an array", key_name(&name)),
        })
        .collect()
}

static PACKAGIST_URL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^https?://(?:[a-z0-9-.]+\.)?packagist.org(/|$)").expect("valid pattern")
});

fn key_name(k: &Key) -> String {
    match k {
        Key::Int(i) => i.to_string(),
        Key::Str(s) => s.clone(),
    }
}

fn disable(repos: &mut Vec<(Key, Value)>, name: &str) {
    let key = Key::of(name);
    if repos.iter().any(|(k, _)| *k == key) {
        repos.retain(|(k, _)| *k != key);
    } else if name == "packagist" {
        repos.retain(|(k, _)| *k != Key::Str("packagist.org".into()));
    }
}

pub fn open<'a>(
    root: &Path,
    manifest: &Map<String, Value>,
    global: Option<&Value>,
    fetcher: &'a Fetcher,
    env: Env,
) -> Result<Vec<Box<dyn Repository + 'a>>> {
    let mut sources = vec![];
    sources.extend(global);
    sources.extend(manifest.get("repositories"));
    let mut out: Vec<Box<dyn Repository + 'a>> = vec![];
    for config in configs(&sources)? {
        let kind = config["type"].as_str().unwrap_or_default();
        let repo: Box<dyn Repository + 'a> = match kind {
            "composer" => {
                let Some(url) = config.get("url").and_then(Value::as_str) else {
                    bail!("A repository of type \"composer\" needs a url");
                };
                Box::new(ComposerRepository::open(fetcher, url))
            }
            "package" => Box::new(ArrayRepository::from_config(&config)?),
            "path" => {
                let packages = path_packages(root, &config, env)?;
                Box::new(ArrayRepository::of(packages))
            }
            "vcs" | "git" | "github" | "gitlab" | "bitbucket" | "git-bitbucket" | "svn"
            | "fossil" | "perforce" | "hg" | "artifact" | "pear" => bail!(
                "Pun does not read {kind} repositories yet ({}); publish the package to a Composer repository, or use a path repository",
                config.get("url").and_then(Value::as_str).unwrap_or(kind)
            ),
            other => bail!("Repository type is not registered: {other}"),
        };
        out.push(filtered(repo, &config)?);
    }
    Ok(out)
}

fn expand_path(path: &str, env: Env) -> String {
    let env = |k: &str| env(k).unwrap_or_default();
    if let Some(rest) = path.strip_prefix('~')
        && (rest.starts_with('/') || rest.starts_with('\\'))
    {
        return format!("{}{rest}", env("HOME"));
    }
    static VAR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^(?:\$(\w++)|%(\w++)%)(.*)$").expect("valid pattern"));
    match VAR.captures(path.as_bytes()).ok().flatten() {
        Some(c) => {
            let text = |i: usize| c.get(i).map(|m| String::from_utf8_lossy(m.as_bytes()));
            let name = text(1).or_else(|| text(2)).unwrap_or_default();
            format!("{}{}", env(&name), text(3).unwrap_or_default())
        }
        None => path.to_string(),
    }
}

pub fn path_packages(
    root: &Path,
    config: &Map<String, Value>,
    env: Env,
) -> Result<Vec<Map<String, Value>>> {
    let Some(url) = config.get("url").and_then(Value::as_str) else {
        bail!("You must specify the `url` configuration for the path repository");
    };
    let url = expand_path(url, env);
    let root_version = root_version(env);
    let mut options = match config.get("options") {
        Some(Value::Object(o)) => o.clone(),
        _ => Map::new(),
    };
    if !options.contains_key("relative") {
        options.insert("relative".into(), (!Path::new(&url).is_absolute()).into());
    }
    let matches = glob_dirs(root, &url);
    if matches.is_empty() {
        let wild = |s: &str| s.contains(['*', '{', '}']);
        if wild(&url) {
            let mut base = url.as_str();
            while wild(base) {
                base = Path::new(base)
                    .parent()
                    .and_then(Path::to_str)
                    .unwrap_or_default();
            }
            if root.join(base).is_dir() {
                return Ok(vec![]);
            }
        }
        bail!("The `url` supplied for the path ({url}) repository does not exist");
    }
    let reference = options
        .get("reference")
        .and_then(Value::as_str)
        .unwrap_or("auto")
        .to_string();
    let mut out = vec![];
    for dir in matches {
        let path = root.join(&dir);
        let file = path.join("composer.json");
        let json = match std::fs::read_to_string(&file) {
            Ok(json) => json,
            Err(_) => {
                let Some(m) = pun_config::manifest::read(&path).map_err(|e| Error::foreign(&*e))?
                else {
                    continue;
                };
                Value::Object(m.json).to_string()
            }
        };
        let Value::Object(mut package) = serde_json::from_str(&json)
            .with_context(|| format!("{} does not contain valid JSON", file.display()))?
        else {
            bail!("Failed loading the package in {}", file.display());
        };
        package.shift_remove("published-time");
        let mut dist = Map::new();
        dist.insert("type".into(), "path".into());
        dist.insert("url".into(), dir.into());
        match reference.as_str() {
            "none" => {
                dist.insert("reference".into(), Value::Null);
            }
            "config" | "auto" => {
                let hashed = format!("{json}{}", serialize(&Value::Object(options.clone())));
                dist.insert("reference".into(), sha1(hashed.as_bytes()).into());
            }
            _ => {}
        }
        let transport: Map<String, Value> = (options.iter())
            .filter(|(k, _)| *k == "symlink" || *k == "relative")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        package.insert("transport-options".into(), Value::Object(transport));
        let name = package
            .get("name")
            .and_then(Value::as_str)
            .map(String::from);
        if let Some(v) = (name.as_deref())
            .and_then(|n| options.get("versions")?.get(n))
            .cloned()
        {
            package.insert("version".into(), v);
        }
        if !package.contains_key("version")
            && let Some(v) = &root_version
            && head(&path).is_some()
            && head(&path) == head(root)
        {
            package.insert("version".into(), v.as_str().into());
        }
        if reference == "auto"
            && path.join(".git").is_dir()
            && let Some(commit) = head(&path)
        {
            dist.insert("reference".into(), commit.into());
        }
        package.insert("dist".into(), Value::Object(dist));
        if !package.contains_key("version") {
            match git_version(&package, &path) {
                Some(guess) => {
                    if let Some((feature, _)) = guess.feature {
                        let mut first = package.clone();
                        first.insert("version".into(), feature.into());
                        out.push(first);
                    }
                    package.insert("version".into(), guess.pretty.into());
                }
                None => {
                    package.insert("version".into(), "dev-main".into());
                }
            }
        }
        out.push(package);
    }
    Ok(out)
}

fn head(dir: &Path) -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .env_remove("GIT_DIR")
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn glob_dirs(root: &Path, pattern: &str) -> Vec<String> {
    let mut out = vec![];
    for alternative in braces(pattern) {
        let absolute = alternative.starts_with('/');
        let mut found: Vec<String> = vec![if absolute { "/".into() } else { String::new() }];
        for segment in alternative.split('/').filter(|s| !s.is_empty()) {
            let mut next = vec![];
            for base in &found {
                let dir: PathBuf = if base.is_empty() {
                    root.to_path_buf()
                } else {
                    root.join(base)
                };
                let join = |name: &str| {
                    if base.is_empty() || base.ends_with('/') {
                        format!("{base}{name}")
                    } else {
                        format!("{base}/{name}")
                    }
                };
                if !segment.contains(['*', '?', '[']) {
                    if dir.join(segment).exists() {
                        next.push(join(segment));
                    }
                    continue;
                }
                let Ok(re) = Regex::new(&segment_regex(segment)) else {
                    continue;
                };
                let mut names: Vec<String> = (std::fs::read_dir(&dir).into_iter().flatten())
                    .flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| re.is_match(n.as_bytes()).unwrap_or(false))
                    .collect();
                names.sort();
                next.extend(names.iter().map(|n| join(n)));
            }
            found = next;
        }
        let mut dirs: Vec<String> = (found.into_iter())
            .filter(|f| !f.is_empty() && root.join(f).is_dir())
            .map(|f| {
                if f == "/" {
                    f
                } else {
                    f.trim_end_matches('/').to_string()
                }
            })
            .collect();
        dirs.sort();
        out.extend(dirs);
    }
    out
}

fn segment_regex(segment: &str) -> String {
    let mut re = String::from("^");
    if segment.starts_with(['*', '?', '[']) {
        re.push_str(r"(?!\.)");
    }
    let mut chars = segment.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            '[' if !chars.clone().any(|c| c == ']') => re.push_str(r"\["),
            '[' => {
                re.push('[');
                if chars.peek() == Some(&'!') {
                    chars.next();
                    re.push('^');
                }
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    if c == '\\' || c == '^' {
                        re.push('\\');
                    }
                    re.push(c);
                }
                re.push(']');
            }
            c => {
                if !c.is_alphanumeric() {
                    re.push('\\');
                }
                re.push(c);
            }
        }
    }
    re.push('$');
    re
}

fn braces(pattern: &str) -> Vec<String> {
    let bytes = pattern.as_bytes();
    let Some(open) = pattern.find('{') else {
        return vec![pattern.to_string()];
    };
    let (mut depth, mut close, mut commas) = (0, None, vec![]);
    for (i, &b) in bytes.iter().enumerate().skip(open) {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            b',' if depth == 1 => commas.push(i),
            _ => {}
        }
    }
    let Some(close) = close else {
        return vec![pattern.to_string()];
    };
    let mut bounds = vec![open];
    bounds.extend(commas);
    bounds.push(close);
    let (head, tail) = (&pattern[..open], &pattern[close + 1..]);
    bounds
        .windows(2)
        .flat_map(|w| braces(&format!("{head}{}{tail}", &pattern[w[0] + 1..w[1]])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn urls(sources: &[Value]) -> Vec<String> {
        let sources: Vec<&Value> = sources.iter().collect();
        (configs(&sources).unwrap().iter())
            .map(|r| {
                r.get("url")
                    .and_then(Value::as_str)
                    .unwrap_or("-")
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn merges_repositories_as_composer() {
        assert_eq!(
            urls(&[
                json!([{"type": "composer", "url": "https://a.test"}, {"packagist.org": false}]),
                json!({"x": {"type": "path", "url": "p"}, "0": {"type": "package", "package": {}}})
            ]),
            ["p", "-", "https://a.test"]
        );
        assert!(urls(&[json!({"packagist": false})]).is_empty());
        assert_eq!(
            urls(&[
                json!([{"type": "composer", "url": "https://repo.packagist.org/"}, {"type": "path", "url": "q"}]),
                json!([{"type": "path", "url": "a"}])
            ]),
            ["a", "https://repo.packagist.org/", "q"]
        );
        assert_eq!(
            urls(&[
                json!([{"type": "path", "url": "g"}]),
                json!({"z": {"type": "path", "url": "l"}, "packagist.org": false})
            ]),
            ["l", "g"]
        );
        assert_eq!(urls(&[json!([])]), [PACKAGIST]);
        assert_eq!(urls(&[json!({})]), [PACKAGIST]);
        assert_eq!(urls(&[json!({"foo": false})]), [PACKAGIST]);
        assert_eq!(
            urls(&[
                json!([{"type": "path", "url": "g"}]),
                json!([{"type": "path", "url": "a"}])
            ]),
            ["a", "g", PACKAGIST]
        );
        for name in ["packagist", "packagist.org"] {
            let mirror = json!({ name: {"type": "composer", "url": "https://mirror.test"} });
            assert_eq!(urls(&[mirror]), ["https://mirror.test"]);
        }
        assert!(configs(&[&json!([{"url": "x"}])]).is_err());
        assert!(configs(&[&json!({"a": "https://x"})]).is_err());
        assert!(configs(&[&json!([1])]).is_err());
    }

    #[test]
    fn reads_path_repositories_as_composer() {
        let d = std::env::temp_dir().join(format!("pun-path-repo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for (dir, json) in [
            (
                "packages/alpha",
                r#"{"name":"acme/alpha","version":"1.2.0","published-time":"x"}"#,
            ),
            ("packages/beta", r#"{"name":"acme/beta"}"#),
            ("packages/.hidden", r#"{"name":"acme/hidden"}"#),
            (
                "other",
                r#"{"name":"acme/other","extra":{"branch-alias":{"dev-main":"2.x-dev"}}}"#,
            ),
            ("libs/x", r#"{"name":"acme/x"}"#),
        ] {
            std::fs::create_dir_all(d.join(dir)).unwrap();
            std::fs::write(d.join(dir).join("composer.json"), format!("{json}\n")).unwrap();
        }
        std::fs::create_dir_all(d.join("packages/nojson")).unwrap();
        let x = d.join("libs/x");
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .args(["-c", "user.email=a@b", "-c", "user.name=a"])
                .args(args)
                .current_dir(&x)
                .env_remove("GIT_DIR")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}");
        };
        git(&["init", "-q", "-b", "feature-y"]);
        git(&["commit", "-q", "--allow-empty", "-m", "one"]);
        git(&["commit", "-q", "--allow-empty", "-m", "two"]);
        let commit = head(&x).unwrap();
        let read = |config: Value| -> Vec<Value> {
            (path_packages(&d, config.as_object().unwrap(), &|_| None)
                .unwrap()
                .iter())
            .map(|p| {
                json!([
                    p["name"],
                    p["version"],
                    p["dist"]["url"],
                    p["dist"]["reference"],
                    p["transport-options"]
                ])
            })
            .collect()
        };
        let relative = json!({"relative": true});
        assert_eq!(
            read(json!({"type": "path", "url": "packages/*"})),
            [
                json!([
                    "acme/alpha",
                    "1.2.0",
                    "packages/alpha",
                    "aa00a4eb7fc9f0e5ac882610faa162563d3661cf",
                    relative
                ]),
                json!([
                    "acme/beta",
                    "dev-main",
                    "packages/beta",
                    "98cb87020e33009725f05995e63e402f6fed0c0f",
                    relative
                ]),
            ]
        );
        let options = json!({"symlink": false, "relative": true});
        assert_eq!(
            read(
                json!({"type": "path", "url": "./{other,packages/b*}", "options": {"symlink": false, "versions": {"acme/beta": "3.0.1"}}})
            ),
            [
                json!([
                    "acme/other",
                    "dev-main",
                    "./other",
                    "59999b24d8d927d280358e27e9d9bffd5fc2b82f",
                    options
                ]),
                json!([
                    "acme/beta",
                    "3.0.1",
                    "./packages/beta",
                    "c62dcd6e5b84364c9cb6c2beab6f574588a5ab2f",
                    options
                ]),
            ]
        );
        assert_eq!(
            read(json!({"type": "path", "url": "libs/x", "options": {"reference": "none"}})),
            [json!(["acme/x", "dev-feature-y", "libs/x", null, relative])]
        );
        assert_eq!(
            read(
                json!({"type": "path", "url": "libs/*", "options": {"reference": "config", "relative": false}})
            ),
            [
                json!(["acme/x", "dev-feature-y", "libs/x", "2a54daede50536b668fffb84a62b06cee054b56a", {"relative": false}])
            ]
        );
        assert_eq!(
            read(json!({"type": "path", "url": "libs/x"})),
            [json!([
                "acme/x",
                "dev-feature-y",
                "libs/x",
                commit,
                relative
            ])]
        );
        assert!(read(json!({"type": "path", "url": "packages/zz*"})).is_empty());
        std::fs::create_dir_all(d.join("members/toml")).unwrap();
        std::fs::write(
            d.join("members/toml/pun.toml"),
            "[package]\nname = \"acme/toml\"\nversion = \"1.0.0\"\n\n[require]\nphp = \">=8.4\"\n",
        )
        .unwrap();
        let member = read(json!({"type": "path", "url": "members/*"}));
        assert_eq!(
            [&member[0][0], &member[0][1], &member[0][2], &member[0][4]],
            [
                &json!("acme/toml"),
                &json!("1.0.0"),
                &json!("members/toml"),
                &relative
            ]
        );
        for bad in [
            json!({"type": "path", "url": "nothing/*"}),
            json!({"type": "path"}),
        ] {
            assert!(path_packages(&d, bad.as_object().unwrap(), &|_| None).is_err());
        }
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn reads_path_repository_corners_as_composer() {
        let d = std::env::temp_dir().join(format!("pun-path-corners-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for (dir, json) in [
            ("feat", r#"{"name":"acme/feat"}"#),
            ("mono", r#"{"name":"acme/mono"}"#),
            ("mono/pkg", r#"{"name":"acme/pkg"}"#),
            ("gl/a", r#"{"name":"acme/a"}"#),
            ("gl/b", r#"{"name":"acme/b"}"#),
            ("gl/c^", r#"{"name":"acme/c"}"#),
            ("gl/[", r#"{"name":"acme/br"}"#),
            ("bad", "{"),
            ("arr", "[]"),
        ] {
            std::fs::create_dir_all(d.join(dir)).unwrap();
            std::fs::write(d.join(dir).join("composer.json"), format!("{json}\n")).unwrap();
        }
        let git = |dir: &str, args: &[&str]| {
            let out = Command::new("git")
                .args(["-c", "user.email=a@b", "-c", "user.name=a"])
                .args(args)
                .current_dir(d.join(dir))
                .env_remove("GIT_DIR")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}");
        };
        for dir in ["feat", "mono"] {
            git(dir, &["init", "-q", "-b", "main"]);
            git(dir, &["add", "."]);
            git(dir, &["commit", "-q", "-m", "one"]);
        }
        git("feat", &["checkout", "-q", "-b", "feature-z"]);
        git("feat", &["commit", "-q", "--allow-empty", "-m", "two"]);
        let commit = head(&d.join("feat")).unwrap();
        let read = |root: &str, config: Value, version: Option<&str>| {
            let env = |k: &str| {
                version
                    .filter(|_| k == "COMPOSER_ROOT_VERSION")
                    .map(String::from)
            };
            let got = path_packages(&d.join(root), config.as_object().unwrap(), &env);
            got.map(|packages| -> Vec<Value> {
                (packages.iter())
                    .map(|p| json!([p["name"], p["version"], p["dist"]["reference"]]))
                    .collect()
            })
            .map_err(|e| format!("{e:#}"))
        };
        assert_eq!(
            read("", json!({"url": "feat"}), None).unwrap(),
            [
                json!(["acme/feat", "dev-feature-z", commit]),
                json!(["acme/feat", "dev-main", commit])
            ]
        );
        let custom = json!({"url": "feat", "options": {"reference": "custom"}});
        assert_eq!(
            read("", custom, None).unwrap(),
            [
                json!(["acme/feat", "dev-feature-z", null]),
                json!(["acme/feat", "dev-main", null])
            ]
        );
        assert_eq!(
            read("mono", json!({"url": "pkg"}), Some("1.2.x-dev")).unwrap(),
            [json!([
                "acme/pkg",
                "1.2.x-dev",
                "a302f5ef1f2391c2efcea8a7b36168cfff964f61"
            ])]
        );
        let names = |url: &str| -> Vec<Value> {
            (read("gl", json!({ "url": url }), None).unwrap().iter())
                .map(|p| p[0].clone())
                .collect()
        };
        assert_eq!(names("["), ["acme/br"]);
        assert_eq!(names("[ab]"), ["acme/a", "acme/b"]);
        assert_eq!(names("[c^]^"), ["acme/c"]);
        for url in ["[z-a]", "missing"] {
            assert_eq!(
                read("gl", json!({ "url": url }), None).unwrap_err(),
                format!("The `url` supplied for the path ({url}) repository does not exist")
            );
        }
        let bad = read("", json!({"url": "bad"}), None).unwrap_err();
        let file = |dir: &str| d.join(dir).join("composer.json").display().to_string();
        assert!(
            bad.starts_with(&format!("{} does not contain valid JSON", file("bad"))),
            "{bad}"
        );
        assert_eq!(
            read("", json!({"url": "arr"}), None).unwrap_err(),
            format!("Failed loading the package in {}", file("arr"))
        );
        assert_eq!(glob_dirs(&d, "/"), ["/"]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn opens_repositories() {
        let fetcher = Fetcher::new(None, crate::fetch::Auth::default(), false);
        let open = |repositories: Value| {
            let manifest = json!({ "repositories": repositories });
            let root = Path::new(env!("CARGO_MANIFEST_DIR"));
            let opened = open(root, manifest.as_object().unwrap(), None, &fetcher, &|_| {
                None
            });
            opened.map(|r| r.len()).map_err(|e| e.to_string())
        };
        let repos = json!([
            {"type": "package", "package": {"name": "a/a", "version": "1.0.0"}},
            {"type": "path", "url": "tests/fixtures/*"},
            {"packagist.org": false}
        ]);
        assert_eq!(open(repos), Ok(2));
        for (repo, message) in [
            (
                json!({"type": "composer"}),
                "A repository of type \"composer\" needs a url",
            ),
            (
                json!({"type": "vcs", "url": "https://git.test/a"}),
                "Pun does not read vcs repositories yet (https://git.test/a); publish the package to a Composer repository, or use a path repository",
            ),
            (
                json!({"type": "artifact"}),
                "Pun does not read artifact repositories yet (artifact); publish the package to a Composer repository, or use a path repository",
            ),
            (
                json!({"type": "mystery"}),
                "Repository type is not registered: mystery",
            ),
        ] {
            assert_eq!(
                open(json!([repo, {"packagist.org": false}])).unwrap_err(),
                message
            );
        }
    }

    #[test]
    fn globs_as_php() {
        assert_eq!(braces("a{b,c{d,e}}f"), ["abf", "acdf", "acef"]);
        assert_eq!(braces("a{b"), ["a{b"]);
        let re = Regex::new(&segment_regex("[!a-c]?.x*")).unwrap();
        assert!(re.is_match(b"dz.xyz").unwrap() && !re.is_match(b"bz.x").unwrap());
        assert!(
            !Regex::new(&segment_regex("*"))
                .unwrap()
                .is_match(b".git")
                .unwrap()
        );
        let env = |k: &str| match k {
            "HOME" => Some("/home/me".to_string()),
            "PUN_TEST_DIR" => Some("/srv".to_string()),
            _ => None,
        };
        assert_eq!(expand_path("$PUN_TEST_DIR/a", &env), "/srv/a");
        assert_eq!(expand_path("%PUN_TEST_DIR%/a", &env), "/srv/a");
        assert_eq!(expand_path("~/a", &env), "/home/me/a");
        assert_eq!(expand_path("a/~", &env), "a/~");
    }
}
