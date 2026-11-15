//! The result cache of `pun format`, `lint` and `check`: the files whose last check found nothing to change and no
//! problem, so the next run with the same key skips them. One file per project root under `<cache dir>/results/`.
//!
//! An entry's key covers the run (the binary, the command, the flags and the resolved config, `Key`), the file's
//! path and content, and, when linting the file read the project's codebase, a fingerprint of the project.
//!
//! The file: `MAGIC`, the BLAKE3 hash of the rest, then sections, one per run key, most recent first. A section is
//! the run key's digest, its body's length and the body: the entry count, then per entry the path's length and
//! bytes, the entry's digest and whether it reads the project. A file that is missing, truncated, from another
//! version or otherwise unreadable is an empty cache.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const MAGIC: &[u8; 8] = b"PUNRC\0\0\x01";
/// The run keys a file keeps (format, lint and check, with and without `--fix`, and a few others).
const RUNS: usize = 8;

pub(crate) type Digest = [u8; 16];

fn digest(h: &blake3::Hasher) -> Digest {
    let mut out = [0; 16];
    out.copy_from_slice(&h.finalize().as_bytes()[..16]);
    out
}

/// The BLAKE3 hash of `bytes`, cut to 128 bits.
pub(crate) fn hash(bytes: &[u8]) -> Digest {
    let mut h = blake3::Hasher::new();
    h.update(bytes);
    digest(&h)
}

/// A key built from labelled parts, each length-prefixed so two different sequences never hash alike.
#[derive(Default)]
pub(crate) struct Key(blake3::Hasher);

impl Key {
    pub(crate) fn add(&mut self, label: &str, value: impl AsRef<[u8]>) -> &mut Self {
        for part in [label.as_bytes(), value.as_ref()] {
            self.0.update(&(part.len() as u64).to_le_bytes());
            self.0.update(part);
        }
        self
    }

    /// A file's size and modification time, or that it is missing.
    pub(crate) fn stat(&mut self, path: &Path) -> &mut Self {
        let label = path.to_string_lossy();
        self.add(&label, stat(path))
    }

    pub(crate) fn digest(&self) -> Digest {
        digest(&self.0)
    }
}

