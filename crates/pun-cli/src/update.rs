//! `pun update`, `pun require` and `pun remove`: resolve the dependencies again (all of them, or the packages named,
//! with Composer's partial-update rules), write the lock (pun.lock, or composer.lock in a project not migrated),
//! then install it as `pun install` does. `require` and `remove` edit pun.toml (or composer.json) in memory and
//! write it only once the new requirements resolve; when the install after fails, both files are put back.

use crate::install::{InstallArgs, Project, fetcher, install_locked, project};
use crate::render::{ColorChoice, Look, Role};
use anyhow::{Context, Result, bail};
use pun_packages::fetch::composer_dirs;
use pun_packages::lock::Lock;
use pun_packages::manifest_edit::{
    json_remove, json_set, name_version_pairs, toml_remove, toml_set,
};
use pun_packages::resolver::pool::Transitive;
use pun_packages::resolver::{self, Options, Resolution, explain, locker, platform, repos};
use pun_packages::scripts::Runner;
use pun_packages::version::{compare, is_platform, name_matches, normalize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(clap::Args)]
pub(crate) struct UpdateArgs {
    /// The packages to update (`vendor/*` patterns too); without any, every package is resolved again.
    packages: Vec<String>,
    #[command(flatten)]
    resolve: ResolveArgs,
    /// Resolve and print the changes without writing the lock or installing.
    #[arg(long)]
    dry_run: bool,
    #[command(flatten)]
    install: InstallArgs,
}

#[derive(clap::Args, Clone, Default)]
pub(crate) struct ResolveArgs {
    /// Also update the named packages' dependencies, except those the project requires itself.
    #[arg(short = 'w', long)]
    with_dependencies: bool,
    /// Also update the named packages' dependencies, the project's own requirements included.
    #[arg(short = 'W', long)]
    with_all_dependencies: bool,
    /// Prefer the lowest version each requirement allows (to test the declared minimums).
    #[arg(long)]
    prefer_lowest: bool,
    /// Prefer stable versions over unstable ones where both satisfy a requirement.
    #[arg(long)]
    prefer_stable: bool,
    /// Keep the locked version of every package not named, where the requirements allow.
    #[arg(short = 'm', long)]
    minimal_changes: bool,
    /// Write the lock but leave vendor/ as it is.
    #[arg(long)]
    no_install: bool,
}

#[derive(clap::Args)]
pub(crate) struct RequireArgs {
    /// Packages to add: `vendor/name`, `vendor/name:^1.2` or `vendor/name ^1.2`. Without a version, the newest that
    /// resolves is written as `^major.minor`.
    #[arg(required = true)]
    packages: Vec<String>,
    /// Add them to require-dev.
    #[arg(long)]
    dev: bool,
    /// Keep the section sorted (the `sort-packages` setting does this always).
    #[arg(long)]
    sort_packages: bool,
    /// Edit the manifest without resolving or installing.
    #[arg(long)]
    no_update: bool,
    #[command(flatten)]
    resolve: ResolveArgs,
    #[command(flatten)]
    install: InstallArgs,
}

#[derive(clap::Args)]
pub(crate) struct RemoveArgs {
    /// Packages to remove (`vendor/*` patterns too), from require or require-dev, whichever holds them.
    #[arg(required = true)]
    packages: Vec<String>,
    /// Accepted for Composer's flag: Pun removes a package from whichever section holds it.
    #[arg(long, hide = true)]
    dev: bool,
    /// Edit the manifest without resolving or installing.
    #[arg(long)]
    no_update: bool,
    #[command(flatten)]
    resolve: ResolveArgs,
    #[command(flatten)]
    install: InstallArgs,
}

/// What one resolution is asked for.
#[derive(Default)]
pub(crate) struct Plan {
    pub(crate) allow_list: Vec<String>,
    pub(crate) resolve: ResolveArgs,
    /// The transitive rule when no flag names one: `require` and `remove` update the dependencies too.
    pub(crate) transitive: Transitive,
    pub(crate) dry_run: bool,
}

