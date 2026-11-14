//! `pun install`: installs the locked dependencies into `vendor/`. The manifest comes from pun.toml's dependency
//! sections, or composer.json in a project not migrated yet; the lock from pun.lock, or composer.lock (imported into
//! pun.lock when pun.toml holds the dependencies). Without a lock, install resolves one first, as `pun update` does.

use crate::render::{ColorChoice, Look, Role};
use anyhow::{Context, Result, bail};
use pun_packages::fetch::{Auth, Fetcher, composer_dirs};
use pun_packages::install::{Options, dirs, install};
use pun_packages::lock::Lock;
use pun_packages::scripts::Runner;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(clap::Args, Clone, Default)]
pub(crate) struct InstallArgs {
    /// Leave out the dev dependencies (`require-dev`).
    #[arg(long)]
    pub(crate) no_dev: bool,
    /// Put the PSR-0 and PSR-4 classes in the class map too.
    #[arg(short = 'o', long)]
    pub(crate) optimize_autoloader: bool,
    /// Load classes from the class map only (implies --optimize-autoloader).
    #[arg(short = 'a', long)]
    pub(crate) classmap_authoritative: bool,
    /// Cache found and missing classes in APCu.
    #[arg(long)]
    pub(crate) apcu_autoloader: bool,
    /// The APCu cache prefix (implies --apcu-autoloader); without it each dump picks a random one.
    #[arg(long, value_name = "PREFIX")]
    pub(crate) apcu_autoloader_prefix: Option<String>,
    /// Skip writing the autoloader.
    #[arg(long)]
    pub(crate) no_autoloader: bool,
    /// Skip the root package's scripts.
    #[arg(long)]
    pub(crate) no_scripts: bool,
    /// Skip the Composer plugins Pun supports.
    #[arg(long)]
    pub(crate) no_plugins: bool,
    /// Leave this platform requirement out of the platform check (`php`, `ext-*`, globs with `*`); repeatable.
    #[arg(long, value_name = "REQ")]
    pub(crate) ignore_platform_req: Vec<String>,
    /// Leave every platform requirement out of the platform check.
    #[arg(long)]
    pub(crate) ignore_platform_reqs: bool,
    /// Run in this directory instead of the current one.
    #[arg(short = 'd', long, value_name = "DIR")]
    pub(crate) working_dir: Option<PathBuf>,
    /// Accepted for Composer's CI flags; Pun never asks and always installs from dists.
    #[arg(short = 'n', long, hide = true)]
    pub(crate) no_interaction: bool,
    #[arg(long, hide = true)]
    pub(crate) prefer_dist: bool,
    #[arg(long, hide = true)]
    pub(crate) no_progress: bool,
}

/// A project's dependencies: the manifest in composer.json's shape, and its lock when it has one.
pub(crate) struct Project {
    pub(crate) root: PathBuf,
    pub(crate) manifest: Map<String, Value>,
    /// The dependencies live in pun.toml, so the lock is pun.lock.
    pub(crate) pun_toml: bool,
    pub(crate) lock: Option<Lock>,
    /// The file the lock was read from.
    pub(crate) lock_name: &'static str,
    pub(crate) notes: Vec<String>,
}

impl Project {
    /// The lock file an update writes: pun.lock beside pun.toml, or beside a composer.json that already has one;
    /// composer.lock otherwise.
    pub(crate) fn lock_file(&self) -> &'static str {
        if self.pun_toml || self.lock_name == "pun.lock" {
            "pun.lock"
        } else {
            "composer.lock"
        }
    }
}

/// The downloader for a project: Composer's cache, the project's and the global auth.json, and `secure-http`.
pub(crate) fn fetcher(root: &Path, manifest: &Map<String, Value>) -> Fetcher {
    let secure_http =
        manifest.get("config").and_then(|c| c.get("secure-http")) != Some(&Value::Bool(false));
    let (home, cache) = composer_dirs().unzip();
    Fetcher::new(
        cache,
        Auth::load(root, home.as_deref(), std::env::var("COMPOSER_AUTH").ok()),
        secure_http,
    )
}