/// A file's size and modification time in nanoseconds, or nothing when it is missing.
pub(crate) fn stat(path: &Path) -> Vec<u8> {
    let Ok(m) = std::fs::metadata(path) else {
        return vec![];
    };
    let mtime = (m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    [m.len().to_le_bytes().as_slice(), &mtime.to_le_bytes()].concat()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Entry {
    digest: Digest,
    project: bool,
}

pub(crate) struct Cache {
    file: PathBuf,
    root: PathBuf,
    run: Digest,
    /// The project's fingerprint, when the run lints with rules that can read the project's codebase.
    project: Option<Digest>,
    old: HashMap<String, Entry>,
    /// This run key's section as read, and whether it came first.
    old_body: Option<(Vec<u8>, bool)>,
    /// The other run keys' sections, most recent first.
    others: Vec<(Digest, Vec<u8>)>,
    clean: Mutex<Vec<(String, Entry)>>,
}

impl Cache {
    /// The cache of `root` for the run key `run`, read from `file`.
    pub(crate) fn open(file: PathBuf, root: &Path, run: Digest, project: Option<Digest>) -> Cache {
        let mut cache = Cache {
            file,
            root: root.to_path_buf(),
            run,
            project,
            old: HashMap::new(),
            old_body: None,
            others: vec![],
            clean: Mutex::new(vec![]),
        };
        let sections = std::fs::read(&cache.file)
            .ok()
            .and_then(|bytes| sections(&bytes))
            .unwrap_or_default();
        for (i, (key, body)) in sections.into_iter().enumerate() {
            if key != run {
                cache.others.push((key, body));
            } else if let Some(entries) = entries(&body) {
                cache.old = entries;
                cache.old_body = Some((body, i == 0));
            }
        }
        cache
    }

    /// The default cache file of the project at `root`: `<cache dir>/results/<hash of the root>.bin`.
    pub(crate) fn default_file(root: &Path) -> Option<PathBuf> {
        let name = blake3::hash(root.as_os_str().as_encoded_bytes()).to_hex();
        let name = &name[..32];
        let dir = crate::x::cache_dir().ok()?;
        Some(dir.join("results").join(format!("{name}.bin")))
    }

    fn entry_digest(&self, rel: &str, content: &Digest, project: bool) -> Option<Digest> {
        let mut key = Key::default();
        key.add("run", self.run)
            .add("path", rel)
            .add("content", content);
        if project {
            key.add("project", self.project?);
        }
        Some(key.digest())
    }

    /// Whether `rel` with this content was clean in an earlier run with the same key; a hit stays in the cache.
    pub(crate) fn hit(&self, rel: &str, content: &Digest) -> bool {
        let Some(e) = self.old.get(rel) else {
            return false;
        };
        let hit = self.entry_digest(rel, content, e.project) == Some(e.digest);
        if hit {
            (self.clean.lock().expect("the cache's entries")).push((rel.to_string(), *e));
        }
        hit
    }

    /// Records that `rel` with this content was clean; `project` when linting it read the project's codebase.
    pub(crate) fn keep(&self, rel: &str, content: &Digest, project: bool) {
        if let Some(digest) = self.entry_digest(rel, content, project) {
            let entry = Entry { digest, project };
            (self.clean.lock().expect("the cache's entries")).push((rel.to_string(), entry));
        }
    }

    /// Writes the entries of this run (and the earlier entries of files it didn't check that still exist) through
    /// a temporary file, so a concurrent run reads the old cache or the new one, never a mix.
    pub(crate) fn save(self, checked: &HashSet<&str>) {
        let mut entries: HashMap<String, Entry> =
            (self.clean.into_inner().expect("the cache's entries"))
                .into_iter()
                .collect();
        for (rel, e) in self.old {
            if !checked.contains(rel.as_str())
                && !entries.contains_key(&rel)
                && self.root.join(&rel).exists()
            {
                entries.insert(rel, e);
            }
        }
        if entries.is_empty() && self.old_body.is_none() {
            return;
        }
        let mut entries: Vec<_> = entries.into_iter().collect();
        entries.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        let mut body = (entries.len() as u64).to_le_bytes().to_vec();
        for (rel, e) in &entries {
            body.extend_from_slice(&(rel.len() as u64).to_le_bytes());
            body.extend_from_slice(rel.as_bytes());
            body.extend_from_slice(&e.digest);
            body.push(u8::from(e.project));
        }
        if self.old_body == Some((body.clone(), true)) {
            return;
        }
        let mut rest = vec![];
        let others = self.others.into_iter().take(RUNS - 1);
        for (key, body) in std::iter::once((self.run, body)).chain(others) {
            rest.extend_from_slice(&key);
            rest.extend_from_slice(&(body.len() as u64).to_le_bytes());
            rest.extend_from_slice(&body);
        }
        let bytes = [MAGIC.as_slice(), &hash(&rest), &rest].concat();
        let _ = write(&self.file, &bytes);
    }
}

fn write(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = file.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = file.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes)
        .and_then(|()| std::fs::rename(&tmp, file))
        .inspect_err(|_| drop(std::fs::remove_file(&tmp)))
}

/// Reads `n` bytes off the front of `bytes`.
fn take<'a>(bytes: &mut &'a [u8], n: usize) -> Option<&'a [u8]> {
    let (head, tail) = bytes.split_at_checked(n)?;
    *bytes = tail;
    Some(head)
}

fn len(bytes: &mut &[u8]) -> Option<usize> {
    let n = u64::from_le_bytes(take(bytes, 8)?.try_into().ok()?);
    usize::try_from(n).ok()
}

fn sections(bytes: &[u8]) -> Option<Vec<(Digest, Vec<u8>)>> {
    let mut bytes = bytes.strip_prefix(MAGIC)?;
    let sum = take(&mut bytes, 16)?;
    if sum != hash(bytes) {
        return None;
    }
    let mut out = vec![];
    while !bytes.is_empty() {
        let key = take(&mut bytes, 16)?.try_into().ok()?;
        let n = len(&mut bytes)?;
        out.push((key, take(&mut bytes, n)?.to_vec()));
    }
    Some(out)
}

