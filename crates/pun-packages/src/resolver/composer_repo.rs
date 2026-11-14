//! A Composer repository (Packagist, Private Packagist, Satis) read through its v2 metadata: one file per package
//! (`p2/<name>.json`, and `p2/<name>~dev.json` for branches), fetched in parallel with conditional requests and
//! kept in Composer's own cache directory, so Pun and Composer share it.

use super::filters::{Advisory, ListEntry};
use super::package::{Origin, Package, load, stability_value};
use super::pool::{Found, Loaded, Repository, Stabilities};
use crate::constraint::Constraint;
use crate::fetch::{Fetched, Fetcher};
use crate::version::{branch_alias, is_platform, normalize, stability_of};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

pub const PACKAGIST: &str = "https://repo.packagist.org";

pub struct ComposerRepository<'a> {
    fetcher: &'a Fetcher,
    url: String,
    cache: Option<PathBuf>,
    metadata_url: String,
    notify: Option<String>,
    files: HashMap<String, Option<File>>,
}

/// A package's metadata file, kept small: its `packages` as JSON text, which each load that needs it parses again,
/// and the fields advisories and filters read. Parsed whole, Packagist's files for one Laravel app take most of a
/// gigabyte.
#[derive(Deserialize)]
struct File {
    packages: Option<Box<RawValue>>,
    minified: Option<String>,
    #[serde(rename = "security-advisories")]
    advisories: Option<Value>,
    filter: Option<Value>,
}

/// The `last-modified` a cached file was stored with; `None` when the file is not a JSON object.
fn last_modified(text: &[u8]) -> Option<Option<String>> {
    #[derive(Deserialize)]
    struct Stamp {
        #[serde(rename = "last-modified")]
        modified: Option<Value>,
    }
    let stamp: Stamp = serde_json::from_slice(text).ok()?;
    Some(stamp.modified.and_then(|m| m.as_str().map(String::from)))
}

/// A file's text as a JSON object.
fn object(text: &[u8], url: &str) -> Result<Map<String, Value>> {
    match serde_json::from_slice(text).with_context(|| format!("{url} is not valid JSON"))? {
        Value::Object(m) => Ok(m),
        _ => bail!("{url} does not hold a JSON object"),
    }
}

