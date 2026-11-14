//! `pun x` (and `punx`, the same binary under that name): run a command from a Composer package without adding it
//! to the project. Each package and constraint installs into a directory of its own in Pun's cache, as a small
//! project with a pun.toml and a pun.lock, so tools never share dependencies with each other or with the project.
//! Later runs reuse the install; a run more than a day after the last resolve resolves again, to pick up a new
//! release, and falls back to the install it has when that fails.

use crate::install::{InstallArgs, project};
use crate::update::{Plan, update_and_install};
use anyhow::{Context, Result, bail};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[derive(clap::Args)]
pub(crate) struct XArgs {
    /// The package (`vendor/name`, `vendor/name:^1.2` or an alias such as `laravel`), then the command when the
    /// package has several, then the command's arguments.
    #[arg(
        required = true,
        num_args = 1..,
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "PACKAGE [COMMAND] [ARGS]"
    )]
    args: Vec<String>,
}

/// Short names for packages people run often: (alias, package, command).
const ALIASES: [(&str, &str, &str); 12] = [
    ("laravel", "laravel/installer", "laravel"),
    ("pint", "laravel/pint", "pint"),
    ("envoy", "laravel/envoy", "envoy"),
    ("phpstan", "phpstan/phpstan", "phpstan"),
    ("psalm", "vimeo/psalm", "psalm"),
    ("rector", "rector/rector", "rector"),
    ("php-cs-fixer", "friendsofphp/php-cs-fixer", "php-cs-fixer"),
    ("phpcs", "squizlabs/php_codesniffer", "phpcs"),
    ("phpcbf", "squizlabs/php_codesniffer", "phpcbf"),
    ("phpunit", "phpunit/phpunit", "phpunit"),
    ("pest", "pestphp/pest", "pest"),
    ("phpmd", "phpmd/phpmd", "phpmd"),
];

/// How long a resolve stays current before a run looks for a newer release.
const FRESH: Duration = Duration::from_hours(24);

pub(crate) fn run(a: XArgs) -> Result<u8> {
    let (spec, rest) = a.args.split_first().expect("clap requires the package");
    let (name, constraint, alias_command) = package(spec)?;
    let dir = tool_dir(&name, &constraint)?;
    std::fs::create_dir_all(&dir).with_context(|| format!("can't create {}", dir.display()))?;
    // Two runs of one tool at once would install into the same vendor/; the second waits for the first.
    let guard = std::fs::File::create(dir.join(".lock"))
        .with_context(|| format!("can't write in {}", dir.display()))?;
    guard.lock().context("can't lock the tool's directory")?;
    ensure_installed(&dir, &name, &constraint)?;
    drop(guard);
    let commands = commands(&dir, &name)?;
    let (command, args) = pick(&name, &commands, alias_command, rest)?;
    exec(&dir.join("vendor/bin").join(command), args)
}

/// The package name, constraint (`*` for the newest stable release) and, for an alias, its command.
fn package(spec: &str) -> Result<(String, String, Option<&'static str>)> {
    let (name, constraint) = match spec.split_once(':').or_else(|| spec.split_once('@')) {
        Some((n, c)) if !c.trim().is_empty() => (n, c.trim()),
        Some((n, _)) => (n, "*"),
        None => (spec, "*"),
    };
    let name = name.to_ascii_lowercase();
    if let Some((_, package, command)) = ALIASES.iter().find(|(a, ..)| *a == name) {
        return Ok((package.to_string(), constraint.to_string(), Some(command)));
    }
    let valid = name.split_once('/').is_some_and(|(vendor, package)| {
        let part = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
        };
        part(vendor) && part(package)
    });
    if !valid {
        bail!(
            "`{name}` is not a package name: use vendor/name (such as laravel/installer) or an alias ({})",
            ALIASES.map(|a| a.0).join(", ")
        );
    }
    Ok((name, constraint.to_string(), None))
}

/// Pun's cache directory: `PUN_CACHE_DIR`, else the platform's user cache directory.
fn cache_dir() -> Result<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = env("PUN_CACHE_DIR") {
        return Ok(dir);
    }
    let home = env("HOME").context("HOME is not set; set PUN_CACHE_DIR")?;
    Ok(if cfg!(target_os = "macos") {
        home.join("Library/Caches/pun")
    } else {
        env("XDG_CACHE_HOME")
            .unwrap_or_else(|| home.join(".cache"))
            .join("pun")
    })
}