/// A manifest edit, written once the update resolves.
struct Edit {
    path: PathBuf,
    text: String,
    original: Option<String>,
}

pub(crate) fn run(a: UpdateArgs) -> Result<u8> {
    let started = Instant::now();
    let mut project = project(a.install.working_dir.as_ref(), false)?;
    if let Some(lock) = &project.lock {
        for name in a.packages.iter().filter(|n| !n.contains('*')) {
            let locked = (lock.all())
                .filter_map(|p| p.get("name").and_then(Value::as_str))
                .any(|n| n.eq_ignore_ascii_case(name));
            if !locked {
                eprintln!(" ! {name} is not in the lock, so there is nothing of it to update");
            }
        }
    }
    let plan = Plan {
        allow_list: a.packages,
        resolve: a.resolve,
        dry_run: a.dry_run,
        ..Plan::default()
    };
    update_and_install(&mut project, plan, &a.install, started)
}

/// Resolves the project, prints what changes, writes the lock and installs it.
pub(crate) fn update_and_install(
    project: &mut Project,
    plan: Plan,
    a: &InstallArgs,
    started: Instant,
) -> Result<u8> {
    pre_update(project, a, &plan)?;
    let (resolution, options) = match resolve(project, &plan, a)? {
        Ok(r) => r,
        Err(problems) => return Ok(print_problems(&problems)),
    };
    let lock = locker::lock(&resolution, &project.manifest, &options);
    finish(project, &plan, a, started, lock, None)
}

fn pre_update(project: &Project, a: &InstallArgs, plan: &Plan) -> Result<()> {
    if a.no_scripts || plan.dry_run {
        return Ok(());
    }
    dispatch(project, a, "pre-update-cmd")
}

fn dispatch(project: &Project, a: &InstallArgs, event: &str) -> Result<()> {
    let (vendor, bin_dir) =
        pun_packages::install::dirs(&project.root.to_string_lossy(), &project.manifest);
    Runner::new(
        &project.root,
        &vendor,
        &bin_dir,
        &project.manifest,
        !a.no_dev,
    )
    .dispatch(event, &[])
}

/// The global config.json's `repositories`.
fn global_repositories() -> Option<Value> {
    let (home, _) = composer_dirs()?;
    let text = std::fs::read_to_string(home.join("config.json")).ok()?;
    let config: Value = serde_json::from_str(&text).ok()?;
    config.get("repositories").cloned()
}

/// The project resolved: the packages and the options they were resolved with, or the problems in sentences.
fn resolve(
    project: &Project,
    plan: &Plan,
    a: &InstallArgs,
) -> Result<std::result::Result<(Resolution, Options), Vec<Vec<String>>>> {
    let r = &plan.resolve;
    let transitive = if r.with_all_dependencies {
        Transitive::All
    } else if r.with_dependencies {
        Transitive::Dependencies
    } else {
        plan.transitive
    };
    let options = Options {
        allow_list: plan.allow_list.clone(),
        transitive,
        prefer_stable: r.prefer_stable,
        prefer_lowest: r.prefer_lowest,
        minimal_changes: r.minimal_changes,
        ignore_platform: a.ignore_platform_req.clone(),
        ignore_all_platform: a.ignore_platform_reqs,
    };
    let php = std::env::var("PHP_BINARY")
        .ok()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| "php".into());
    let overrides = (project.manifest.get("config"))
        .and_then(|c| c.get("platform"))
        .and_then(Value::as_object);
    let platform = platform::detect(&php, overrides)?;
    let fetcher = fetcher(&project.root, &project.manifest);
    let global = global_repositories();
    let repositories = repos::open(&project.root, &project.manifest, global.as_ref(), &fetcher)?;
    let resolved = resolver::resolve(
        &project.manifest,
        platform,
        repositories,
        project.lock.as_ref(),
        &options,
    )?;
    Ok(match resolved {
        Ok(resolution) => Ok((resolution, options)),
        Err(failure) => Err(explain::explain(&failure, &project.manifest)),
    })
}