/// The cache directory name Composer uses for a repository URL.
fn cache_name(url: &str) -> String {
    url.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// A URL from the repository's root file made absolute against the repository's scheme and host.
fn canonical(base: &str, url: &str) -> String {
    if !url.starts_with('/') {
        return url.to_string();
    }
    if url.starts_with("//") {
        let scheme = base.split("://").next().unwrap_or("https");
        return format!("{scheme}:{url}");
    }
    let host_end = base
        .find("://")
        .and_then(|i| base[i + 3..].find('/').map(|j| i + 3 + j))
        .unwrap_or(base.len());
    format!("{}{url}", &base[..host_end])
}

impl<'a> ComposerRepository<'a> {
    /// The repository at `url`. Nothing is fetched until a package is asked of it.
    pub fn open(fetcher: &'a Fetcher, url: &str) -> ComposerRepository<'a> {
        let url = url.trim_end_matches('/').to_string();
        let cache = (fetcher.cache_dir()).map(|c| c.join("repo").join(cache_name(&url)));
        ComposerRepository {
            fetcher,
            url,
            cache,
            metadata_url: String::new(),
            notify: None,
            files: HashMap::new(),
        }
    }

    /// Reads `packages.json` (from the cache when it is under ten minutes old) the first time it is needed.
    fn root(&mut self) -> Result<()> {
        if !self.metadata_url.is_empty() {
            return Ok(());
        }
        let url = self.url.clone();
        let root = self
            .cached_or_fetch(&format!("{url}/packages.json"), "packages.json", 600)?
            .with_context(|| format!("{url} has no packages.json"))?;
        let Some(metadata) = root.get("metadata-url").and_then(Value::as_str) else {
            bail!(
                "{url} is a Composer repository without a metadata-url (the v1 format), which Pun does not read"
            );
        };
        self.metadata_url = canonical(&url, metadata);
        self.notify = (root.get("notify-batch").or_else(|| root.get("notify")))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(|n| canonical(&url, n));
        Ok(())
    }

    fn cache_path(&self, key: &str) -> Option<PathBuf> {
        let key: String = (key.chars())
            .map(|c| {
                if c.is_ascii_lowercase() || c.is_ascii_digit() || ".$~_".contains(c) {
                    c
                } else {
                    '-'
                }
            })
            .collect();
        self.cache.as_ref().map(|c| c.join(key))
    }

    fn read_cache(&self, key: &str) -> Option<(Vec<u8>, SystemTime)> {
        let path = self.cache_path(key)?;
        let text = std::fs::read(&path).ok()?;
        let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
        Some((text, modified))
    }

    /// The file at `url`: the cached copy when it is younger than `max_age` seconds, else fetched with
    /// `If-Modified-Since`. `None` when the repository has no such file.
    fn cached_or_fetch(
        &self,
        url: &str,
        key: &str,
        max_age: u64,
    ) -> Result<Option<Map<String, Value>>> {
        let cached = self.read_cache(key);
        if let Some((text, at)) = &cached
            && at.elapsed().unwrap_or(Duration::MAX) < Duration::from_secs(max_age)
            && let Ok(data) = object(text, url)
        {
            return Ok(Some(data));
        }
        let text = fetch_file(self.fetcher, url, self.cache_path(key), cached.map(|c| c.0))?;
        text.map(|t| object(&t, url)).transpose()
    }
}

/// One metadata file's text, conditional on the cached copy's `last-modified`; a failed request falls back to the
/// cache. A cached copy that is not a JSON object counts as none.
fn fetch_file(
    fetcher: &Fetcher,
    url: &str,
    cache: Option<PathBuf>,
    cached: Option<Vec<u8>>,
) -> Result<Option<Vec<u8>>> {
    let (cached, since) = match cached.and_then(|t| last_modified(&t).map(|m| (t, m))) {
        Some((text, since)) => (Some(text), since),
        None => (None, None),
    };
    let response = match fetcher.metadata(url, since.as_deref()) {
        Ok(r) => r,
        Err(_) if cached.is_some() => return Ok(cached),
        Err(e) => return Err(e).with_context(|| format!("fetching {url}")),
    };
    match response {
        Fetched::NotModified => {
            if let Some(path) = &cache {
                // Touch the copy so a fresh root file is reused for a while.
                let _ = std::fs::File::options()
                    .append(true)
                    .open(path)
                    .and_then(|f| f.set_modified(SystemTime::now()));
            }
            Ok(cached)
        }
        Fetched::NotFound => Ok(None),
        Fetched::Body(body, modified) => {
            let mut data = object(&body, url)?;
            if let Some(m) = modified {
                data.insert("last-modified".into(), m.into());
            }
            let text = Value::Object(data).to_string().into_bytes();
            if let Some(path) = cache
                && let Some(dir) = path.parent()
                && std::fs::create_dir_all(dir).is_ok()
            {
                let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
                if std::fs::write(&tmp, &text).is_ok() {
                    let _ = std::fs::rename(&tmp, &path);
                }
            }
            Ok(Some(text))
        }
    }
}

/// At most this many metadata requests run at once (Composer allows 12).
const PARALLEL: usize = 16;

/// Fetches each (file, url, cache path) on a few worker threads, which also read the cached copies and parse the
/// results, returning (file, result) in order.
fn fetch_all(
    fetcher: &Fetcher,
    jobs: Vec<(String, String, Option<PathBuf>)>,
) -> Vec<(String, Result<Option<File>>)> {
    let next = std::sync::atomic::AtomicUsize::new(0);
    let jobs: Vec<std::sync::Mutex<Option<_>>> = jobs
        .into_iter()
        .map(|j| std::sync::Mutex::new(Some(j)))
        .collect();
    let results: Vec<std::sync::Mutex<Option<(String, Result<Option<File>>)>>> = (0..jobs.len())
        .map(|_| std::sync::Mutex::new(None))
        .collect();
    std::thread::scope(|scope| {
        for _ in 0..jobs.len().min(PARALLEL) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(job) = jobs.get(i) else { break };
                    let (file, url, path) = job
                        .lock()
                        .expect("a job")
                        .take()
                        .expect("each job runs once");
                    let cached = path.as_ref().and_then(|p| std::fs::read(p).ok());
                    let result = fetch_file(fetcher, &url, path, cached).and_then(|text| {
                        (text.map(|t| serde_json::from_slice::<File>(&t)))
                            .transpose()
                            .with_context(|| format!("{url} is not valid metadata"))
                    });
                    *results[i].lock().expect("a result") = Some((file, result));
                }
            });
        }
    });
    (results.into_iter())
        .map(|r| r.into_inner().expect("a result").expect("every job ran"))
        .collect()
}

