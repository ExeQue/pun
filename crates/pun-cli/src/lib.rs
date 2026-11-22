#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a command-line tool prints its reports"
)]

mod agent;
mod analyse;
mod analyse_report;
mod baseline;
mod cache;
mod composer;
mod console;
mod extensions;
pub mod finder;
mod fix_report;
mod github;
mod init;
mod install;
mod lint_plan;
mod list_files;
mod migrate;
mod php_diff;
mod pint_error;
mod prettier;
mod render;
mod report;
mod run;
#[cfg(feature = "pprof")]
mod sampler;
mod self_update;
mod table;
mod update;
mod watch;
mod workspace;
mod x;

use anyhow::Result;
use clap::{Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use pun_config::Options;
use run::{LintOpts, Stage};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Parser)]
#[command(
    name = "pun",
    bin_name = "pun",
    version,
    about = "Pun: format, lint and analyse PHP in one binary"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
    /// Agent output (a summary line, then one line per finding) even when no coding agent is detected.
    #[arg(long, global = true)]
    agent: bool,
    /// Human output even under a coding agent.
    #[arg(long, global = true, conflicts_with = "agent")]
    no_agent: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// Fix (or check) the layout of the given paths: the `[format]` rules.
    Format(Box<FormatArgs>),
    /// Report the `[lint]` rules' problems; `--fix` applies their fixes, except for rules with `fix = "off"`.
    Lint(Box<StageCmd>),
    /// Format, lint and analyse in one run: reports, or `--fix` applies the format and lint fixes.
    Check(Box<StageCmd>),
    /// Find bugs with types (PHPStan's rules): the `[analyse]` rules. Never edits a file.
    #[command(visible_alias = "analyze")]
    Analyse(Box<analyse::AnalyseArgs>),
    /// Write pun.toml from pint.json or .php-cs-fixer(.dist).php, rector.php and phpstan.neon, and rewrite `@php-cs-fixer-ignore` comments.
    Migrate(migrate::MigrateArgs),
    /// Install composer.lock into vendor/, with Composer's autoloader;
    /// without a lock, resolve one first.
    Install(install::InstallArgs),
    /// Resolve the dependencies again (all, or the packages named), write the lock and install it.
    Update(update::UpdateArgs),
    /// Add packages to require (or require-dev with --dev), then update and install them.
    Require(update::RequireArgs),
    /// Remove packages from require or require-dev, then update and install.
    #[command(visible_alias = "rm")]
    Remove(update::RemoveArgs),
    /// Run a command from a Composer package without adding it to the project (`punx` for short).
    X(x::XArgs),
    /// Composer's other commands: `dump-autoload`.
    Composer(composer::ComposerArgs),
    /// Write pun.toml for a new project: asks the code style, lint sets and analysis preset (`--yes` takes the defaults).
    Init(init::InitArgs),
    /// Replace this pun with the latest release (or the one named), verified as the installer verifies it.
    SelfUpdate(self_update::SelfUpdateArgs),
    /// Print the files a command would check with the current pun.toml.
    ListFiles(list_files::ListFilesArgs),
    /// Print the resolved configuration.
    Config {
        /// Output format (default: toml).
        #[arg(long, value_enum)]
        format: Option<ConfigFormat>,
        /// The pun.toml to read (default: the one in the current directory).
        #[arg(long)]
        config: Option<PathBuf>,
        /// Resolve this built-in format preset without a pun.toml.
        #[arg(long)]
        preset: Option<String>,
    },
    /// The extensions that add a framework's or tool's rules (`pun extension list`).
    Extension {
        #[command(subcommand)]
        cmd: extensions::ExtensionCmd,
    },
    /// Print a rule's summary, group, sets, fix safety, upstream names and docs link.
    Explain {
        /// A rule ID (`format/arrays/syntax`), or an upstream or old name, which is answered with its Pun ID.
        rule: String,
    },
}

/// `pun lint` / `pun check`: `pun format`'s flags plus `--fix` and the lint levels.
#[derive(Args)]
struct StageCmd {
    #[command(flatten)]
    args: FormatArgs,
    /// Apply the fixes instead of reporting them.
    #[arg(long)]
    fix: bool,
    /// Turn a rule or group off (`-A lint/style`), after pun.toml; repeatable, applied in order with -W and -D.
    #[arg(short = 'A', long = "allow", value_name = "RULE")]
    allow: Vec<String>,
    /// Turn a rule or group on as a warning.
    #[arg(short = 'W', long = "warn", value_name = "RULE")]
    warn: Vec<String>,
    /// Turn a rule or group on as an error.
    #[arg(short = 'D', long = "deny", value_name = "RULE")]
    deny: Vec<String>,
    /// Exit 1 when there are warnings.
    #[arg(long)]
    deny_warnings: bool,
    /// Exit 1 when there are more than this many warnings.
    #[arg(long, value_name = "N")]
    max_warnings: Option<usize>,
    /// Print each problem on one line.
    #[arg(long)]
    concise: bool,
    /// `-A`, `-W` and `-D` in command-line order (filled in `main` from clap's indices).
    #[arg(skip)]
    levels: Vec<(lint_plan::CliLevel, String)>,
}