/// Prints why the requirements don't resolve; the exit code, 2 as Composer's.
fn print_problems(problems: &[Vec<String>]) -> u8 {
    let look = Look::stderr(ColorChoice::Auto);
    let cross = look.paint(Role::Error, look.glyph("✖", "x"));
    eprintln!("{cross} The requirements can't be resolved:");
    for (i, lines) in problems.iter().enumerate() {
        eprintln!("\n  Problem {}", i + 1);
        for l in lines {
            eprintln!("    - {l}");
        }
    }
    2
}

/// Prints the changes, writes the manifest edit and the lock, then installs; a failed install after an edit puts
/// both files back.
fn finish(
    project: &mut Project,
    plan: &Plan,
    a: &InstallArgs,
    started: Instant,
    lock: Lock,
    edit: Option<Edit>,
) -> Result<u8> {
    print_changes(project.lock.as_ref(), &lock);
    if plan.dry_run {
        return Ok(0);
    }
    let lock_file = project.lock_file();
    let lock_path = project.root.join(lock_file);
    let lock_backup = std::fs::read_to_string(&lock_path).ok();
    if let Some(e) = &edit {
        write(&e.path, &e.text)?;
    }
    let text = if lock_file == "pun.lock" {
        // pun.lock is TOML, which has no null: take it through composer.lock's reader, which drops them.
        Lock::from_composer_lock(&lock.to_composer_lock())?.to_toml()
    } else {
        lock.to_composer_lock()
    };
    write(&lock_path, &text)?;
    let result = if plan.resolve.no_install {
        (!a.no_scripts)
            .then(|| dispatch(project, a, "post-update-cmd"))
            .transpose()
            .map(|_| {
                eprintln!("Wrote {lock_file}; vendor/ is unchanged (--no-install)");
                0
            })
    } else {
        install_locked(project, &lock, lock_file, a, started, true)
    };
    if let Some(e) = edit
        && !matches!(result, Ok(0))
    {
        restore(&e.path, e.original.as_deref())?;
        restore(&lock_path, lock_backup.as_deref())?;
        eprintln!(
            "The install failed, so {} and {lock_file} are back as they were",
            e.path.file_name().unwrap_or_default().to_string_lossy()
        );
    }
    project.lock = Some(lock);
    result
}

