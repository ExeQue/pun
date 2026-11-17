//! The shared package store, after uv, pnpm and Bun: each dist archive is extracted once into Pun's cache, keyed by
//! what identifies its contents (the dist's type, URL, reference and checksum, and the umask its files were created
//! under, since that sets their modes as it sets Composer's), and `vendor/` gets the files linked from there.
//!
//! The link mode decides how a file reaches `vendor/` (uv's `link-mode` names):
//! - `clone`, the default: a copy-on-write clone (`FICLONE` on Linux for btrfs, XFS and bcachefs; `clonefile` on
//!   macOS for APFS). The blocks are shared until either side writes, so an edit in `vendor/` stays in that
//!   project. Where the filesystem can't clone, files are copied.
//! - `hardlink`: one inode for the store and every project, so the second project costs no disk on any
//!   filesystem. Writing to a file in `vendor/` in place writes the store and every project linked to it, so it is
//!   opt-in, as `context/plan/packages.md` §7 decides.
//! - `copy`: an ordinary copy.
//!
//! Modes are copied from the store, which was extracted as `vendor/` was before the store, so `vendor/` keeps
//! Composer's modes in every link mode.

use crate::error::{Context, Result, bail};
use crate::lock::LockedPackage;
use crate::php::sha256;
use serde_json::Value;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkMode {
    #[default]
    Clone,
    Hardlink,
    Copy,
}

impl LinkMode {
    pub fn parse(name: &str) -> Result<LinkMode> {
        Ok(match name {
            "clone" => LinkMode::Clone,
            "hardlink" => LinkMode::Hardlink,
            "copy" => LinkMode::Copy,
            other => bail!("unknown link mode `{other}`: use clone, hardlink or copy"),
        })
    }
}

/// The process's umask, read once before any thread creates files.
static UMASK: LazyLock<u32> = LazyLock::new(umask);

#[cfg_attr(
    target_os = "linux",
    expect(clippy::useless_conversion, reason = "mode_t is u16 on macOS")
)]
fn umask() -> u32 {
    // SAFETY: umask only swaps the process's file mode mask; it is set back at once.
    let old = unsafe { libc::umask(0o022) };
    // SAFETY: as above.
    unsafe { libc::umask(old) };
    u32::from(old)
}

pub struct Store {
    root: PathBuf,
    mode: LinkMode,
    /// Set at the first clone the filesystem refuses; the rest are copied.
    no_clone: AtomicBool,
    counter: AtomicUsize,
}

impl Store {
    pub fn new(cache: &Path, mode: LinkMode) -> Store {
        LazyLock::force(&UMASK);
        Store {
            root: cache.join("store").join("v1"),
            mode,
            no_clone: AtomicBool::new(false),
            counter: AtomicUsize::new(0),
        }
    }

    /// The entry a package's dist archive extracts to; `None` for a dist that is not an archive.
    pub fn key(package: &LockedPackage) -> Option<String> {
        let dist = package.dist()?;
        let get = |k: &str| dist.get(k).and_then(Value::as_str).unwrap_or_default();
        if !matches!(get("type"), "zip" | "tar") || get("url").is_empty() {
            return None;
        }
        let id = format!(
            "{}\0{}\0{}\0{}\0{:o}",
            get("type"),
            get("url"),
            get("reference"),
            get("shasum"),
            *UMASK
        );
        Some(sha256(id.as_bytes()))
    }

    /// Links the entry `key` into `target`, extracting it first with `fill` (which unpacks into the directory it is
    /// given) when the store does not have it. True when the store had it.
    pub fn place(
        &self,
        key: &str,
        target: &Path,
        fill: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<bool> {
        let entry = self.root.join(&key[..2]).join(key);
        let hit = entry.is_dir();
        if !hit {
            std::fs::create_dir_all(entry.parent().unwrap_or(&self.root))?;
            let tmp = self.temp(&self.root);
            let filled = fill(&tmp);
            if let Err(e) = filled {
                let _ = std::fs::remove_dir_all(&tmp);
                return Err(e);
            }
            // Another process may have stored it meanwhile; either copy is the same.
            if std::fs::rename(&tmp, &entry).is_err() {
                let _ = std::fs::remove_dir_all(&tmp);
            }
        }
        let parent = target.parent().context("target without a parent")?;
        std::fs::create_dir_all(parent)?;
        let tmp = self.temp(parent);
        let linked = self.link_tree(&entry, &tmp);
        if let Err(e) = linked {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e.into());
        }
        crate::fetch::remove(target)?;
        std::fs::rename(&tmp, target)?;
        Ok(hit)
    }

    fn temp(&self, dir: &Path) -> PathBuf {
        let n = self.counter.fetch_add(1, Ordering::Relaxed);
        dir.join(format!(".pun-{}-{n}.tmp", std::process::id()))
    }

    fn link_tree(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        std::fs::create_dir(to)?;
        same_mode(&std::fs::metadata(from)?, to)?;
        for e in std::fs::read_dir(from)? {
            let e = e?;
            let (src, dst) = (e.path(), to.join(e.file_name()));
            let kind = e.file_type()?;
            if kind.is_dir() {
                self.link_tree(&src, &dst)?;
            } else if kind.is_symlink() {
                std::os::unix::fs::symlink(std::fs::read_link(&src)?, &dst)?;
            } else {
                self.link_file(&src, &dst)?;
            }
        }
        Ok(())
    }

