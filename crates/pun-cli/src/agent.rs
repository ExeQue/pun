use std::fmt::Write as _;
use std::sync::OnceLock;
use std::time::Instant;

const CAP: usize = 20;

struct Mode {
    on: bool,
    cmd: &'static str,
    start: Instant,
}

static MODE: OnceLock<Mode> = OnceLock::new();

pub(crate) fn init(flag: Option<bool>, cmd: &'static str) {
    let on = flag
        .or_else(|| boolean("PUN_AGENT"))
        .or_else(|| boolean("PAO_DISABLE").filter(|d| *d).map(|_| false))
        .or_else(|| boolean("PAO_FORCE").filter(|f| *f))
        .unwrap_or_else(detected);
    let _ = MODE.set(Mode {
        on,
        cmd,
        start: Instant::now(),
    });
}

const OWN: &[&str] = &[
    "CURSOR_AGENT",
    "GROK_PLUGIN_ROOT",
    "GROK_PLUGIN_DATA",
    "GEMINI_CLI",
    "CODEX_SANDBOX",
    "CODEX_CI",
    "CODEX_THREAD_ID",
    "CODEX_SANDBOX_NETWORK_DISABLED",
    "AUGMENT_AGENT",
    "OPENCODE_CLIENT",
    "OPENCODE",
    "AMP_CURRENT_THREAD_ID",
    "CLAUDECODE",
    "CLAUDE_CODE",
    "REPL_ID",
    "COPILOT_MODEL",
    "COPILOT_ALLOW_ALL",
    "COPILOT_GITHUB_TOKEN",
    "COPILOT_CLI",
    "ANTIGRAVITY_AGENT",
    "ANTIGRAVITY_CLI_ALIAS",
    "PI_CODING_AGENT",
    "MATTERHORN_SESSION_ID",
    "JUNIE_DATA",
    "JUNIE_SHIM_PATH",
    "KIRO_AGENT_PATH",
    "KIMI_PLUGIN_ROOT",
    "CLINE_ACTIVE",
    "GOOSE_TERMINAL",
    "OPENCLAW_SHELL",
    "ZCODE_ENV",
];

fn detected() -> bool {
    OWN.iter()
        .any(|v| std::env::var(v).is_ok_and(|s| !s.trim().is_empty()))
        || std::path::Path::new("/opt/.devin").exists()
        || agent_detector::detect().is_some_and(|a| {
            a.name != "cursor" || a.source != agent_detector::DetectionSource::ToolEnvVar
        })
}

fn boolean(var: &str) -> Option<bool> {
    let v = std::env::var(var).ok()?.trim().to_ascii_lowercase();
    match v.as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

pub(crate) fn enabled() -> bool {
    MODE.get().is_some_and(|m| m.on)
}

fn mode() -> &'static Mode {
    MODE.get().expect("agent::init runs first")
}

pub(crate) fn cmd() -> &'static str {
    mode().cmd
}

pub(crate) fn render(
    result: &str,
    detail: &[String],
    checked: &str,
    items: &[String],
    all: bool,
) -> String {
    let m = mode();
    let mut o = format!("pun {}: {result}", m.cmd);
    for d in detail {
        let _ = write!(o, ", {d}");
    }
    let sep = if checked.is_empty() { "" } else { ", " };
    let _ = writeln!(
        o,
        " ({checked}{sep}{:.2}s)",
        m.start.elapsed().as_secs_f64()
    );
    let shown = if all {
        items.len()
    } else {
        items.len().min(CAP)
    };
    for i in &items[..shown] {
        let _ = writeln!(o, "{i}");
    }
    if shown < items.len() {
        let _ = writeln!(o, "+{} more; run with -v for all", items.len() - shown);
    }
    o
}

pub(crate) fn error(message: &str) -> String {
    let one: Vec<&str> = message
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    format!("pun {}: error: {}\n", cmd(), one.join(" "))
}

pub(crate) fn count(n: usize, what: &str) -> String {
    format!("{n} {what}{}", if n == 1 { "" } else { "s" })
}
