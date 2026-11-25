//! `[analyse] php-rules`' result cache, like PHPStan's: the errors of each check (a file, or a trait's body in the
//! context of a class) from earlier runs, so an unchanged check skips the worker and a run whose checks are all
//! unchanged starts none.
//!
//! An entry holds while the run key and the content of the checked file and of each file it depends on are
//! unchanged. Its dependencies, as PHPStan's DependencyResolver finds them, are the files declaring the classes and
//! functions the file names or its expressions' types hold, with their parent classes, interfaces and traits. The run
//! key covers the caller's key (Pun's version, binary and config), the worker, its setup, the neon files with their
//! includes, Composer's lock file and installed packages, and the PHP files the workers had loaded (the rules, the
//! bootstrap files and what those load), whose list the cache keeps and hashes again on each run.
//!
//! The file is JSON: the run key, the loaded files, a table of paths, and the entries, which name their file and
//! dependencies by their index in the table. A file that is missing, unreadable or from another key is an empty cache.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use serde::{Deserialize, Serialize};

use crate::php_rules::RuleError;

/// The checked file and, for a trait's body, the class that uses it and the trait.
pub(crate) type Check = (PathBuf, Option<(String, String)>);

#[derive(Serialize, Deserialize)]
struct Stored {
    key: String,
    loaded: Vec<usize>,
    paths: Vec<PathBuf>,
    entries: Vec<StoredEntry>,
}

#[derive(Serialize, Deserialize)]
struct StoredEntry {
    file: usize,
    context: Option<(String, String)>,
    deps: Vec<usize>,
    digest: String,
    errors: Vec<RuleError>,
}

#[derive(Clone)]
struct Entry {
    deps: Vec<PathBuf>,
    digest: String,
    errors: Vec<RuleError>,
}

pub(crate) struct Cache {
    file: PathBuf,
    /// The run key without the loaded files.
    base: String,
    old: HashMap<Check, Entry>,
    old_loaded: Vec<PathBuf>,
    kept: Mutex<HashMap<Check, Entry>>,
    hashes: Mutex<HashMap<PathBuf, String>>,
}

/// Adds `bytes` to `h`, length-prefixed so two different sequences never hash alike.
fn part(h: &mut blake3::Hasher, bytes: &[u8]) {
    h.update(&(bytes.len() as u64).to_le_bytes());
    h.update(bytes);
}

fn hex(h: &blake3::Hasher) -> String {
    h.finalize().to_hex()[..32].to_string()
}

impl Cache {
    /// The cache in `file` for the run key `base`.
    pub(crate) fn open(file: PathBuf, base: String) -> Cache {
        let mut cache = Cache {
            file,
            base,
            old: HashMap::new(),
            old_loaded: Vec::new(),
            kept: Mutex::default(),
            hashes: Mutex::default(),
        };
        let stored: Option<Stored> =
            (std::fs::read(&cache.file).ok()).and_then(|bytes| serde_json::from_slice(&bytes).ok());
        let Some(stored) = stored else {
            return cache;
        };
        let path = |i: &usize| stored.paths.get(*i).cloned();
        let Some(loaded) = stored.loaded.iter().map(path).collect::<Option<Vec<_>>>() else {
            return cache;
        };
        if cache.key(&loaded) != stored.key {
            return cache;
        }
        for e in &stored.entries {
            let (Some(file), Some(deps)) = (path(&e.file), e.deps.iter().map(path).collect())
            else {
                return cache;
            };
            let entry = Entry {
                deps,
                digest: e.digest.clone(),
                errors: e.errors.clone(),
            };
            cache.old.insert((file, e.context.clone()), entry);
        }
        cache.old_loaded = loaded;
        cache
    }

    fn key(&self, loaded: &[PathBuf]) -> String {
        let mut h = blake3::Hasher::new();
        part(&mut h, self.base.as_bytes());
        for file in loaded {
            part(&mut h, file.as_os_str().as_encoded_bytes());
            part(&mut h, self.hash(file).as_bytes());
        }
        hex(&h)
    }

    /// The hash of `path`'s content (`-` when it can't be read), once per run.
    fn hash(&self, path: &Path) -> String {
        let hashes = || self.hashes.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(h) = hashes().get(path) {
            return h.clone();
        }
        let h = std::fs::read(path).map_or_else(
            |_| "-".to_string(),
            |bytes| blake3::hash(&bytes).to_hex()[..32].to_string(),
        );
        hashes().insert(path.to_path_buf(), h.clone());
        h
    }

    fn digest(&self, check: &Check, deps: &[PathBuf]) -> String {
        let mut h = blake3::Hasher::new();
        part(&mut h, self.base.as_bytes());
        for file in std::iter::once(&check.0).chain(deps) {
            part(&mut h, file.as_os_str().as_encoded_bytes());
            part(&mut h, self.hash(file).as_bytes());
        }
        if let Some((class, trait_name)) = &check.1 {
            part(&mut h, class.as_bytes());
            part(&mut h, trait_name.as_bytes());
        }
        hex(&h)
    }

