use crate::install::{InstallArgs, Project, fetcher, install_locked, project};
use crate::render::{ColorChoice, Look, Role};
use anyhow::{Context, Result, bail};
use pun_packages::fetch::composer_dirs;
use pun_packages::lock::Lock;
use pun_packages::manifest_edit::{name_version_pairs, toml_remove, toml_set};
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
    /// In a workspace, the member whose manifest to edit; the default is the member holding the current directory,
    /// else the root.
    #[arg(short = 'p', long = "package", value_name = "MEMBER")]
    member: Option<String>,
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
    /// Edit the manifest without resolving or installing.
    #[arg(long)]
    no_update: bool,
    /// In a workspace, the member whose manifest to edit; the default is the member holding the current directory,
    /// else the root.
    #[arg(short = 'p', long = "package", value_name = "MEMBER")]
    member: Option<String>,
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
    std::thread::spawn(move || drop(resolution));
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
        &|k| std::env::var(k).ok(),
    )
    .dispatch(event, &[])?;
    Ok(())
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
    let env = |k: &str| std::env::var(k).ok();
    let repositories = repos::open(
        &project.root,
        &project.manifest,
        global.as_ref(),
        &fetcher,
        &env,
    )?;
    let resolved = resolver::resolve(
        &project.manifest,
        platform,
        repositories,
        project.lock.as_ref(),
        &options,
        &env,
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
    let lock_file = "pun.lock";
    let lock_path = project.root.join(lock_file);
    let lock_backup = std::fs::read_to_string(&lock_path).ok();
    if let Some(e) = &edit {
        write(&e.path, &e.text)?;
    }
    let text = Lock::from_composer_lock(&lock.to_composer_lock())?.to_toml();
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
        install_locked(project, &lock, a, started, true)
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

/// The pun.toml an edit changes: the project's own, or a workspace member's.
struct Target {
    path: PathBuf,
    /// The file's text now; `None` when it doesn't exist yet.
    original: Option<String>,
    manifest: Map<String, Value>,
    /// The member's package name, when the edit is a member's.
    member: Option<String>,
}

impl Target {
    fn file(&self, project: &Project) -> String {
        let path = self.path.strip_prefix(&project.root).unwrap_or(&self.path);
        path.display().to_string()
    }

    fn parse(&self, text: &str) -> Result<Map<String, Value>> {
        let table: toml::Table =
            toml::from_str(text).with_context(|| format!("invalid {}", self.path.display()))?;
        Ok(pun_config::manifest::parse(&self.path, &table)?
            .map(|m| m.json)
            .unwrap_or_default())
    }

    /// Makes the edit visible to the resolver: a member's file is written now, since the workspace's path
    /// repository reads it; the root's manifest is replaced in memory.
    fn apply(&self, project: &mut Project, text: &str) -> Result<()> {
        if self.member.is_some() {
            return write(&self.path, text);
        }
        let parsed = self.parse(text)?;
        project.manifest = match &project.workspace {
            Some(w) => w.manifest(parsed),
            None => parsed,
        };
        Ok(())
    }

    /// Takes back what [`Target::apply`] wrote.
    fn undo(&self) -> Result<()> {
        match &self.member {
            Some(_) => restore(&self.path, self.original.as_deref()),
            None => Ok(()),
        }
    }
}

/// The manifest `require` and `remove` edit: `-p`'s member, the member holding the current directory, or the
/// project's own.
fn target(project: &Project, member: Option<&str>) -> Result<Target> {
    let chosen = match &project.workspace {
        Some(w) => w.member(member, &project.here)?,
        None if member.is_some() => {
            bail!("-p names a workspace member, and this project has no [workspace]")
        }
        None => None,
    };
    let (dir, member) = match chosen {
        Some(m) => (m.dir.as_path(), Some(m.name.clone())),
        None => (project.root.as_path(), None),
    };
    let path = dir.join("pun.toml");
    let original = std::fs::read_to_string(&path).ok();
    let mut target = Target {
        path,
        original,
        manifest: Map::new(),
        member,
    };
    target.manifest = target.parse(target.original.as_deref().unwrap_or_default())?;
    Ok(target)
}

/// Resolves after an edit; when it doesn't resolve, takes the edit back and gives the exit code.
fn resolve_edit(
    project: &Project,
    plan: &Plan,
    a: &InstallArgs,
    target: &Target,
) -> Result<std::result::Result<(Resolution, Options), u8>> {
    match resolve(project, plan, a) {
        Ok(Ok(r)) => Ok(Ok(r)),
        Ok(Err(problems)) => {
            target.undo()?;
            Ok(Err(print_problems(&problems)))
        }
        Err(e) => {
            target.undo()?;
            Err(e)
        }
    }
}

fn sort_packages(manifest: &Map<String, Value>) -> bool {
    manifest.get("config").and_then(|c| c.get("sort-packages")) == Some(&Value::Bool(true))
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
    let target = target(&project, a.member.as_deref())?;
    let (section, other) = if a.dev {
        ("require-dev", "require")
    } else {
        ("require", "require-dev")
    };
    let own_name = (target.manifest.get("name"))
        .and_then(Value::as_str)
        .map(str::to_ascii_lowercase);
    let pairs = name_version_pairs(&a.packages);
    for (name, version) in &pairs {
        if !is_platform(name) && name.split('/').filter(|p| !p.is_empty()).count() != 2 {
            bail!("{name} is not a package name: write it as vendor/name");
        }
        if own_name.as_deref() == Some(name.to_ascii_lowercase().as_str()) {
            bail!("the project is {name}, so it can't require itself");
        }
        if let Some(v) = version.as_deref().filter(|v| *v != "self.version") {
            pun_packages::constraint::parse(v)
                .with_context(|| format!("{name}: {v} is not a version constraint"))?;
        }
    }
    let sort = a.sort_packages || sort_packages(&target.manifest);
    let set = |text: &str, name: &str, constraint: &str| -> Result<String> {
        let text = toml_set(text, section, name, constraint, sort)?;
        if required(&target.manifest, other, name).is_empty() {
            Ok(text)
        } else {
            Ok(toml_remove(&text, other, name)?)
        }
    };
    let mut text = target.original.clone().unwrap_or_default();
    for (name, version) in &pairs {
        if !required(&target.manifest, other, name).is_empty() {
            eprintln!(" ! {name} moves from {other} to {section}");
        }
        text = set(&text, name, version.as_deref().unwrap_or("*"))?;
    }
    let file = target.file(&project);
    if a.no_update {
        write(&target.path, &text)?;
        eprintln!("Updated {file}");
        return Ok(0);
    }
    let mut names: Vec<String> = pairs.iter().map(|(n, _)| n.to_ascii_lowercase()).collect();
    names.extend(target.member.clone());
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
    target.apply(&mut project, &text)?;
    let (resolution, options) = match resolve_edit(&project, &plan, &a.install, &target)? {
        Ok(r) => r,
        Err(code) => return Ok(code),
    };
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
        text = set(&text, name, &constraint)?;
        picked.push((name.to_ascii_lowercase(), constraint));
    }
    if !picked.is_empty() {
        if target.member.is_some() {
            target.apply(&mut project, &text)?;
            let (resolution, options) = match resolve_edit(&project, &plan, &a.install, &target)? {
                Ok(r) => r,
                Err(code) => return Ok(code),
            };
            lock = locker::lock(&resolution, &project.manifest, &options);
        } else {
            target.apply(&mut project, &text)?;
            locker::update_hash(&mut lock, &project.manifest, &picked)?;
        }
    }
    eprintln!("Updated {file}");
    let edit = Edit {
        path: target.path,
        text,
        original: target.original,
    };
    finish(&mut project, &plan, &a.install, started, lock, Some(edit))
}

pub(crate) fn remove(a: RemoveArgs) -> Result<u8> {
    let started = Instant::now();
    let mut project = project(a.install.working_dir.as_ref(), false)?;
    let target = target(&project, a.member.as_deref())?;
    let mut text = target.original.clone().unwrap_or_default();
    let mut removed = vec![];
    for pattern in &a.packages {
        let mut found = false;
        for section in ["require", "require-dev"] {
            for name in required(&target.manifest, section, pattern) {
                text = toml_remove(&text, section, &name)?;
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
    let file = target.file(&project);
    if a.no_update {
        write(&target.path, &text)?;
        eprintln!("Updated {file}");
        return Ok(0);
    }
    removed.extend(target.member.clone());
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
    target.apply(&mut project, &text)?;
    let (resolution, options) = match resolve_edit(&project, &plan, &a.install, &target)? {
        Ok(r) => r,
        Err(code) => return Ok(code),
    };
    let lock = locker::lock(&resolution, &project.manifest, &options);
    eprintln!("Updated {file}");
    let edit = Edit {
        path: target.path,
        text,
        original: target.original,
    };
    finish(&mut project, &plan, &a.install, started, lock, Some(edit))
}
