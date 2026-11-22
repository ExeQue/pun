use crate::run::{self, LintOpts, Stage, load_config};
use crate::{Exit, FormatArgs, root};
use anyhow::Result;
use notify::{EventKind, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasher, RandomState};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

const DEBOUNCE: Duration = Duration::from_millis(100);

pub(crate) fn run(mut a: FormatArgs) -> Result<Exit> {
    a.pun.watch = false;
    let started_by = parent();
    let root = root()?;
    report(&a, run::stage(&a, Stage::Format, &LintOpts::default()));
    let (tx, rx) = mpsc::channel();
    let mut watcher = notify::recommended_watcher(tx)?;
    watcher.watch(&root, RecursiveMode::Recursive)?;
    let rerun = FormatArgs {
        dirty: false,
        diff: None,
        pun: crate::PunArgs {
            staged: false,
            since: None,
            ..a.pun.clone()
        },
        ..a.clone()
    };
    let hasher = RandomState::new();
    let hash = |p: &PathBuf| std::fs::read(p).ok().map(|c| hasher.hash_one(&c));
    let mut done: HashMap<PathBuf, u64> = load_config(&a, &root)
        .and_then(|cfg| crate::finder::discover(&cfg, &a.paths))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|f| Some((f.clone(), hash(&f)?)))
        .collect();
    loop {
        let first = loop {
            match rx.recv_timeout(Duration::from_secs(1)) {
                _ if parent() != started_by => return Ok(Exit::Success),
                Ok(Ok(e)) if matches!(e.kind, EventKind::Access(_)) => {}
                Ok(e) => break e,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(Exit::Success),
            }
        };
        let mut changed = HashSet::new();
        let mut add = |e: notify::Result<notify::Event>| {
            let events = e
                .into_iter()
                .filter(|e| !matches!(e.kind, EventKind::Access(_)));
            changed.extend(events.flat_map(|e| e.paths));
        };
        add(first);
        while let Ok(e) = rx.recv_timeout(DEBOUNCE) {
            add(e);
        }
        if !root.exists() {
            return Ok(Exit::Success);
        }
        let files =
            match load_config(&a, &root).and_then(|cfg| crate::finder::discover(&cfg, &a.paths)) {
                Ok(f) => f,
                Err(e) => {
                    report(&a, Err(e));
                    continue;
                }
            };
        let targets: Vec<PathBuf> = files
            .into_iter()
            .filter(|f| changed.contains(f) && done.get(f).is_none_or(|&h| hash(f) != Some(h)))
            .collect();
        if targets.is_empty() {
            continue;
        }
        report(
            &rerun,
            run::stage(
                &FormatArgs {
                    paths: targets.clone(),
                    ..rerun.clone()
                },
                Stage::Format,
                &LintOpts::default(),
            ),
        );
        for t in targets {
            if let Some(h) = hash(&t) {
                done.insert(t, h);
            }
        }
    }
}

fn report(a: &FormatArgs, r: Result<Exit>) {
    if let Err(e) = r {
        crate::report::fail(e, &a.failure());
    }
}

#[cfg(unix)]
fn parent() -> u32 {
    std::os::unix::process::parent_id()
}

#[cfg(not(unix))]
fn parent() -> u32 {
    0
}