#[derive(Clone, Copy, PartialEq, clap::ValueEnum)]
enum ConfigFormat {
    Toml,
    Json,
}

/// Pint's `DefaultCommand` flags, plus Pun's own.
#[derive(Args, Default, Clone)]
#[command(about = "Fix the coding style of the given path")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one bool per command-line flag"
)]
pub(crate) struct FormatArgs {
    /// The paths to run on (`-` reads stdin).
    paths: Vec<PathBuf>,
    /// Enable the `format/laravel/blade` rule to format Blade files.
    #[arg(long)]
    blade: bool,
    /// The configuration that should be used.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Disable loading any configuration file.
    #[arg(long)]
    no_config: bool,
    /// The preset that should be used.
    #[arg(long)]
    preset: Option<String>,
    /// Test for code style errors without fixing them.
    #[arg(long, alias = "check")]
    test: bool,
    /// Test for code style errors without fixing them and stop on first error.
    #[arg(long)]
    bail: bool,
    /// Fix code style errors but exit with status 1 if there were any changes made.
    #[arg(long)]
    repair: bool,
    /// Only fix files that have changed since branching off from the given branch.
    #[arg(long, value_name = "BRANCH")]
    diff: Option<String>,
    /// Only fix files that have uncommitted changes.
    #[arg(long)]
    dirty: bool,
    /// The output format that should be used (json, txt, agent, checkstyle, junit, xml, gitlab, github).
    #[arg(long)]
    format: Option<String>,
    /// Output the test results to a file at this path.
    #[arg(long)]
    output_to_file: Option<PathBuf>,
    /// The format that should be used when outputting the test results to a file.
    #[arg(long)]
    output_format: Option<String>,
    /// File path context for stdin input.
    #[arg(long)]
    stdin_filename: Option<PathBuf>,
    /// The path to the cache file (default: one per project in Pun's cache directory).
    #[arg(long, value_name = "PATH")]
    cache_file: Option<PathBuf>,
    #[expect(
        clippy::doc_markdown,
        reason = "this doc comment is the --help text, printed as written"
    )]
    /// The number of threads (default: one per core). Read like Pint's intval(), so any value is accepted.
    #[arg(long, value_parser = threads)]
    max_processes: Option<usize>,
    /// Print more detail: every finding under a coding agent, and the error class of a run error.
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,
    /// Print nothing; the exit code is the result.
    #[arg(short, long)]
    quiet: bool,
    #[expect(
        clippy::doc_markdown,
        reason = "this doc comment is the --help text, printed as written"
    )]
    /// Colour the output: auto (on for a terminal, off when piped or with NO_COLOR), always or never.
    #[arg(long, value_enum, value_name = "WHEN")]
    color: Option<render::ColorChoice>,
    /// `--color=always` (Pint's spelling); also decorates the `--format` reports as Pint does.
    #[arg(long, hide = true)]
    ansi: bool,
    /// `--color=never` (Pint's spelling).
    #[arg(long, hide = true)]
    no_ansi: bool,
    #[command(flatten)]
    pun: PunArgs,
    /// `GITHUB_ACTIONS=true`, read once in `main`: human output gets GitHub annotations after it.
    #[arg(skip)]
    github_actions: bool,
}

impl FormatArgs {
    /// `--color`, or Pint's `--ansi` (which wins over `--no-ansi`, as in Symfony's configureIO).
    fn color(&self) -> render::ColorChoice {
        use render::ColorChoice;
        self.color.unwrap_or(if self.ansi {
            ColorChoice::Always
        } else if self.no_ansi {
            ColorChoice::Never
        } else {
            ColorChoice::Auto
        })
    }

    /// How a failed run of `pun format`, `lint` or `check` shows its error.
    fn failure(&self) -> report::Failure {
        let human = self.format.is_none() && !self.quiet && !agent::enabled();
        report::Failure {
            pint: true,
            machine: !human,
            github_format: self.format.as_deref().and_then(|f| f.parse().ok())
                == Some(report::ReportFormat::Github),
            annotate: human && self.github_actions,
            color: self.color(),
        }
    }
}

