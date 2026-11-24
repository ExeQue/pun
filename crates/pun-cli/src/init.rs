use crate::render::{ColorChoice, Look, Renderer};
use crate::{Exit, agent, migrate, root};
use anyhow::{Result, bail};
use dialoguer::console::Term;
use pun_config::composer::{Composer, Locked};
use pun_config::{AnalysePreset, LintPreset};
use std::io::IsTerminal;
use std::path::Path;

#[derive(clap::Args)]
pub(crate) struct InitArgs {
    /// The code style, `[format] preset` (default: laravel for a Laravel app, else psr12).
    #[arg(long, value_enum)]
    style: Option<Style>,
    /// The lint preset: essential, recommended or strict (default: recommended).
    #[arg(long, value_name = "PRESET")]
    lint: Option<String>,
    /// The analysis preset: basic, essential, recommended, strict or max (default: recommended).
    #[arg(long, value_name = "PRESET")]
    analyse: Option<String>,
    /// Don't ask: take the defaults and the flags.
    #[arg(long, short)]
    yes: bool,
}

/// The code styles init offers. `empty` stays valid in pun.toml but is not offered (init.md Decisions).
#[derive(Clone, Copy, PartialEq, Debug, clap::ValueEnum)]
enum Style {
    Laravel,
    Psr12,
}

const STYLES: [&str; 2] = ["laravel", "psr12"];

pub(crate) fn run(a: InitArgs) -> Result<Exit> {
    let root = root()?;
    if root.join("pun.toml").exists() {
        bail!(
            "pun.toml exists; `pun init` doesn't overwrite it. Edit it, or move it away and run `pun init` again."
        );
    }
    let lint = match a.lint {
        Some(p) => lint_preset(&p)?,
        None => LintPreset::DEFAULT,
    };
    let analyse = match a.analyse.as_deref() {
        Some(p) => preset(p)?,
        None => AnalysePreset::Recommended,
    };
    let composer = pun_config::composer::detect(&root)?;
    let style = match a.style {
        Some(s) => s,
        None if composer.locked("laravel/framework").is_some() => Style::Laravel,
        None => Style::Psr12,
    };
    let human = !agent::enabled();
    let interactive = !(a.yes || agent::enabled() || ci())
        && std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal();
    let old: Vec<String> = (migrate::detect(&root, Path::new("pint.json")).into_iter())
        .map(|d| d.1)
        .collect();
    if !old.is_empty() && !interactive {
        bail!(
            "Found {}: this project fits `pun migrate`, which turns {} into pun.toml. Nothing written.",
            and(&old),
            if old.len() == 1 { "it" } else { "them" }
        );
    }
    if human {
        println!(
            "\n  Pun · new project setup\n  {}\n",
            found(&root, &composer)
        );
    }
    let term = Term::stdout();
    if !old.is_empty() {
        let migrate = dialoguer::Confirm::with_theme(&theme())
            .with_prompt(format!(
                "Found {}. Migrate {} instead?",
                and(&old),
                if old.len() == 1 { "it" } else { "them" }
            ))
            .default(true)
            .interact_on(&term)?;
        if migrate {
            return migrate::run(&migrate::MigrateArgs::default());
        }
    }
    let (style, lint, analyse) = if interactive {
        ask(&term, style, lint, analyse)?
    } else {
        (style, lint, analyse)
    };
    let style = STYLES[style as usize];
    let extensions = pun_config::migrate::extensions(&composer);
    let extensions = if extensions.is_empty() {
        String::new()
    } else {
        format!("[extensions]\n{extensions}\n")
    };
    let toml = format!(
        "# Written by `pun init`.\n{extensions}[format]\npreset = \"{style}\"\n\n[lint]\npreset = \"{lint}\"\n\n[analyse]\npreset = \"{analyse}\"\n"
    );
    pun_config::from_toml(&root, &toml)?;
    std::fs::write(root.join("pun.toml"), &toml)?;
    if !human {
        let detail = [
            "pun.toml written".to_string(),
            format!("style {style}"),
            format!("lint {lint}"),
            format!("analyse {analyse}"),
            format!("extensions {}", none_or(&ids(&composer).join(" "))),
        ];
        let next = ["next: run `pun check`, `pun check --fix` before committing; add `pun check` to CI and composer scripts".to_string()];
        print!("{}", agent::render("ok", &detail, "", &next, false));
        return Ok(Exit::Success);
    }
    if !interactive {
        println!("Code style: {style}");
        println!("Lint preset: {lint}");
        println!("Analysis preset: {analyse}");
    }
    if let Some(eco) = ecosystems(&composer) {
        println!("\n  Extensions on from composer.lock: {eco}. See [extensions] in pun.toml.");
    }
    let r = Renderer::new(Look::stdout(ColorChoice::Auto), &root);
    println!("\n  {}", r.done("Wrote pun.toml").trim_end());
    println!("\n  Done. Run `pun check` any time; `pun check --fix` before you commit.");
    println!("  Add `pun check` to CI and your composer scripts.");
    Ok(Exit::Success)
}

