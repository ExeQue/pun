#![allow(dead_code, reason = "each test binary uses some of the helpers")]
pub(crate) mod prettier;
pub(crate) mod pty;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(crate) const PUN: &str = env!("CARGO_BIN_EXE_pun");

/// The pinned Pint (`scripts/oracle.sh`), canonical so paths in its output match; `None` skips the Pint cases.
pub(crate) fn oracle() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.oracle/pint/builds/pint");
    p.exists().then(|| p.canonicalize().unwrap())
}

pub(crate) fn scratch(prefix: &str, name: &str) -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!(
        "pun-{prefix}-{name}-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

pub(crate) fn write_files(dir: &Path, files: &[(&str, &str)]) {
    for (f, c) in files {
        let p = dir.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, c).unwrap();
    }
}

pub(crate) fn project(prefix: &str, name: &str, files: &[(&str, &str)]) -> PathBuf {
    let d = scratch(prefix, name);
    write_files(&d, files);
    d
}

pub(crate) fn read(d: &Path, f: &str) -> String {
    std::fs::read_to_string(d.join(f)).unwrap()
}

pub(crate) fn snapshot(dir: &Path, skip: impl Fn(&str) -> bool) -> BTreeMap<String, Vec<u8>> {
    fn walk(
        dir: &Path,
        rel: &str,
        skip: &dyn Fn(&str) -> bool,
        out: &mut BTreeMap<String, Vec<u8>>,
    ) {
        for e in std::fs::read_dir(dir.join(rel)).unwrap().flatten() {
            let r = if rel.is_empty() {
                e.file_name().to_string_lossy().into_owned()
            } else {
                format!("{rel}/{}", e.file_name().to_string_lossy())
            };
            if skip(&r) || e.path().is_symlink() {
                continue;
            }
            if e.path().is_dir() {
                walk(dir, &r, skip, out);
            } else {
                let content = std::fs::read(e.path()).unwrap_or_else(|_| b"<unreadable>".to_vec());
                out.insert(r, content);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, "", &skip, &mut out);
    out
}

/// A command with only the `keep` variables (and coverage's), and a cache directory of its test's own: the result
/// cache keeps a file per project root, so no test shares cached results with another test or with the user.
pub(crate) fn clean_env<'c>(c: &'c mut Command, keep: &[&str]) -> &'c mut Command {
    c.env_clear();
    for k in keep.iter().chain(&["LLVM_PROFILE_FILE"]) {
        if let Ok(v) = std::env::var(k) {
            c.env(k, v);
        }
    }
    c.env("PUN_CACHE_DIR", test_cache_dir())
}

/// The cache directory of the running test: one per process and thread, so per test under nextest and cargo test.
pub(crate) fn test_cache_dir() -> PathBuf {
    let thread = format!("{:?}", std::thread::current().id());
    let thread: String = thread.chars().filter(char::is_ascii_digit).collect();
    std::env::temp_dir().join(format!("pun-test-cache-{}-{thread}", std::process::id()))
}

pub(crate) fn migrate(dir: &Path) {
    let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
        .arg("migrate")
        .env("PUN_AGENT", "0")
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

pub(crate) fn human(args: &[&str]) -> bool {
    !args.iter().any(|a| {
        a.starts_with("--format")
            || a.starts_with("--stdin")
            || *a == "--quiet"
            || *a == "-"
            || (a.starts_with('-') && !a.starts_with("--") && a.contains('q'))
    })
}

fn pun_summary(line: &str) -> bool {
    let time = line.strip_suffix('s').and_then(|l| l.rsplit(' ').next());
    time.is_some_and(|t| {
        t.len() >= 4 && t.as_bytes()[t.len() - 3] == b'.' && t.parse::<f64>().is_ok()
    })
}

pub(crate) fn human_pair<'a>(
    pint: &str,
    pun: &str,
    pint_files: impl IntoIterator<Item = &'a String>,
    pun_files: impl IntoIterator<Item = &'a String>,
) -> (String, String) {
    let plain = strip_ansi(pun);
    let Some(end) = plain
        .lines()
        .collect::<Vec<_>>()
        .into_iter()
        .rposition(pun_summary)
    else {
        return (pint.to_string(), pun.to_string());
    };
    if !pint.contains("────") {
        return (pint.to_string(), pun.to_string());
    }
    let tail: String = pun.split_inclusive('\n').skip(end + 1).collect();
    let pint_tail = if pint.ends_with(&tail) {
        tail.clone()
    } else {
        "(Pint's output ends differently)".to_string()
    };
    let head: Vec<&str> = plain.lines().take(end).collect();
    (
        format!("{}\n{pint_tail}", named(&strip_ansi(pint), pint_files)),
        format!("{}\n{tail}", named(&head.join("\n"), pun_files)),
    )
}

fn named<'a>(text: &str, files: impl IntoIterator<Item = &'a String>) -> String {
    let lines: Vec<&str> = (text.lines())
        .filter(|l| {
            let bare = l.strip_prefix(" ✔ ").or_else(|| l.strip_prefix(" ok "));
            bare.is_none_or(|rest| rest.contains("  "))
        })
        .collect();
    let named: Vec<&str> = (files.into_iter())
        .filter(|rel| {
            lines.iter().any(|l| {
                [" ", "\n", ":"]
                    .iter()
                    .any(|end| format!("{l}\n").contains(&format!(" {rel}{end}")))
            })
        })
        .map(String::as_str)
        .collect();
    format!("named: {}", named.join(", "))
}

pub(crate) fn strip_ansi(out: &str) -> String {
    let mut plain = String::new();
    let mut chars = out.chars();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            plain.push(c);
        } else if chars.next() == Some('[') {
            for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            }
        }
    }
    plain
}

/// A scratch tree holding `files`, each with `content`, for the finder tests.
pub(crate) fn tree(prefix: &str, files: &[&str], content: &str) -> PathBuf {
    let root = scratch(prefix, "tree");
    for f in files {
        let p = root.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, content).unwrap();
    }
    root
}
