//! `[analyse] php-rules` (prototype): the project's own PHPStan rule classes, run by one long-lived `php` worker
//! (`php_rules_worker.php`, protocol in its header) on PHPStan's container and reflection, with the expression types
//! Pun inferred. An opt-in exception to "no PHP fallback" (context/PLAN.md §2): the rules are the project's code.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Mutex;

use serde::Deserialize;
use serde_json::{Value, json};

const WORKER: &str = include_str!("php_rules_worker.php");

/// An error a project rule reported.
#[derive(Deserialize, Debug, PartialEq)]
pub(crate) struct RuleError {
    pub(crate) message: String,
    pub(crate) line: i64,
    pub(crate) identifier: Option<String>,
    pub(crate) tip: Option<String>,
    #[serde(rename = "nonIgnorable")]
    pub(crate) non_ignorable: bool,
}

struct Worker {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    token: String,
}

impl Worker {
    fn send(&mut self, message: &Value) -> Result<(), String> {
        writeln!(self.stdin, "{message}")
            .and_then(|()| self.stdin.flush())
            .map_err(|e| format!("can't write to the php-rules worker: {e}"))
    }

    /// The worker's next message; output that isn't the worker's goes to stderr.
    fn receive(&mut self) -> Result<Value, String> {
        let mut line = String::new();
        loop {
            line.clear();
            match self.stdout.read_line(&mut line) {
                Ok(0) => return Err("the php-rules worker stopped".to_string()),
                Ok(_) => {}
                Err(e) => return Err(format!("can't read from the php-rules worker: {e}")),
            }
            match line.split_once(&self.token) {
                Some((before, reply)) => {
                    let _ = std::io::stderr().write_all(before.as_bytes());
                    let reply: Value = serde_json::from_str(reply.trim_end())
                        .map_err(|e| format!("the php-rules worker sent {reply:?}: {e}"))?;
                    if let Some(e) = reply.get("error").and_then(Value::as_str) {
                        return Err(e.to_string());
                    }
                    return Ok(reply);
                }
                None => {
                    let _ = std::io::stderr().write_all(line.as_bytes());
                }
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The running worker, shared by the analysis threads.
pub struct PhpRules {
    // ponytail: one worker behind a lock serializes the rule phase of every file; a pool of workers if it shows.
    worker: Mutex<Option<Worker>>,
    failure: Mutex<Option<String>>,
    identifiers: Mutex<HashSet<String>>,
}

impl PhpRules {
    /// Starts `php` on the project in `root`: its vendor/autoload.php and vendor/phpstan/phpstan/phpstan.phar, the
    /// rules the `neon` files register, and `paths` as PHPStan's analysed paths.
    pub fn start(root: &Path, neon: &[PathBuf], paths: &[PathBuf]) -> Result<PhpRules, String> {
        let nanos = (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH))
            .map_or(0, |d| d.as_nanos());
        let token = format!("\u{1e}pun-php-rules-{}-{nanos}:", std::process::id());
        // `php -r`: the worker needs no file, which a containerized php (lerd) can't read from /tmp
        let code = WORKER.trim_start_matches("<?php");
        // no memory limit, as PHPStan's `--memory-limit=-1`: php's default 128M doesn't parse a large file
        let child = Command::new("php")
            .args(["-d", "memory_limit=-1", "-r", code])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn();
        let mut child = child.map_err(|e| format!("can't run php for the php-rules: {e}"))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            return Err("can't talk to the php-rules worker".to_string());
        };
        let mut worker = Worker {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            token: token.clone(),
        };
        let path = |p: &Path| p.to_string_lossy().into_owned();
        worker.send(&json!({
            "token": token,
            "root": path(root),
            "autoload": path(&root.join("vendor/autoload.php")),
            "phar": path(&root.join("vendor/phpstan/phpstan/phpstan.phar")),
            "neon": neon.iter().map(|n| path(&root.join(n))).collect::<Vec<_>>(),
            "paths": paths.iter().map(|p| path(p)).collect::<Vec<_>>(),
        }))?;
        worker.receive()?;
        Ok(PhpRules {
            worker: Mutex::new(Some(worker)),
            failure: Mutex::new(None),
            identifiers: Mutex::new(HashSet::new()),
        })
    }

    /// Runs the rules on `file`, or on the body of a trait it declares in the context of a class that uses it
    /// (`in_trait`: the class and the trait); `type_at(start, end)` answers the worker's type questions with a PHPDoc type. A
    /// failure stops the worker and is kept for [`PhpRules::failure`]: Pun can't report half the rules' errors.
    pub(crate) fn check(
        &self,
        file: &Path,
        in_trait: Option<(&str, &str)>,
        mut type_at: impl FnMut(u32, u32) -> Option<String>,
    ) -> Vec<RuleError> {
        let mut guard = self
            .worker
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(worker) = guard.as_mut() else {
            return Vec::new();
        };
        let mut ask = || -> Result<Vec<RuleError>, String> {
            let mut request = json!({"file": file.to_string_lossy()});
            if let Some((class, trait_name)) = in_trait {
                request["class"] = json!(class);
                request["trait"] = json!(trait_name);
            }
            worker.send(&request)?;
            loop {
                let mut reply = worker.receive()?;
                if let Some([start, end]) = reply
                    .get("type")
                    .and_then(|t| serde_json::from_value::<[u32; 2]>(t.clone()).ok())
                {
                    worker.send(&json!({"type": type_at(start, end)}))?;
                    continue;
                }
                let errors = reply.get_mut("errors").map(Value::take).unwrap_or_default();
                return serde_json::from_value(errors)
                    .map_err(|e| format!("the php-rules worker sent bad errors: {e}"));
            }
        };
        match ask() {
            Ok(errors) => {
                let mut ids = self
                    .identifiers
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                ids.extend(
                    errors
                        .iter()
                        .map(|e| e.identifier.clone().unwrap_or_default()),
                );
                errors
            }
            Err(e) => {
                *guard = None;
                let mut failure = self
                    .failure
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                failure.get_or_insert_with(|| format!("{}: {e}", file.display()));
                Vec::new()
            }
        }
    }

    /// Why the worker stopped during the run, if it did.
    pub fn failure(&self) -> Option<String> {
        self.failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Whether a project rule reported `identifier` (`""` for an error without one) in this run.
    pub fn reported(&self, identifier: &str) -> bool {
        (self
            .identifiers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner))
        .contains(identifier)
    }
}