/// `MetadataMinifier::expand`: each version holds only what changed from the one before; `__unset` removes a key.
/// Calls `each` with every version expanded in turn, so a caller copies only the versions it keeps.
pub fn expand(
    versions: &[Value],
    mut each: impl FnMut(&Map<String, Value>) -> Result<()>,
) -> Result<()> {
    let mut current: Option<Map<String, Value>> = None;
    for v in versions {
        let Value::Object(v) = v else { continue };
        let next = match current.as_mut() {
            None => current.insert(v.clone()),
            Some(c) => {
                for (k, val) in v {
                    if val == "__unset" {
                        c.shift_remove(k);
                    } else {
                        c.insert(k.clone(), val.clone());
                    }
                }
                c
            }
        };
        each(next)?;
    }
    Ok(())
}

/// `isVersionAcceptable`: the version or its branch alias matches the constraint at an accepted stability.
fn acceptable(
    c: &Constraint,
    name: &str,
    version: &str,
    data: &Map<String, Value>,
    s: &Stabilities,
) -> bool {
    // Only a branch has an alias; most versions are tags, so skip copying their metadata to look.
    let branch = (data.get("version").and_then(Value::as_str))
        .is_some_and(|v| v.starts_with("dev-") || v.ends_with("-dev"));
    let alias = branch.then(|| branch_alias(data).map(|a| a.0)).flatten();
    std::iter::once(version.to_string()).chain(alias).any(|v| {
        s.accepts(&[name], stability_value(stability_of(&v))) && c.matches(&Constraint::exact(&v))
    })
}

