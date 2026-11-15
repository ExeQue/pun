use crate::Exit;
use crate::install::{InstallArgs, project};
use crate::update::{Plan, update_and_install};
use anyhow::{Context, Result, bail};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[derive(clap::Args)]
pub(crate) struct XArgs {
    /// The package (`vendor/name`, `vendor/name:^1.2` or an alias such as `laravel`), then the command when the
    /// package has several, then the command's arguments. `list` prints the installs in the cache, and
    /// `clean [PACKAGE...]` removes them (all, or the packages named).
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

pub(crate) fn run(a: &XArgs) -> Result<Exit> {
    let (spec, rest) = a.args.split_first().expect("clap requires the package");
    match spec.as_str() {
        "list" => return list(),
        "clean" => return clean(rest),
        _ => {}
    }
    let (name, constraint, alias_command) = package(spec)?;
    let dir = tool_dir(&name, &constraint)?;
    std::fs::create_dir_all(&dir).with_context(|| format!("can't create {}", dir.display()))?;
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
pub(crate) fn cache_dir() -> Result<PathBuf> {
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
            let _ = write!(version, "%{b:02X}");
        }
    }
    Ok(cache_dir()?.join("x").join(name).join(version))
}

/// The constraint a directory name from `tool_dir` stands for.
fn constraint_of(dir_name: &str) -> String {
    if dir_name == "latest" {
        return "*".into();
    }
    let (mut out, mut rest) = (Vec::new(), dir_name.as_bytes());
    while let Some((&b, tail)) = rest.split_first() {
        let hex = tail.get(..2).and_then(|h| std::str::from_utf8(h).ok());
        match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
            Some(byte) if b == b'%' => {
                out.push(byte);
                rest = &tail[2..];
            }
            _ => {
                out.push(b);
                rest = tail;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Every install in the cache: (package, constraint, directory), sorted by package, then constraint.
fn installs() -> Result<Vec<(String, String, PathBuf)>> {
    let entries = |dir: &Path| -> Vec<(String, PathBuf)> {
        (std::fs::read_dir(dir).into_iter().flatten().flatten())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
            .collect()
    };
    let mut all = Vec::new();
    for (vendor, vendor_dir) in entries(&cache_dir()?.join("x")) {
        for (package, package_dir) in entries(&vendor_dir) {
            for (version, dir) in entries(&package_dir) {
                all.push((format!("{vendor}/{package}"), constraint_of(&version), dir));
            }
        }
    }
    all.sort();
    Ok(all)
}

/// `punx list`: each install, as the spec that runs it, with the version it has. A directory a run is still
/// installing into has no version yet and isn't listed.
fn list() -> Result<Exit> {
    let rows: Vec<_> = (installs()?.into_iter())
        .filter_map(|(name, constraint, dir)| {
            let version = installed_version(&dir, &name)?;
            Some(match constraint.as_str() {
                "*" => (name, version),
                _ => (format!("{name}:{constraint}"), version),
            })
        })
        .collect();
    if rows.is_empty() {
        println!("No packages are installed.");
        return Ok(Exit::Success);
    }
    let width = rows.iter().map(|(s, _)| s.len()).max().unwrap_or(0);
    for (spec, version) in rows {
        println!("{spec:width$}  {version}");
    }
    Ok(Exit::Success)
}

/// vendor/composer/installed.json, as far as `punx` reads it.
#[derive(serde::Deserialize)]
struct Installed {
    #[serde(default)]
    packages: Vec<InstalledPackage>,
}

#[derive(serde::Deserialize)]
struct InstalledPackage {
    name: String,
    version: Option<String>,
    #[serde(default)]
    bin: Vec<String>,
}

impl Installed {
    fn into_package(self, name: &str) -> Option<InstalledPackage> {
        self.packages.into_iter().find(|p| p.name == name)
    }
}

/// The version of `name` that `dir` has installed.
fn installed_version(dir: &Path, name: &str) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("vendor/composer/installed.json")).ok()?;
    let installed: Installed = serde_json::from_str(&text).ok()?;
    installed.into_package(name)?.version
}

/// `punx clean`: removes every install, or those of the packages named. A package named without a constraint
/// loses all its installs; with one, only that constraint's.
fn clean(specs: &[String]) -> Result<Exit> {
    let wanted = (specs.iter())
        .map(|spec| {
            let (name, constraint, _) = package(spec)?;
            let pinned = spec.contains([':', '@']);
            Ok((name, pinned.then_some(constraint)))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut removed = 0;
    for (name, constraint, dir) in installs()? {
        let matches = |(n, c): &(String, Option<String>)| {
            *n == name && c.as_ref().is_none_or(|c| *c == constraint)
        };
        if !wanted.is_empty() && !wanted.iter().any(matches) {
            continue;
        }
        if let Ok(guard) = std::fs::File::open(dir.join(".lock")) {
            let _ = guard.lock();
        }
        std::fs::remove_dir_all(&dir).with_context(|| format!("can't remove {}", dir.display()))?;
        let package_dir = dir.parent();
        let _ = package_dir.map(std::fs::remove_dir);
        let _ = package_dir.and_then(Path::parent).map(std::fs::remove_dir);
        removed += 1;
    }
    println!(
        "Removed {removed} install{}.",
        if removed == 1 { "" } else { "s" }
    );
    Ok(Exit::Success)
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
        .and_then(|mut p| update_and_install(&mut p, &Plan::default(), &args, Instant::now()));
    match result {
        Ok(Exit::Success) => {
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
            let _ = std::fs::remove_dir_all(dir);
            let _ = dir.parent().map(std::fs::remove_dir);
            match result {
                Ok(code) => bail!("{name} didn't install (exit {})", code as u8),
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
    let installed: Installed = serde_json::from_str(&text).context("invalid installed.json")?;
    let package = (installed.into_package(name))
        .with_context(|| format!("{name} is not in {}", path.display()))?;
    Ok((package.bin.iter())
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
fn exec(command: &Path, args: &[String]) -> Result<Exit> {
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
        std::process::exit(
            status
                .code()
                .map_or(1, |c| u8::try_from(c).map_or(1, i32::from)),
        )
    }
}
