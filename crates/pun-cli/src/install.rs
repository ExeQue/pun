use crate::render::{ColorChoice, Look, Role};
use anyhow::{Context, Result, bail};
use pun_packages::autoload::{Apcu, Scan};
use pun_packages::fetch::{Auth, Fetcher, composer_dirs};
use pun_packages::install::{Dirs, Options, dirs, install};
use pun_packages::lock::Lock;
use pun_packages::scripts::Runner;
use pun_packages::version::IgnorePlatform;
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
}

/// A project's dependencies: pun.toml's dependency sections in composer.json's shape, and pun.lock when it exists.
pub(crate) struct Project {
    pub(crate) root: PathBuf,
    pub(crate) manifest: Map<String, Value>,
    pub(crate) lock: Option<Lock>,
    /// The workspace the project is the root of; `manifest` then holds the members too.
    pub(crate) workspace: Option<crate::workspace::Workspace>,
    /// The directory the command started in: the root, or a directory below it.
    pub(crate) here: PathBuf,
}

/// What a project not migrated yet is told.
pub(crate) const MIGRATE: &str =
    "run `pun migrate` to move composer.json and composer.lock into pun.toml and pun.lock";

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
/// new project whose dependencies go in pun.toml. In a workspace member, the project is the workspace's root.
pub(crate) fn project(working_dir: Option<&PathBuf>, create: bool) -> Result<Project> {
    let here = match working_dir {
        Some(d) => d
            .canonicalize()
            .with_context(|| format!("can't open {}", d.display()))?,
        None => crate::root()?,
    };
    let workspace = crate::workspace::find(&here)?;
    let root = workspace
        .as_ref()
        .map_or_else(|| here.clone(), |w| w.root.clone());
    let manifest = match pun_config::manifest::read(&root)? {
        Some(m) => m.json,
        None if workspace.is_some() => Map::new(),
        None if root.join("composer.json").is_file() => {
            bail!("the dependencies are still in composer.json: {MIGRATE}")
        }
        None if create => Map::new(),
        None => bail!(
            "no dependencies here: pun.toml has no [require]; add one with `pun require <package>`"
        ),
    };
    let lock = Lock::read(&root)?;
    if lock.is_none() && root.join("composer.lock").is_file() {
        bail!("there is a composer.lock but no pun.lock: {MIGRATE}");
    }
    let manifest = match &workspace {
        Some(w) => w.manifest(manifest),
        None => manifest,
    };
    Ok(Project {
        root,
        manifest,
        lock,
        workspace,
        here,
    })
}

pub(crate) fn run(a: InstallArgs) -> Result<u8> {
    let started = Instant::now();
    let mut project = project(a.working_dir.as_ref(), false)?;
    if project.lock.is_none() {
        eprintln!("No pun.lock yet: resolving the dependencies to their latest versions");
        return crate::update::update_and_install(
            &mut project,
            crate::update::Plan::default(),
            &a,
            started,
        );
    }
    let lock = project.lock.take().expect("a lock");
    install_locked(&project, &lock, &a, started, false)
}

/// Installs `lock` into the project's `vendor/` and reports it; `update` dispatches the update events instead of
/// the install ones.
pub(crate) fn install_locked(
    project: &Project,
    lock: &Lock,
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
    let scan = if a.classmap_authoritative {
        Scan::Authoritative
    } else if a.optimize_autoloader {
        Scan::Optimized
    } else {
        Scan::Classmap
    };
    let apcu = match &a.apcu_autoloader_prefix {
        Some(prefix) => Apcu::Prefix(prefix.clone()),
        None if a.apcu_autoloader => Apcu::RandomPrefix,
        None => Apcu::Off,
    };
    let options = Options {
        no_dev: a.no_dev,
        scan,
        apcu,
        no_autoloader: a.no_autoloader,
        no_plugins: a.no_plugins,
        ignore_platform: IgnorePlatform::new(a.ignore_platform_req.clone(), a.ignore_platform_reqs),
    };
    let fetcher = fetcher(root, manifest);
    let Dirs {
        vendor,
        bin: bin_dir,
    } = dirs(&root.to_string_lossy(), manifest);
    let env = |k: &str| std::env::var(k).ok();
    let mut scripts = Runner::new(root, &vendor, &bin_dir, manifest, !a.no_dev, &env);
    let total = lock.packages.len() + if a.no_dev { 0 } else { lock.packages_dev.len() };
    eprintln!(
        "Installing {} from pun.lock",
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
    let report = install(root, manifest, lock, &fetcher, &options, &env, &mut events)?;
    drop(events);
    if !a.no_scripts {
        scripts.dispatch(post, &[])?;
    }

    let ok = look.paint(Role::Ok, look.glyph("✔", "+"));
    let warn = look.paint(Role::Warning, look.glyph("⚠", "!"));
    if report.stale_lock {
        eprintln!(
            " {warn} pun.lock is out of date with the dependencies it locks: they changed since it was written, so run `pun update` to refresh it"
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
    for w in &report.warnings {
        eprintln!(" {warn} {w}");
    }
    let secs = started.elapsed().as_secs_f64();
    eprintln!(
        "{}",
        look.paint(Role::Summary, &format!("Installed in {secs:.1}s"))
    );
    Ok(0)
}