/// `pun format`'s flags that Pint doesn't have.
#[derive(Args, Default, Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one bool per command-line flag"
)]
pub(crate) struct PunArgs {
    /// Only fix files with staged changes (Pun).
    #[arg(long)]
    staged: bool,
    /// Only fix files changed since the given commit, plus untracked files (Pun).
    #[arg(long, value_name = "REF")]
    since: Option<String>,
    /// Read code from stdin (Pun); `--stdin-filename` gives it a path for exclusion rules.
    #[arg(long)]
    stdin: bool,
    /// Print the fixers the run applies, in priority order (to stderr).
    #[arg(long)]
    explain: bool,
    /// Keep running and re-format files as they change (Pun).
    #[arg(long, conflicts_with_all = ["stdin", "stdin_filename"])]
    watch: bool,
    /// Check every file, without reading or writing the result cache (Pun; also `PUN_NO_CACHE=1`).
    #[arg(long)]
    no_cache: bool,
}

/// What a run tells the shell. Pint's 0 and 1, and 2 when Pun couldn't run, where Pint exits 1.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub(crate) enum Exit {
    Success = 0,
    /// Problems found, or files changed under `--test` or `--repair`.
    Failure = 1,
    RunError = 2,
    /// The lock doesn't meet composer.json's requirements (Composer's `Installer::ERROR_LOCK_FILE_INVALID`).
    LockInvalid = 4,
}

impl From<Exit> for ExitCode {
    fn from(exit: Exit) -> ExitCode {
        ExitCode::from(exit as u8)
    }
}

/// `--max-processes`, read like Pint's `intval()`: any value is accepted, and one that is not a positive number is 0,
/// one thread per core.
#[expect(
    clippy::unnecessary_wraps,
    reason = "clap's `value_parser` takes a fallible function"
)]
pub(crate) fn threads(value: &str) -> Result<usize, std::convert::Infallible> {
    Ok(usize::try_from(pun_tokens::php_compat::intval(value)).unwrap_or(0))
}

/// The stack of every thread that parses, lints or analyses: they recurse as deep as the syntax tree, and a long
/// chain of concatenations or `&&` overflows the 8 MiB of a main thread and Rust's 2 MiB default.
pub(crate) const STACK: usize = 36 * 1024 * 1024;

/// Runs `pun` on the process's arguments, on a thread with a stack deep enough for the parser, and returns its exit
/// code.
///
/// # Panics
///
/// When the thread can't start, or when the run panics outside a file's isolated fixing.
pub fn main() -> ExitCode {
    let run = std::thread::Builder::new()
        .name("main".into())
        .stack_size(STACK)
        .spawn(run)
        .expect("the main thread starts");
    run.join()
        .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

fn run() -> ExitCode {
    #[cfg(feature = "pprof")]
    let sampler = sampler::start();
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if (args.first().map(std::path::Path::new))
        .and_then(std::path::Path::file_stem)
        .is_some_and(|n| n == "punx")
    {
        args.insert(1, "x".into());
    }
    let matches = Cli::command().get_matches_from(args);
    let mut cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    if let (Cmd::Lint(cmd) | Cmd::Check(cmd), Some((_, sub))) = (&mut cli.cmd, matches.subcommand())
    {
        cmd.levels = levels(sub);
    }
    let name = matches.subcommand_name().unwrap_or_default();
    let flag = (cli.agent || cli.no_agent).then_some(cli.agent);
    agent::init(flag, name);
    let github_actions = std::env::var_os("GITHUB_ACTIONS").is_some_and(|v| v == "true");
    let mut cmd = cli.cmd;
    let other = report::Failure {
        pint: false,
        machine: false,
        github_format: false,
        annotate: false,
        color: render::ColorChoice::Auto,
    };
    let (threads, failure) = match &mut cmd {
        Cmd::Format(a) => {
            a.github_actions = github_actions;
            (a.max_processes, a.failure())
        }
        Cmd::Lint(cmd) | Cmd::Check(cmd) => {
            cmd.args.github_actions = github_actions;
            (cmd.args.max_processes, cmd.args.failure())
        }
        Cmd::Analyse(a) => {
            a.github_actions = github_actions;
            (a.max_processes(), a.failure())
        }
        _ => (None, other),
    };
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.unwrap_or(0))
        .stack_size(STACK)
        .build_global();
    let result = match cmd {
        Cmd::Format(a) if a.pun.watch => watch::run(*a),
        Cmd::Format(a) => run::stage(&a, Stage::Format, &LintOpts::default()),
        Cmd::Lint(cmd) => lint_or_check(*cmd, |fix| Stage::Lint { fix }),
        Cmd::Check(cmd) => lint_or_check(*cmd, |fix| Stage::Check { fix }),
        Cmd::Analyse(a) => analyse::run(&a),
        Cmd::Migrate(a) => migrate::run(&a),
        Cmd::Init(a) => init::run(a),
        Cmd::Install(a) => install::run(&a),
        Cmd::Update(a) => update::run(a),
        Cmd::Require(a) => update::require(a),
        Cmd::Remove(a) => update::remove(a),
        Cmd::X(a) => x::run(&a),
        Cmd::Composer(a) => composer::run(a),
        Cmd::SelfUpdate(a) => self_update::run(&a),
        Cmd::ListFiles(a) => list_files::run(a),
        Cmd::Config {
            format,
            config,
            preset,
            ..
        } => show_config(format, config, preset),
        Cmd::Extension { cmd } => extensions::run(cmd),
        Cmd::Explain { rule } => pun_config::explain(&rule).map(|t| {
            print!("{t}");
            Exit::Success
        }),
    };
    #[cfg(feature = "pprof")]
    if let Some(s) = sampler {
        s.report();
    }
    result.unwrap_or_else(|e| report::fail(e, &failure)).into()
}