    fn link_file(&self, src: &Path, dst: &Path) -> std::io::Result<()> {
        match self.mode {
            // Across filesystems (or past the link limit) a hardlink can't be made; the file is copied.
            LinkMode::Hardlink if std::fs::hard_link(src, dst).is_ok() => return Ok(()),
            LinkMode::Clone if !self.no_clone.load(Ordering::Relaxed) => match clone(src, dst) {
                Ok(()) => return same_mode(&std::fs::metadata(src)?, dst),
                Err(_) => self.no_clone.store(true, Ordering::Relaxed),
            },
            _ => {}
        }
        std::fs::copy(src, dst).map(|_| ())
    }
}

fn same_mode(meta: &std::fs::Metadata, path: &Path) -> std::io::Result<()> {
    let want = meta.mode() & 0o7777;
    if std::fs::metadata(path)?.mode() & 0o7777 != want {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(want))?;
    }
    Ok(())
}

/// A copy-on-write clone of `src` at `dst`, or the error of a filesystem that can't make one.
#[cfg(target_os = "linux")]
fn clone(src: &Path, dst: &Path) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let from = std::fs::File::open(src)?;
    let to = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)?;
    // SAFETY: both descriptors are open for the duration of the call.
    if unsafe { libc::ioctl(to.as_raw_fd(), libc::FICLONE, from.as_raw_fd()) } == 0 {
        return Ok(());
    }
    let error = std::io::Error::last_os_error();
    drop(to);
    std::fs::remove_file(dst)?;
    Err(error)
}

#[cfg(target_os = "macos")]
fn clone(src: &Path, dst: &Path) -> std::io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let c =
        |p: &Path| std::ffi::CString::new(p.as_os_str().as_bytes()).map_err(std::io::Error::other);
    let (s, d) = (c(src)?, c(dst)?);
    // SAFETY: both are valid NUL-terminated paths.
    if unsafe { libc::clonefile(s.as_ptr(), d.as_ptr(), libc::CLONE_NOFOLLOW) } == 0 {
        return Ok(());
    }
    Err(std::io::Error::last_os_error())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn package(dist: &serde_json::Value) -> LockedPackage {
        LockedPackage(
            json!({"name": "a/b", "version": "1.0.0", "dist": dist})
                .as_object()
                .unwrap()
                .clone(),
        )
    }

    #[test]
    fn keys_name_the_archive() {
        let zip = package(
            &json!({"type": "zip", "url": "https://x/a.zip", "reference": "abc", "shasum": ""}),
        );
        let other = package(
            &json!({"type": "zip", "url": "https://x/a.zip", "reference": "abd", "shasum": ""}),
        );
        assert_eq!(Store::key(&zip).unwrap().len(), 64);
        assert_ne!(Store::key(&zip), Store::key(&other));
        assert_eq!(
            Store::key(&package(&json!({"type": "path", "url": "../p"}))),
            None
        );
        assert_eq!(Store::key(&package(&json!({"type": "tar"}))), None);
        assert_eq!(LinkMode::parse("hardlink").unwrap(), LinkMode::Hardlink);
        assert_eq!(LinkMode::parse("copy").unwrap(), LinkMode::Copy);
        assert_eq!(LinkMode::parse("clone").unwrap(), LinkMode::default());
        assert_eq!(
            LinkMode::parse("symlink").unwrap_err().to_string(),
            "unknown link mode `symlink`: use clone, hardlink or copy"
        );
    }

    /// Each mode gives the same files, modes and symlinks; only hardlinks share the store's inodes, and a failed
    /// fill leaves neither an entry nor a target behind.
    #[test]
    fn every_mode_links_the_same_tree() {
        let t = std::env::temp_dir().join(format!("pun-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        let fill = |dir: &Path| -> Result<()> {
            std::fs::create_dir_all(dir.join("bin"))?;
            std::fs::write(dir.join("bin/tool"), "#!/bin/sh\n")?;
            std::fs::set_permissions(dir.join("bin/tool"), std::fs::Permissions::from_mode(0o755))?;
            std::fs::write(dir.join("README"), "hi")?;
            std::os::unix::fs::symlink("README", dir.join("link"))?;
            Ok(())
        };
        let mut filled = 0;
        for mode in [LinkMode::Clone, LinkMode::Hardlink, LinkMode::Copy] {
            let store = Store::new(&t.join("cache"), mode);
            let target = t.join(format!("{mode:?}/vendor/a/b"));
            std::fs::create_dir_all(&target).unwrap();
            std::fs::write(target.join("stale"), "").unwrap();
            let hit = store
                .place("ab12", &target, |d| {
                    filled += 1;
                    fill(d)
                })
                .unwrap();
            assert_eq!(hit, mode != LinkMode::Clone, "{mode:?}");
            assert!(!target.join("stale").exists());
            let meta = |p: &str| std::fs::metadata(target.join(p)).unwrap();
            assert_eq!(meta("bin/tool").mode() & 0o777, 0o755);
            assert_eq!(
                std::fs::read_link(target.join("link")).unwrap(),
                Path::new("README")
            );
            assert_eq!(
                std::fs::read_to_string(target.join("README")).unwrap(),
                "hi"
            );
            assert_eq!(
                meta("README").nlink() > 1,
                mode == LinkMode::Hardlink,
                "{mode:?}"
            );
        }
        assert_eq!(filled, 1);
        let store = Store::new(&t.join("cache"), LinkMode::Copy);
        let target = t.join("failed/a/b");
        let err = store
            .place("cd34", &target, |_| bail!("no archive"))
            .unwrap_err();
        assert_eq!(err.to_string(), "no archive");
        assert!(!target.exists() && !t.join("cache/store/v1/cd/cd34").exists());
        std::fs::remove_dir_all(&t).unwrap();
    }
}