/// The three questions, each starting on its default.
fn ask(
    term: &Term,
    style: Style,
    lint: LintPreset,
    analyse: AnalysePreset,
) -> Result<(Style, LintPreset, AnalysePreset)> {
    let theme = theme();
    let style = dialoguer::Select::with_theme(&theme)
        .with_prompt("Code style")
        .items(STYLES)
        .default(style as usize)
        .interact_on(term)?;
    let lint = dialoguer::Select::with_theme(&theme)
        .with_prompt("Lint preset")
        .items(LintPreset::ALL.map(LintPreset::name))
        .default(LintPreset::ALL.iter().position(|p| *p == lint).unwrap_or(1))
        .interact_on(term)?;
    let names = AnalysePreset::ALL.map(AnalysePreset::name);
    let analyse = dialoguer::Select::with_theme(&theme)
        .with_prompt("Analysis preset")
        .items(names)
        .default(
            AnalysePreset::ALL
                .iter()
                .position(|p| *p == analyse)
                .unwrap_or(2),
        )
        .interact_on(term)?;
    Ok((
        [Style::Laravel, Style::Psr12][style],
        LintPreset::ALL[lint],
        AnalysePreset::ALL[analyse],
    ))
}

/// The prompts' look (`? Code style ›`, then `✔ Code style · laravel`; context/plan/init.md §2), shared with
/// `pun migrate`'s checklist.
pub(crate) fn theme() -> dialoguer::theme::ColorfulTheme {
    dialoguer::theme::ColorfulTheme::default()
}

/// `--lint`, with schema.rs's wording for an unknown preset.
fn lint_preset(p: &str) -> Result<LintPreset> {
    LintPreset::parse(p).ok_or_else(|| {
        let names: Vec<&str> = LintPreset::ALL.map(LintPreset::name).to_vec();
        anyhow::anyhow!("--lint: `{p}` is not a lint preset; use {}", quoted(&names))
    })
}

/// `--analyse`, with schema.rs's wording for an unknown preset.
fn preset(p: &str) -> Result<AnalysePreset> {
    match AnalysePreset::parse(p) {
        Some(a) => Ok(a),
        None => bail!(
            "--analyse {p}: expected {}, or `phpstan/level_0` to `phpstan/level_10` or `phpstan/level_max`",
            quoted(&AnalysePreset::ALL.map(AnalysePreset::name))
        ),
    }
}

/// schema.rs's list: each item in backticks, comma-separated.
fn quoted(items: &[&str]) -> String {
    let q: Vec<String> = items.iter().map(|s| format!("`{s}`")).collect();
    q.join(", ")
}

/// `a`, `a and b`, `a, b and c`.
fn and(items: &[String]) -> String {
    match items {
        [rest @ .., last] if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => items.join(""),
    }
}

fn none_or(s: &str) -> String {
    if s.is_empty() {
        "none".into()
    } else {
        s.into()
    }
}

/// `CI` set to anything but empty, `0` or `false`, as CI services do.
fn ci() -> bool {
    std::env::var("CI").is_ok_and(|v| !matches!(v.as_str(), "" | "0" | "false"))
}

/// The header's second line: what composer.json and composer.lock say, and which of them were read.
fn found(root: &Path, c: &Composer) -> String {
    let version = |l: &Locked| match l.semver {
        Some(v) => v.major.to_string(),
        None => l.version.clone(),
    };
    let mut parts = vec![];
    if let Some(l) = c.locked("laravel/framework") {
        parts.push(format!("Laravel {}", version(l)));
    }
    if let Some(l) = c.locked("pestphp/pest") {
        parts.push(format!("Pest {}", version(l)));
    } else if let Some(l) = c.locked("phpunit/phpunit") {
        parts.push(format!("PHPUnit {}", version(l)));
    }
    if let Some(php) = &c.php {
        parts.push(format!("PHP {}", php.constraint));
    }
    if parts.is_empty() {
        return "No Laravel, Pest, PHPUnit or PHP version found in composer.json or composer.lock"
            .into();
    }
    let files: Vec<String> = (c.sources.iter())
        .map(|s| s.strip_prefix(root).unwrap_or(s).display().to_string())
        .collect();
    format!("Found {} ({})", parts.join(", "), files.join(", "))
}

/// The extensions init turns on, as the line after the questions names them.
fn ecosystems(c: &Composer) -> Option<String> {
    let names: Vec<String> = (pun_config::extensions::detected(c).into_iter())
        .map(|e| e.name.to_string())
        .collect();
    (!names.is_empty()).then(|| and(&names))
}

fn ids(c: &Composer) -> Vec<&'static str> {
    (pun_config::extensions::detected(c).into_iter())
        .map(|e| e.id)
        .collect()
}