impl Repository for ComposerRepository<'_> {
    fn load(
        &mut self,
        index: usize,
        arena: &mut Vec<Package>,
        batch: &[(String, Constraint)],
        stabilities: &Stabilities,
        loaded: &Loaded,
    ) -> Result<Found> {
        let mut wanted: Vec<(String, String, &Constraint)> = vec![];
        for (name, constraint) in batch {
            if is_platform(name) || name == "__root__" {
                continue;
            }
            wanted.push((name.clone(), name.clone(), constraint));
            if stabilities.accepts(&[name], stability_value("dev")) {
                wanted.push((format!("{name}~dev"), name.clone(), constraint));
            }
        }
        if wanted.is_empty() {
            return Ok(Found {
                names: vec![],
                packages: vec![],
            });
        }
        self.root()?;
        let missing: Vec<&String> = (wanted.iter().map(|w| &w.0))
            .filter(|f| !self.files.contains_key(*f))
            .collect();
        let jobs: Vec<(String, String, Option<PathBuf>)> = (missing.iter())
            .map(|file| {
                let url = self.metadata_url.replace("%package%", file);
                let key = format!("provider-{}.json", file.replace('/', "~"));
                ((*file).clone(), url, self.cache_path(&key))
            })
            .collect();
        let fetched = fetch_all(self.fetcher, jobs);
        for (file, data) in fetched {
            self.files.insert(file, data?);
        }
        let mut names = vec![];
        let mut packages = vec![];
        for (file, name, constraint) in wanted {
            let Some(Some(data)) = self.files.get(&file) else {
                continue;
            };
            let all: Option<Value> =
                (data.packages.as_ref()).and_then(|p| serde_json::from_str(p.get()).ok());
            let Some(Value::Array(versions)) = all.as_ref().and_then(|p| p.get(&name)) else {
                continue;
            };
            if !names.contains(&name) {
                names.push(name.clone());
            }
            let mut take = |version: &Map<String, Value>| -> Result<()> {
                let normalized = match version.get("version_normalized").and_then(Value::as_str) {
                    Some(v) if v != "9999999-dev" => v.to_string(),
                    _ => match version
                        .get("version")
                        .and_then(Value::as_str)
                        .map(normalize)
                    {
                        Some(Ok(v)) => v,
                        _ => return Ok(()),
                    },
                };
                if loaded.get(&name).is_some_and(|l| l.contains(&normalized))
                    || !acceptable(constraint, &name, &normalized, version, stabilities)
                {
                    return Ok(());
                }
                let mut version = version.clone();
                // Composer's repository loader keeps no transport options.
                version.shift_remove("transport-options");
                if let Some(n) = &self.notify {
                    version
                        .entry("notification-url")
                        .or_insert_with(|| n.clone().into());
                }
                let loaded =
                    load(version, Origin::Repository(index), arena).with_context(|| {
                        format!("Could not load packages in {} from {file}", self.url)
                    })?;
                packages.extend(loaded);
                Ok(())
            };
            if data.minified.as_deref() == Some("composer/2.0") {
                expand(versions, take)?;
            } else {
                for version in versions.iter().filter_map(Value::as_object) {
                    take(version)?;
                }
            }
        }
        Ok(Found { names, packages })
    }

    fn advisories(&mut self, names: &[String]) -> Result<Vec<Advisory>> {
        let mut out = vec![];
        for name in names {
            let Some(Some(data)) = self.files.get(name) else {
                continue;
            };
            for a in (data
                .advisories
                .as_ref()
                .and_then(Value::as_array)
                .into_iter()
                .flatten())
            .filter_map(Value::as_object)
            {
                out.push(Advisory::parse(name, a)?);
            }
        }
        Ok(out)
    }

    fn list_entries(&mut self, names: &[String]) -> Result<Vec<ListEntry>> {
        let mut out = vec![];
        for name in names {
            let Some(Some(data)) = self.files.get(name) else {
                continue;
            };
            for (list, entries) in data
                .filter
                .as_ref()
                .and_then(Value::as_object)
                .into_iter()
                .flatten()
            {
                for e in entries
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_object)
                {
                    let mut e = e.clone();
                    e.entry("package").or_insert_with(|| name.clone().into());
                    if e.contains_key("constraint") {
                        out.push(ListEntry::parse(list, &e)?);
                    }
                }
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Expected values from composer/metadata-minifier's `MetadataMinifier::expand` on the same list.
    #[test]
    fn expands_as_composer() {
        let versions = [
            json!({"name": "a/a", "version": "2.0", "require": {"php": ">=8"}, "extra": {"x": 1}}),
            json!({"version": "1.0", "extra": "__unset"}),
            json!({"version": "0.9", "require": {"php": ">=7"}}),
        ];
        let mut got = vec![];
        expand(&versions, |v| {
            got.push(Value::Object(v.clone()));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            got,
            [
                json!({"name": "a/a", "version": "2.0", "require": {"php": ">=8"}, "extra": {"x": 1}}),
                json!({"name": "a/a", "version": "1.0", "require": {"php": ">=8"}}),
                json!({"name": "a/a", "version": "0.9", "require": {"php": ">=7"}}),
            ]
        );
    }

    #[test]
    fn names_cache_directories_as_composer() {
        assert_eq!(cache_name(PACKAGIST), "https---repo.packagist.org");
        assert_eq!(
            cache_name("https://repo.example.com:8080/x"),
            "https---repo.example.com-8080-x"
        );
        assert_eq!(
            canonical(PACKAGIST, "/p2/%package%.json"),
            "https://repo.packagist.org/p2/%package%.json"
        );
        assert_eq!(canonical("https://a.b/c", "//cdn.b/p"), "https://cdn.b/p");
        assert_eq!(canonical(PACKAGIST, "https://x/y"), "https://x/y");
    }
}