/// `pun lint` / `pun check`: a `pun format` run over other rules; without `--fix` nothing is written.
fn lint_or_check(cmd: StageCmd, stage: fn(bool) -> Stage) -> Result<Exit> {
    let mut a = cmd.args;
    a.test |= !cmd.fix;
    let lint = LintOpts {
        levels: cmd.levels,
        deny_warnings: cmd.deny_warnings,
        max_warnings: cmd.max_warnings,
        concise: cmd.concise,
    };
    run::stage(&a, stage(cmd.fix), &lint)
}

/// `-A`, `-W` and `-D` in the order they were given: later ones win.
fn levels(m: &clap::ArgMatches) -> Vec<(lint_plan::CliLevel, String)> {
    use lint_plan::CliLevel;
    let mut out: Vec<(usize, CliLevel, String)> = vec![];
    for (id, level) in [
        ("allow", CliLevel::Allow),
        ("warn", CliLevel::Warn),
        ("deny", CliLevel::Deny),
    ] {
        if let (Some(values), Some(indices)) = (m.get_many::<String>(id), m.indices_of(id)) {
            out.extend(values.zip(indices).map(|(v, i)| (i, level, v.clone())));
        }
    }
    out.sort_by_key(|l| l.0);
    out.into_iter().map(|(_, l, v)| (l, v)).collect()
}

/// `PUN_IGNORE_PHP_CONSTRAINT`: the harness runs corpora whose composer.json allows PHP < 8.4.
fn ignore_php_constraint() -> bool {
    std::env::var_os("PUN_IGNORE_PHP_CONSTRAINT").is_some()
}

fn root() -> Result<PathBuf> {
    Ok(std::env::current_dir()?.canonicalize()?)
}

fn show_config(
    fmt: Option<ConfigFormat>,
    config: Option<PathBuf>,
    preset: Option<String>,
) -> Result<Exit> {
    let root = root()?;
    let cfg = pun_config::load(
        &root,
        &Options {
            config,
            preset,
            ignore_php_constraint: ignore_php_constraint(),
            ..Default::default()
        },
    )?;
    let v = cfg.show();
    let sources: Vec<String> = (cfg.sources.iter())
        .map(|s| s.strip_prefix(&root).unwrap_or(s).display().to_string())
        .collect();
    let toml = if sources.is_empty() {
        toml::to_string(&v)?
    } else {
        format!(
            "# Resolved from {}.\n\n{}",
            sources.join(", "),
            toml::to_string(&v)?
        )
    };
    if fmt.is_none() && agent::enabled() {
        let detail = [
            format!("preset {}", cfg.preset),
            format!("PHP {}", cfg.php),
            agent::count(cfg.rules.len(), "format rule"),
            agent::count(cfg.lint.rules.len(), "lint rule"),
        ];
        let from = format!("from {}", sources.join(", "));
        eprint!("{}", agent::render("ok", &detail, &from, &[], false));
        print!("{toml}");
    } else if fmt == Some(ConfigFormat::Json) {
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        print!("{toml}");
    }
    Ok(Exit::Success)
}