/// Writes `text` to `path` through a temporary file beside it, so a reader never sees half of it.
fn write(path: &Path, text: &str) -> Result<()> {
    let tmp = path.with_extension(format!("pun-{}.tmp", std::process::id()));
    std::fs::write(&tmp, text).with_context(|| format!("can't write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("can't write {}", path.display()))
}

fn restore(path: &Path, original: Option<&str>) -> Result<()> {
    match original {
        Some(text) => write(path, text),
        None => match std::fs::remove_file(path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(e).with_context(|| format!("can't remove {}", path.display()))
            }
            _ => Ok(()),
        },
    }
}

/// Each package's (lowercase name, version, short reference) in a lock, by name.
fn versions(lock: &Lock) -> Vec<(String, String, String)> {
    let s = |v: Option<&Value>| v.and_then(Value::as_str).unwrap_or_default().to_string();
    let mut out: Vec<(String, String, String)> = (lock.all())
        .map(|p| {
            let dist = s(p.get("dist").and_then(|d| d.get("reference")));
            let reference = if dist.is_empty() {
                s(p.get("source").and_then(|d| d.get("reference")))
            } else {
                dist
            };
            (
                s(p.get("name")).to_ascii_lowercase(),
                s(p.get("version")),
                reference.chars().take(7).collect(),
            )
        })
        .collect();
    out.sort();
    out
}

/// The lock's changes, one line per package: `+` added, `↑`/`↓` a new version, `~` a new commit of the same
/// branch, `-` removed.
fn print_changes(old: Option<&Lock>, new: &Lock) {
    let look = Look::stderr(ColorChoice::Auto);
    let (old, new) = (old.map(versions).unwrap_or_default(), versions(new));
    let arrow = look.glyph("→", "->");
    let mut lines = vec![];
    for (name, version, reference) in &new {
        match old.iter().find(|(n, _, _)| n == name) {
            None => lines.push(format!(" {} {name} {version}", look.paint(Role::Ok, "+"))),
            Some((_, was, _)) if was != version => {
                let up = compare(
                    &normalize(version).unwrap_or_else(|_| version.clone()),
                    &normalize(was).unwrap_or_else(|_| was.clone()),
                ) >= 0;
                let mark = if up {
                    look.paint(Role::Ok, look.glyph("↑", "^"))
                } else {
                    look.paint(Role::Warning, look.glyph("↓", "v"))
                };
                lines.push(format!(" {mark} {name} {was} {arrow} {version}"));
            }
            Some((_, _, was)) if was != reference => lines.push(format!(
                " {} {name} {version} ({was} {arrow} {reference})",
                look.paint(Role::Ok, "~")
            )),
            Some(_) => {}
        }
    }
    for (name, version, _) in &old {
        if !new.iter().any(|(n, _, _)| n == name) {
            lines.push(format!(
                " {} {name} {version}",
                look.paint(Role::Error, "-")
            ));
        }
    }
    if lines.is_empty() {
        eprintln!("Nothing to change: the lock is up to date");
    } else {
        eprintln!("Lock changes:");
        for l in lines {
            eprintln!("{l}");
        }
    }
}

/// The manifest file of a project, and its text now (`None` when it doesn't exist yet).
fn manifest_file(project: &Project) -> (PathBuf, Option<String>) {
    let name = if project.pun_toml {
        "pun.toml"
    } else {
        "composer.json"
    };
    let path = project.root.join(name);
    let text = std::fs::read_to_string(&path).ok();
    (path, text)
}

/// The manifest an edited text holds, in composer.json's shape.
fn parse_manifest(project: &Project, path: &Path, text: &str) -> Result<Map<String, Value>> {
    if project.pun_toml {
        let table: toml::Table = toml::from_str(text).context("invalid pun.toml")?;
        Ok(pun_config::manifest::parse(path, &table)?
            .map(|m| m.json)
            .unwrap_or_default())
    } else {
        serde_json::from_str(text).context("invalid composer.json")
    }
}

fn sort_packages(project: &Project) -> bool {
    project
        .manifest
        .get("config")
        .and_then(|c| c.get("sort-packages"))
        == Some(&Value::Bool(true))
}

/// The names in a link section matching `pattern` (a name, or a `vendor/*` pattern).
fn required(manifest: &Map<String, Value>, section: &str, pattern: &str) -> Vec<String> {
    (manifest
        .get(section)
        .and_then(Value::as_object)
        .into_iter()
        .flatten())
    .map(|(k, _)| k.clone())
    .filter(|k| name_matches(&pattern.to_ascii_lowercase(), &k.to_ascii_lowercase()))
    .collect()
}

pub(crate) fn require(a: RequireArgs) -> Result<u8> {
    let started = Instant::now();
    let mut project = project(a.install.working_dir.as_ref(), true)?;
    let (section, other) = if a.dev {
        ("require-dev", "require")
    } else {
        ("require", "require-dev")
    };
    let root_name = project
        .manifest
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    let pairs = name_version_pairs(&a.packages);
    for (name, version) in &pairs {
        if !is_platform(name) && name.split('/').filter(|p| !p.is_empty()).count() != 2 {
            bail!("{name} is not a package name: write it as vendor/name");
        }
        if root_name.as_deref() == Some(name.to_ascii_lowercase().as_str()) {
            bail!("the project is {name}, so it can't require itself");
        }
        if let Some(v) = version.as_deref().filter(|v| *v != "self.version") {
            pun_packages::constraint::parse(v)
                .with_context(|| format!("{name}: {v} is not a version constraint"))?;
        }
    }
    let sort = a.sort_packages || sort_packages(&project);
    let set = |project: &Project, text: &str, name: &str, constraint: &str| -> Result<String> {
        if project.pun_toml {
            let text = toml_set(text, section, name, constraint, sort)?;
            if required(&project.manifest, other, name).is_empty() {
                Ok(text)
            } else {
                toml_remove(&text, other, name)
            }
        } else {
            json_set(text, section, other, name, constraint, sort)
        }
    };
    let (path, original) = manifest_file(&project);
    let mut text = original.clone().unwrap_or_default();
    for (name, version) in &pairs {
        if !required(&project.manifest, other, name).is_empty() {
            eprintln!(" ! {name} moves from {other} to {section}");
        }
        text = set(&project, &text, name, version.as_deref().unwrap_or("*"))?;
    }
    let file = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    if a.no_update {
        write(&path, &text)?;
        eprintln!("Updated {file}");
        return Ok(0);
    }
    project.manifest = parse_manifest(&project, &path, &text)?;
    let names: Vec<String> = pairs.iter().map(|(n, _)| n.to_ascii_lowercase()).collect();
    let plan = Plan {
        allow_list: if project.lock.is_some() {
            names
        } else {
            vec![]
        },
        resolve: a.resolve,
        transitive: Transitive::Dependencies,
        dry_run: false,
    };
    pre_update(&project, &a.install, &plan)?;
    let (resolution, options) = match resolve(&project, &plan, &a.install)? {
        Ok(r) => r,
        Err(problems) => return Ok(print_problems(&problems)),
    };
    // Composer resolves a package named without a version at `*`, then writes the constraint for what it found
    // and updates the lock's hash and stability flags to match.
    let mut lock = locker::lock(&resolution, &project.manifest, &options);
    let mut picked = vec![];
    for (name, version) in &pairs {
        if version.is_some() {
            continue;
        }
        let Some(constraint) = locker::recommended(&resolution, name) else {
            continue;
        };
        eprintln!("Using version {constraint} for {name}");
        if constraint.starts_with("dev-")
            && !["dev-main", "dev-master", "dev-trunk", "dev-latest"].contains(&constraint.as_str())
        {
            eprintln!(
                " ! {constraint} looks like a feature branch, which may not keep working for long"
            );
        }
        text = set(&project, &text, name, &constraint)?;
        picked.push((name.to_ascii_lowercase(), constraint));
    }
    if !picked.is_empty() {
        project.manifest = parse_manifest(&project, &path, &text)?;
        locker::update_hash(&mut lock, &project.manifest, &picked)?;
    }
    eprintln!("Updated {file}");
    let edit = Edit {
        path,
        text,
        original,
    };
    finish(&mut project, &plan, &a.install, started, lock, Some(edit))
}

pub(crate) fn remove(a: RemoveArgs) -> Result<u8> {
    let started = Instant::now();
    let mut project = project(a.install.working_dir.as_ref(), false)?;
    let (path, original) = manifest_file(&project);
    let mut text = original.clone().unwrap_or_default();
    let mut removed = vec![];
    for pattern in &a.packages {
        let mut found = false;
        for section in ["require", "require-dev"] {
            for name in required(&project.manifest, section, pattern) {
                text = if project.pun_toml {
                    toml_remove(&text, section, &name)?
                } else {
                    json_remove(&text, section, &name)?
                };
                removed.push(name.to_ascii_lowercase());
                found = true;
            }
        }
        if !found {
            eprintln!(" ! {pattern} is not required, so there is nothing to remove");
        }
    }
    if removed.is_empty() {
        return Ok(0);
    }
    let file = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    if a.no_update {
        write(&path, &text)?;
        eprintln!("Updated {file}");
        return Ok(0);
    }
    project.manifest = parse_manifest(&project, &path, &text)?;
    let plan = Plan {
        allow_list: if project.lock.is_some() {
            removed
        } else {
            vec![]
        },
        resolve: a.resolve,
        transitive: Transitive::Dependencies,
        dry_run: false,
    };
    pre_update(&project, &a.install, &plan)?;
    let (resolution, options) = match resolve(&project, &plan, &a.install)? {
        Ok(r) => r,
        Err(problems) => return Ok(print_problems(&problems)),
    };
    let lock = locker::lock(&resolution, &project.manifest, &options);
    eprintln!("Updated {file}");
    let edit = Edit {
        path,
        text,
        original,
    };
    finish(&mut project, &plan, &a.install, started, lock, Some(edit))
}