    /// The errors `check` found in an earlier run, when neither its file nor a file it depends on changed since.
    pub(crate) fn hit(&self, check: &Check) -> Option<Vec<RuleError>> {
        let e = self.old.get(check)?;
        if self.digest(check, &e.deps) != e.digest {
            return None;
        }
        let errors = e.errors.clone();
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.insert(check.clone(), e.clone());
        Some(errors)
    }

    /// Records what `check` found, depending on `deps`.
    pub(crate) fn keep(&self, check: Check, deps: Vec<PathBuf>, errors: Vec<RuleError>) {
        let digest = self.digest(&check, &deps);
        let mut kept = self.kept.lock().unwrap_or_else(PoisonError::into_inner);
        kept.insert(
            check,
            Entry {
                deps,
                digest,
                errors,
            },
        );
    }

    /// Writes this run's entries, and the earlier ones of checks it didn't make whose file still exists, with the
    /// files the workers loaded (`loaded`, plus those of earlier runs). Through a temporary file, so a concurrent run
    /// reads the old cache or the new one, never a mix.
    pub(crate) fn save(&self, loaded: Vec<PathBuf>) {
        let mut entries =
            std::mem::take(&mut *self.kept.lock().unwrap_or_else(PoisonError::into_inner));
        for (check, e) in &self.old {
            if !entries.contains_key(check) && check.0.exists() {
                entries.insert(check.clone(), e.clone());
            }
        }
        let mut loaded: Vec<PathBuf> = loaded.into_iter().chain(self.old_loaded.clone()).collect();
        loaded.sort();
        loaded.dedup();
        let mut paths: Vec<PathBuf> = Vec::new();
        let mut index: HashMap<PathBuf, usize> = HashMap::new();
        let mut at = |p: &PathBuf| {
            *index.entry(p.clone()).or_insert_with(|| {
                paths.push(p.clone());
                paths.len() - 1
            })
        };
        let mut entries: Vec<_> = entries.into_iter().collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let entries = (entries.into_iter())
            .map(|((file, context), e)| StoredEntry {
                file: at(&file),
                context,
                deps: e.deps.iter().map(&mut at).collect(),
                digest: e.digest,
                errors: e.errors,
            })
            .collect();
        let stored = Stored {
            key: self.key(&loaded),
            loaded: loaded.iter().map(&mut at).collect(),
            paths,
            entries,
        };
        if let Ok(bytes) = serde_json::to_vec(&stored) {
            let _ = write(&self.file, &bytes);
        }
    }
}

/// Writes `bytes` to `file` owner-only (the errors quote the project's code) through a temporary sibling.
fn write(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let dir = file.parent().unwrap_or(Path::new("."));
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    let mut tmp = file.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let written = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .and_then(|mut out| out.write_all(bytes))
        .and_then(|()| std::fs::rename(&tmp, file));
    written.inspect_err(|_| drop(std::fs::remove_file(&tmp)))
}

/// The run key without the loaded files: `parts`, and the content of the `neon` files (relative to `root`) with their
/// includes and of Composer's lock file and installed packages.
pub(crate) fn run_key(root: &Path, neon: &[PathBuf], parts: &[&str]) -> String {
    let mut h = blake3::Hasher::new();
    for p in parts {
        part(&mut h, p.as_bytes());
    }
    let composer = ["composer.lock", "vendor/composer/installed.json"].map(|f| root.join(f));
    for file in neon_files(root, neon).iter().chain(&composer) {
        part(&mut h, file.as_os_str().as_encoded_bytes());
        part(&mut h, &std::fs::read(file).unwrap_or_default());
    }
    hex(&h)
}

/// The neon files `neon` (relative to `root`) and the files they include, recursively. An include with a parameter
/// (`%rootDir%/...`) is PHPStan's own, which the run key covers through Composer's lock file.
fn neon_files(root: &Path, neon: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    let mut pending: Vec<PathBuf> = neon.iter().map(|n| root.join(n)).collect();
    while let Some(file) = pending.pop() {
        if out.contains(&file) {
            continue;
        }
        let includes = (std::fs::read(&file).ok())
            .and_then(|bytes| pun_phpstan::neon::decode_bytes(&bytes).ok())
            .and_then(|v| v.as_array().and_then(|a| a.get_str("includes")).cloned());
        let dir = file.parent().unwrap_or(root).to_path_buf();
        out.push(file);
        let Some(includes) = includes.as_ref().and_then(|i| i.as_array()) else {
            continue;
        };
        pending.extend(
            (includes.values())
                .filter_map(|i| i.as_str())
                .filter(|i| !i.contains('%'))
                .map(|i| dir.join(i)),
        );
    }
    out
}