fn entries(mut body: &[u8]) -> Option<HashMap<String, Entry>> {
    let count = len(&mut body)?;
    let mut out = HashMap::with_capacity(count.min(body.len()));
    for _ in 0..count {
        let n = len(&mut body)?;
        let rel = String::from_utf8(take(&mut body, n)?.to_vec()).ok()?;
        let digest = take(&mut body, 16)?.try_into().ok()?;
        let project = take(&mut body, 1)?[0] == 1;
        out.insert(rel, Entry { digest, project });
    }
    body.is_empty().then_some(out)
}

#[cfg(test)]
mod tests {
    use super::{Cache, Key, MAGIC, hash};
    use std::collections::HashSet;

    fn dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("pun-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// An entry is found only under the run key, path, content and project fingerprint it was kept with; a damaged
    /// file is an empty cache, and the other run keys' sections survive a save.
    #[test]
    fn keys_entries_on_everything() {
        let d = dir("keys");
        let file = d.join("results/x.bin");
        std::fs::write(d.join("a.php"), "<?php\n").unwrap();
        std::fs::write(d.join("b.php"), "<?php\n").unwrap();
        let (a, b) = (hash(b"a"), hash(b"b"));
        let run = Key::default().add("stage", "format").digest();
        let other = Key::default().add("stage", "lint").digest();
        let project = Some(hash(b"project"));

        let c = Cache::open(file.clone(), &d, run, project);
        assert!(!c.hit("a.php", &a));
        c.keep("a.php", &a, false);
        c.keep("b.php", &b, true);
        c.save(&HashSet::from(["a.php", "b.php"]));

        let c = Cache::open(file.clone(), &d, run, project);
        assert!(c.hit("a.php", &a) && c.hit("b.php", &b));
        c.save(&HashSet::from(["a.php", "b.php"]));

        let c = Cache::open(file.clone(), &d, run, project);
        assert!(c.hit("a.php", &a), "a hit stays");
        assert!(!c.hit("a.php", &b), "content");
        assert!(!c.hit("b.php", &a), "path");
        assert!(c.hit("b.php", &b));
        assert!(
            !Cache::open(file.clone(), &d, other, project).hit("a.php", &a),
            "run"
        );
        let changed = Cache::open(file.clone(), &d, run, Some(hash(b"changed")));
        assert!(
            changed.hit("a.php", &a) && !changed.hit("b.php", &b),
            "project"
        );
        assert!(!Cache::open(file.clone(), &d, run, None).hit("b.php", &b));

        // A save under another key keeps this one; a file this run didn't check stays while it exists.
        let c = Cache::open(file.clone(), &d, other, None);
        c.keep("a.php", &a, false);
        c.save(&HashSet::new());
        Cache::open(file.clone(), &d, run, project).save(&HashSet::new());
        let c = Cache::open(file.clone(), &d, run, project);
        assert!(c.hit("a.php", &a) && c.hit("b.php", &b));
        std::fs::remove_file(d.join("b.php")).unwrap();
        let c = Cache::open(file.clone(), &d, run, project);
        assert!(c.hit("a.php", &a));
        c.save(&HashSet::from(["a.php"]));
        let c = Cache::open(file.clone(), &d, run, project);
        assert!(
            c.hit("a.php", &a) && !c.hit("b.php", &b),
            "dropped when gone"
        );
        // Checked and not clean any more: dropped.
        Cache::open(file.clone(), &d, run, project).save(&HashSet::from(["a.php"]));
        assert!(!Cache::open(file.clone(), &d, run, project).hit("a.php", &a));
        assert!(Cache::open(file.clone(), &d, other, None).hit("a.php", &a));

        let bytes = std::fs::read(&file).unwrap();
        for damaged in [
            bytes[..bytes.len() - 1].to_vec(),
            [b"PUNRC\0\0\x00".as_slice(), &bytes[MAGIC.len()..]].concat(),
            [&bytes[..bytes.len() - 1], b"x"].concat(),
        ] {
            std::fs::write(&file, damaged).unwrap();
            assert!(!Cache::open(file.clone(), &d, other, None).hit("a.php", &a));
        }
        std::fs::remove_dir_all(&d).unwrap();
    }
}