/// The directory a package installs into for one constraint, which is percent-encoded into the name.
fn tool_dir(name: &str, constraint: &str) -> Result<PathBuf> {
    let mut version = String::new();
    for b in (if constraint == "*" {
        "latest"
    } else {
        constraint
    })
    .bytes()
    {
        if b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_') {
            version.push(char::from(b));
        } else {
            version.push_str(&format!("%{b:02X}"));
        }
    }
    Ok(cache_dir()?.join("x").join(name).join(version))
}

/// Installs the package into `dir` unless its install there is less than a day old.
fn ensure_installed(dir: &Path, name: &str, constraint: &str) -> Result<()> {
    let manifest = format!(
        "[require]\n{} = {}\n",
        toml::Value::String(name.into()),
        toml::Value::String(constraint.into())
    );
    let lock = dir.join("pun.lock");
    let installed = dir.join("vendor/composer/installed.json").is_file();
    let age = (std::fs::metadata(&lock).and_then(|m| m.modified()))
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok());
    if installed && age.is_some_and(|a| a < FRESH) {
        return Ok(());
    }
    std::fs::write(dir.join("pun.toml"), manifest)
        .with_context(|| format!("can't write {}", dir.display()))?;
    if !installed {
        eprintln!("Installing {name} into {}", dir.display());
    }
    let args = InstallArgs {
        working_dir: Some(dir.to_path_buf()),
        no_dev: true,
        no_scripts: true,
        ..InstallArgs::default()
    };
    let result = project(args.working_dir.as_ref(), false)
        .and_then(|mut p| update_and_install(&mut p, Plan::default(), &args, Instant::now()));
    match result {
        Ok(0) => {
            // Count the day from this resolve even when it changed nothing.
            let _ = std::fs::File::options()
                .append(true)
                .open(&lock)
                .and_then(|f| f.set_modified(SystemTime::now()));
            Ok(())
        }
        Ok(_) | Err(_) if installed => {
            eprintln!(" ! couldn't look for a newer {name}, so this run uses the one installed");
            Ok(())
        }
        result => {
            // Nothing was installed, so leave no directory behind.
            let _ = std::fs::remove_dir_all(dir);
            let _ = dir.parent().map(std::fs::remove_dir);
            match result {
                Ok(code) => bail!("{name} didn't install (exit {code})"),
                Err(e) => Err(e),
            }
        }
    }
}

/// The commands the installed package declares (its `bin` entries, by file name).
fn commands(dir: &Path, name: &str) -> Result<Vec<String>> {
    let path = dir.join("vendor/composer/installed.json");
    let text =
        std::fs::read_to_string(&path).with_context(|| format!("can't read {}", path.display()))?;
    let installed: Value = serde_json::from_str(&text).context("invalid installed.json")?;
    let package = (installed.get("packages").and_then(Value::as_array))
        .into_iter()
        .flatten()
        .find(|p| p.get("name").and_then(Value::as_str) == Some(name))
        .with_context(|| format!("{name} is not in {}", path.display()))?;
    Ok((package.get("bin").and_then(Value::as_array))
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter_map(|b| Path::new(b).file_name())
        .map(|b| b.to_string_lossy().into_owned())
        .collect())
}

/// The command to run and its arguments, as cpx picks it: a first argument naming one of the commands, the
/// alias's command, the only command, or the command named after the package.
fn pick<'a>(
    name: &str,
    commands: &'a [String],
    alias_command: Option<&str>,
    rest: &'a [String],
) -> Result<(&'a str, &'a [String])> {
    if let Some((first, args)) = rest.split_first()
        && let Some(c) = commands.iter().find(|c| *c == first)
    {
        return Ok((c, args));
    }
    let short = name.rsplit('/').next().unwrap_or(name);
    let chosen = match commands {
        [] => bail!("{name} has no commands to run: its composer.json lists no bin"),
        [only] => Some(only),
        _ => (commands.iter())
            .find(|c| Some(c.as_str()) == alias_command)
            .or_else(|| commands.iter().find(|c| *c == short)),
    };
    match chosen {
        Some(c) => Ok((c, rest)),
        None => bail!(
            "{name} has {} commands ({}): name one after the package, as in `punx {name} {}`",
            commands.len(),
            commands.join(", "),
            commands[0]
        ),
    }
}

/// Runs the command in the current directory with the terminal, and exits with its status.
fn exec(command: &Path, args: &[String]) -> Result<u8> {
    let mut cmd = std::process::Command::new(command);
    cmd.args(args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = cmd.exec();
        Err(error).with_context(|| format!("can't run {}", command.display()))
    }
    #[cfg(not(unix))]
    {
        let status = cmd
            .status()
            .with_context(|| format!("can't run {}", command.display()))?;
        Ok(status.code().map_or(1, |c| u8::try_from(c).unwrap_or(1)))
    }
}