/// The project in `working_dir` (or the current directory). With `create`, a directory without dependencies is a
/// new project whose dependencies go in pun.toml.
pub(crate) fn project(working_dir: Option<&PathBuf>, create: bool) -> Result<Project> {
    let root = match working_dir {
        Some(d) => d
            .canonicalize()
            .with_context(|| format!("can't open {}", d.display()))?,
        None => crate::root()?,
    };
    let mut notes = vec![];
    let composer_lock = |root: &PathBuf| -> Result<Option<Lock>> {
        match std::fs::read_to_string(root.join("composer.lock")) {
            Err(_) => Ok(None),
            Ok(text) => Lock::from_composer_lock(&text)
                .context("invalid composer.lock")
                .map(Some),
        }
    };
    let (manifest, pun_toml, lock, lock_name) = match pun_config::manifest::read(&root)? {
        Some(m) => match Lock::read(&root)? {
            Some(lock) => (m.json, true, Some(lock), "pun.lock"),
            None => match composer_lock(&root)? {
                Some(lock) => {
                    std::fs::write(root.join("pun.lock"), lock.to_toml())
                        .context("can't write pun.lock")?;
                    notes.push("wrote pun.lock from composer.lock".to_string());
                    (m.json, true, Some(lock), "pun.lock")
                }
                None => (m.json, true, None, "pun.lock"),
            },
        },
        None => {
            let Ok(text) = std::fs::read_to_string(root.join("composer.json")) else {
                if create {
                    return Ok(Project {
                        root,
                        manifest: Map::new(),
                        pun_toml: true,
                        lock: None,
                        lock_name: "pun.lock",
                        notes,
                    });
                }
                bail!(
                    "no dependencies here: pun.toml has no [require] and there is no composer.json"
                );
            };
            let json: Map<String, Value> =
                serde_json::from_str(&text).context("invalid composer.json")?;
            match Lock::read(&root)? {
                Some(lock) => (json, false, Some(lock), "pun.lock"),
                None => {
                    let lock = composer_lock(&root)?;
                    if lock.is_some() {
                        notes.push(
                            "run `pun migrate` to move the dependencies into pun.toml and pun.lock"
                                .to_string(),
                        );
                    }
                    (json, false, lock, "composer.lock")
                }
            }
        }
    };
    Ok(Project {
        root,
        manifest,
        pun_toml,
        lock,
        lock_name,
        notes,
    })
}

pub(crate) fn run(a: InstallArgs) -> Result<u8> {
    let started = Instant::now();
    let mut project = project(a.working_dir.as_ref(), false)?;
    if project.lock.is_none() {
        eprintln!(
            "No {} yet: resolving the dependencies to their latest versions",
            project.lock_file()
        );
        return crate::update::update_and_install(
            &mut project,
            crate::update::Plan::default(),
            &a,
            started,
        );
    }
    let lock = project.lock.take().expect("a lock");
    install_locked(&project, &lock, project.lock_name, &a, started, false)
}

/// Installs `lock` into the project's `vendor/` and reports it; `update` dispatches the update events instead of
/// the install ones.
pub(crate) fn install_locked(
    project: &Project,
    lock: &Lock,
    lock_name: &str,
    a: &InstallArgs,
    started: Instant,
    update: bool,
) -> Result<u8> {
    let (root, manifest) = (&project.root, &project.manifest);
    let look = Look::stderr(ColorChoice::Auto);
    let (pre, post) = if update {
        ("pre-update-cmd", "post-update-cmd")
    } else {
        ("pre-install-cmd", "post-install-cmd")
    };
    let options = Options {
        no_dev: a.no_dev,
        optimize: a.optimize_autoloader,
        authoritative: a.classmap_authoritative,
        apcu: a.apcu_autoloader,
        apcu_prefix: a.apcu_autoloader_prefix.clone(),
        no_autoloader: a.no_autoloader,
        no_plugins: a.no_plugins,
        ignore_platform: a.ignore_platform_req.clone(),
        ignore_all_platform: a.ignore_platform_reqs,
    };
    let fetcher = fetcher(root, manifest);
    let (vendor, bin_dir) = dirs(&root.to_string_lossy(), manifest);
    let mut scripts = Runner::new(root, &vendor, &bin_dir, manifest, !a.no_dev);
    let total = lock.packages.len() + if a.no_dev { 0 } else { lock.packages_dev.len() };
    eprintln!(
        "Installing {} from {lock_name}",
        crate::agent::count(total, "package")
    );
    if !a.no_scripts && !update {
        scripts.dispatch(pre, &[])?;
    }
    let mut events = |event: &str| {
        if a.no_scripts {
            Ok(())
        } else {
            scripts.dispatch(event, &[])
        }
    };
    let report = install(root, manifest, lock, &fetcher, &options, &mut events)?;
    drop(events);
    if !a.no_scripts {
        scripts.dispatch(post, &[])?;
    }

    let ok = look.paint(Role::Ok, look.glyph("✔", "+"));
    let warn = look.paint(Role::Warning, look.glyph("⚠", "!"));
    if report.stale_lock {
        eprintln!(
            " {warn} {lock_name} is out of date with the dependencies it locks: they changed since it was written, so run `pun update` to refresh it"
        );
    }
    let mut changes = vec![];
    for (what, n) in [
        ("installed", report.installed.len()),
        ("updated", report.updated.len()),
        ("removed", report.removed.len()),
    ] {
        if n > 0 {
            changes.push(format!("{n} {what}"));
        }
    }
    if report.kept > 0 {
        changes.push(format!("{} up to date", report.kept));
    }
    let cached = if report.cached > 0 {
        format!(" ({} from the cache)", report.cached)
    } else {
        String::new()
    };
    eprintln!(
        " {ok} packages: {}{cached}",
        if changes.is_empty() {
            "none".into()
        } else {
            changes.join(", ")
        }
    );
    if !a.no_autoloader {
        let noun = if report.classes == 1 {
            "class"
        } else {
            "classes"
        };
        eprintln!(" {ok} autoloader: {} {noun}", report.classes);
    }
    for w in report.warnings.iter().chain(&project.notes) {
        eprintln!(" {warn} {w}");
    }
    let secs = started.elapsed().as_secs_f64();
    eprintln!(
        "{}",
        look.paint(Role::Summary, &format!("Installed in {secs:.1}s"))
    );
    Ok(0)
}
