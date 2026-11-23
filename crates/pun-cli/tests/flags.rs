//! Git selection and run-mode flags against the real Pint (`php .oracle/pint/builds/pint`). Every scenario is a bash
//! script that builds a throwaway git repo; it runs twice (one copy per tool, so readdir order agrees) and both tools
//! must print the same stdout/stderr (repo paths normalised; human runs: the same files named, tests/common), exit the
//! same and leave the same files. Pun runs
//! `pun migrate` on the config Pint reads (pint.json in the cwd, `--config`, `--preset`), then `pun format`, with
//! Pint's `--ansi` spelled `--color=always`. The Pun-only `--staged` is compared with Pint run on the same explicit
//! file list.
//! Skipped without the oracle.
mod common;

use common::{oracle, scratch};

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const BASE: &str = r#"
g() { git -c user.email=t@t -c user.name=t -c commit.gpgsign=false "$@"; }
bad() { mkdir -p "$(dirname "$1")"; printf '<?php\n$a = array(1,2);   \n' > "$1"; }
more() { printf '$b = array(3);\n' >> "$1"; }
git init -q -b main
mkdir -p app/Models
if [ -f "$CORPUS/app/Models/User.php" ]; then cp "$CORPUS/app/Models/User.php" app/Models/User.php; fi
for f in app/Mod.php app/Del.php app/Ren.php app/Staged.php app/Keep.php "app/With Space.php" sub/inner/S.php sub/T.php vendor/V.php; do bad "$f"; done
printf '<?php\n\n$a = [1];\n' > app/Clean.php
g add -A; g commit -qm init
"#;

const DIRTY: &str = r#"
g checkout -qb feature
more app/Mod.php; rm app/Del.php
g mv app/Ren.php app/Renamed.php
g mv app/Keep.php app/Kept.php; more app/Kept.php
more app/Staged.php; g add app/Staged.php; more app/Staged.php
more "app/With Space.php"; more vendor/V.php; more sub/T.php
bad app/New.php; bad "app/New Space.php"; bad sub/inner/U.php; bad newdir/deep/X.php
"#;

const DIVERGED: &str = r#"
g checkout -qb feature
more app/Mod.php; g commit -qam feat
g checkout -q main
more app/Keep.php; g commit -qam main1
bad app/MainOnly.php; g add -A; g commit -qm main2
g checkout -q feature
more sub/T.php; g add sub/T.php
more sub/inner/S.php
bad sub/Untr.php; bad "sub/Un tr.php"
g mv app/Ren.php app/Renamed.php; g rm -q app/Del.php
"#;

#[derive(Debug, PartialEq)]
struct Run {
    stdout: String,
    stderr: String,
    code: Option<i32>,
    files: BTreeMap<String, Vec<u8>>,
}

fn run(
    program: &[&str],
    script: &str,
    sub: &str,
    args: &[&str],
    stdin: &[u8],
    migrate: Option<&[&str]>,
) -> Run {
    thread_local! { static REPO: PathBuf = scratch("flags", "repo"); }
    let dir = REPO.with(PathBuf::clone);
    std::fs::remove_dir_all(&dir).unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/laravel-laravel");
    let ok = Command::new("bash")
        .args(["-ec", &format!("{BASE}\n{script}")])
        .current_dir(&dir)
        .env("CORPUS", corpus)
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "setup: {}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let home = dir.join("home");
    std::fs::create_dir_all(&home).unwrap();
    if let Some(m) = migrate {
        let out = Command::new(env!("CARGO_BIN_EXE_pun"))
            .arg("migrate")
            .args(m)
            .env("PUN_AGENT", "0")
            .current_dir(dir.join(sub))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let mut c = Command::new(program[0]);
    c.args(&program[1..]).args(args).current_dir(dir.join(sub));
    common::clean_env(&mut c, &["PATH"]);
    c.env("HOME", &home)
        .env("PUN_AGENT", "0")
        .env("TMPDIR", &home)
        .env("COLUMNS", "200")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let out = child.wait_with_output().unwrap();
    let files = common::snapshot(&dir, |r| {
        r == ".git" || r == "home" || r.ends_with("pun.toml")
    });
    let root = dir.to_string_lossy().into_owned();
    let norm = |b: &[u8]| {
        String::from_utf8_lossy(b)
            .replace(&root, "ROOT")
            .replace(&root.replace('/', "\\/"), "ROOT")
    };
    Run {
        stdout: norm(&out.stdout),
        stderr: norm(&out.stderr),
        code: out.status.code(),
        files,
    }
}

struct Cx {
    oracle: String,
}

impl Cx {
    fn new() -> Option<Self> {
        let oracle = oracle()?.to_string_lossy().into_owned();
        Some(Cx { oracle })
    }
    fn pint(&self, script: &str, sub: &str, args: &[&str], stdin: &[u8]) -> Run {
        run(&["php", &self.oracle], script, sub, args, stdin, None)
    }
    #[expect(clippy::unused_self, reason = "called like `pint`, its counterpart")]
    fn ours(&self, script: &str, sub: &str, args: &[&str], stdin: &[u8]) -> Run {
        let config = |a: &&str| a.starts_with("--config=") || a.starts_with("--preset=");
        let pun = [env!("CARGO_BIN_EXE_pun"), "format"];
        if args.contains(&"--no-config") {
            return run(&pun, script, sub, args, stdin, None);
        }
        let mut args: Vec<&str> = (args.iter())
            .map(|a| if *a == "--ansi" { "--color=always" } else { a })
            .collect();
        if let Some(i) = args.iter().position(|a| *a == "--config") {
            let v = args.remove(i + 1);
            args[i] = Box::leak(format!("--config={v}").into_boxed_str());
        }
        let (migrate, rest): (Vec<&str>, Vec<&str>) = args.into_iter().partition(config);
        run(&pun, script, sub, &rest, stdin, Some(&migrate))
    }
    fn same_as(
        &self,
        script: &str,
        sub: &str,
        args: &[&str],
        pint_args: &[&str],
        stdin: &[u8],
    ) -> Run {
        let want = self.pint(script, sub, pint_args, stdin);
        let mut want = want;
        let mut got = self.ours(script, sub, args, stdin);
        if common::human(args) {
            (want.stdout, got.stdout) = common::human_pair(
                &want.stdout,
                &got.stdout,
                want.files.keys(),
                got.files.keys(),
            );
        }
        assert_eq!(got, want, "{sub:?} {args:?}");
        want
    }
    fn same(&self, script: &str, sub: &str, args: &[&str]) -> Run {
        self.same_as(script, sub, args, args, b"")
    }
    fn same_error(&self, script: &str, args: &[&str], message: &str, pun: &str) {
        let mut want = self.pint(script, "", args, b"");
        let mut got = self.ours(script, "", args, b"");
        let squash = |s: &str| s.split_whitespace().collect::<String>();
        assert_eq!(want.code, Some(1), "{args:?}: {want:?}");
        assert!(
            squash(&want.stdout).contains(&squash(message)),
            "{args:?}: pint said {}",
            want.stdout
        );
        assert_eq!(got.code, Some(2), "{args:?}: {got:?}");
        if common::human(args) {
            (want.stdout, got.stdout) = common::human_pair(
                &want.stdout,
                &got.stdout,
                want.files.keys(),
                got.files.keys(),
            );
            assert_eq!(common::strip_ansi(&got.stderr), pun, "{args:?}");
            if !want.stdout.starts_with("named: ") {
                want.stdout = String::new();
            }
            got.stderr.clone_from(&want.stderr);
        }
        got.code = want.code;
        assert_eq!(got, want, "{args:?}");
    }
}

/// The files a `--format=json` report names, sorted.
fn listed(r: &Run) -> Vec<String> {
    let report: serde_json::Value = serde_json::from_str(&r.stdout).unwrap();
    let mut v: Vec<String> = (report["files"].as_array().unwrap().iter())
        .map(|f| f["name"].as_str().unwrap().to_string())
        .collect();
    v.sort_unstable();
    v
}

#[test]
fn dirty_matches_pint() {
    let Some(cx) = Cx::new() else { return };
    let r = cx.same(DIRTY, "", &["--dirty", "--test", "--format=json"]);
    assert_eq!(
        listed(&r),
        [
            "app/Mod.php",
            "app/New.php",
            "app/Renamed.php",
            "app/Staged.php",
            "newdir/deep/X.php",
            "sub/T.php",
            "sub/inner/U.php"
        ]
    );
    cx.same(DIRTY, "", &["--dirty"]);
    cx.same(DIRTY, "", &["--dirty", "app/Clean.php"]);
    let r = cx.same(DIRTY, "sub", &["--dirty", "--test", "--format=json"]);
    assert_eq!(listed(&r), ["T.php", "inner/U.php"]);
    cx.same("", "", &["--dirty"]);
    let message = "The [--dirty] option is only available when using Git.";
    cx.same_error("rm -rf .git", &["--dirty", "--format=json"], message, "");
    cx.same_error(
        "rm -rf .git",
        &["--dirty"],
        message,
        &format!("x {message}\n"),
    );
}

#[test]
fn diff_matches_pint() {
    let Some(cx) = Cx::new() else { return };
    let r = cx.same(DIVERGED, "", &["--diff=main", "--test", "--format=json"]);
    assert_eq!(
        listed(&r),
        [
            "app/Mod.php",
            "app/Renamed.php",
            "sub/T.php",
            "sub/Un tr.php",
            "sub/Untr.php",
            "sub/inner/S.php"
        ]
    );
    cx.same(DIVERGED, "", &["--diff=main"]);
    let r = cx.same(DIVERGED, "sub", &["--diff=main", "--test", "--format=json"]);
    assert_eq!(listed(&r), ["Un tr.php", "Untr.php"]);
    let message = "The [--diff] option is only available when using Git.";
    cx.same_error(DIVERGED, &["--diff=nope", "--format=json"], message, "");
    cx.same(
        DIVERGED,
        "",
        &["--diff=main", "--dirty", "--test", "--format=json"],
    );
}

#[test]
fn git_selection_resolves_symlinks_like_pint() {
    let Some(cx) = Cx::new() else { return };
    let link = "ln -s Mod.php app/Link.php; ln -s ../vendor/V.php app/VLink.php";
    let r = cx.same(link, "", &["--dirty"]);
    assert_eq!(r.files["app/Mod.php"], b"<?php\n\n$a = [1, 2];\n");
    cx.same(link, "", &["--diff=main", "--test", "--format=json"]);
    cx.same("ln -s Missing.php app/Dangling.php", "", &["--dirty"]);
}

#[test]
fn repair_matches_pint() {
    let Some(cx) = Cx::new() else { return };
    let broken = "printf '<?php\\nfunction (\\n' > app/Broken.php";
    for args in [
        &["--repair"][..],
        &["--repair", "app/Clean.php"],
        &["--repair", "--format=json"],
        &["--repair", "app/Broken.php"],
    ] {
        cx.same(broken, "", args);
    }
}

#[test]
fn max_processes_matches_pint() {
    let Some(cx) = Cx::new() else { return };
    for args in [
        &["--max-processes=1", "--test"][..],
        &["--max-processes=3", "--test"],
    ] {
        cx.same("", "", args);
    }
}

#[test]
fn config_flags_match_pint() {
    let Some(cx) = Cx::new() else { return };
    let cfg = r#"mkdir -p cfg; printf '{"preset":"psr12","exclude":["sub"]}' > cfg/other.json
printf '{"preset":"empty","rules":{"array_syntax":true},"notPath":["app/Keep.php"]}' > pint.json"#;
    for (sub, args) in [
        ("", &["--test"][..]),
        ("", &["--no-config", "--test"]),
        ("", &["--config=cfg/other.json"]),
        ("", &["--config", "cfg/other.json", "--no-config", "--test"]),
        ("", &["--no-config", "--preset=psr12", "--test"]),
        ("sub", &["--config=../cfg/other.json", "--test"]),
        ("sub", &["--test"]),
        ("", &["--config=missing.json"]),
    ] {
        cx.same(cfg, sub, args);
    }
    let bad = &format!("{cfg}\nprintf 'not json' > bad.json");
    let r = cx.pint(bad, "", &["--config=bad.json"], b"");
    assert_eq!(r.code, Some(1));
    assert!(
        r.stdout
            .contains("The configuration file [bad.json] is not valid JSON."),
        "{}",
        r.stdout
    );
    let r = run(
        &[env!("CARGO_BIN_EXE_pun"), "migrate", "--config=bad.json"],
        bad,
        "",
        &[],
        b"",
        None,
    );
    assert_eq!(r.code, Some(2));
    assert!(
        r.stderr
            .starts_with("x The configuration file [ROOT/bad.json] is not valid JSON."),
        "{}",
        r.stderr
    );
}

#[test]
fn output_to_file_matches_pint() {
    let Some(cx) = Cx::new() else { return };
    for args in [
        &["--test", "--output-to-file=r.json", "--output-format=json"][..],
        &["--output-to-file=r.json", "--output-format=json"],
        &["--test", "--output-to-file=r.txt"],
        &["--test", "--output-to-file=r.txt", "--format=junit"],
        &[
            "--test",
            "--output-to-file=r.xml",
            "--format=json",
            "--output-format=checkstyle",
        ],
        &["--test", "--output-to-file=r.x", "--output-format=gitlab"],
        &[
            "--test",
            "-q",
            "--output-to-file=r.x",
            "--output-format=junit",
        ],
        &["--dirty", "--output-to-file=r.json", "--output-format=json"],
    ] {
        cx.same("bad app/New.php", "", args);
    }
    let formats = "use json, agent, checkstyle, junit, gitlab or github";
    for (args, message, pun) in [
        (
            &["--test", "--output-to-file=r.x", "--output-format=nope"][..],
            "Format [nope] is not supported.".to_string(),
            format!("x Format [nope] is not supported.\n  {formats}\n"),
        ),
        (
            &[
                "--test",
                "-q",
                "--output-to-file=r.x",
                "--output-format=nope",
            ],
            "Format [nope] is not supported.".into(),
            String::new(),
        ),
        (
            &[
                "--test",
                "--output-to-file=nodir/r.json",
                "--output-format=json",
            ],
            "file_put_contents(nodir/r.json): Failed to open stream: No such file or directory"
                .into(),
            "x can't write the report to nodir/r.json: No such file or directory\n".into(),
        ),
        (
            &[
                "--test",
                "--output-to-file=app",
                "--output-format=json",
                "--ansi",
            ],
            "file_put_contents(app): Failed to open stream: Is a directory".into(),
            "x can't write the report to app: Is a directory\n".into(),
        ),
        (
            &["--test", "--format=json", "--output-to-file=app"],
            "file_put_contents(app): Failed to open stream: Is a directory".into(),
            String::new(),
        ),
    ] {
        cx.same_error("bad app/New.php", args, &message, &pun);
    }
}

#[test]
fn stdin_matches_pint() {
    let Some(cx) = Cx::new() else { return };
    let cfg = r#"printf '{"preset":"empty","rules":{"array_syntax":true},"notPath":["app/Keep.php"]}' > pint.json"#;
    for input in [
        &b"<?php\n$a = array(1,2);   \n"[..],
        b"<?php\nfunction (\n",
        b"",
    ] {
        for (args, pint_args) in [
            (&["-"][..], &["-"][..]),
            (&["-", "--test"], &["-", "--test"]),
            (&["-", "--no-config"], &["-", "--no-config"]),
            (
                &["--stdin-filename=app/X.php"],
                &["--stdin-filename=app/X.php"],
            ),
            (
                &["-", "--stdin-filename=app/Keep.php"],
                &["-", "--stdin-filename=app/Keep.php"],
            ),
            (
                &["--stdin-filename=storage/a/X.php", "-"],
                &["--stdin-filename=storage/a/X.php", "-"],
            ),
            (
                &["--stdin-filename=app/X.php", "--repair"],
                &["--stdin-filename=app/X.php", "--repair"],
            ),
        ] {
            cx.same_as(cfg, "", args, pint_args, input);
        }
    }
    let cfg =
        r#"printf '{"preset":"empty","rules":{"combine_consecutive_unsets":true}}' > pint.json"#;
    let input = b"<?php unset($a ); unset( $b );\nunset($x, ); unset( $y);";
    let r = cx.same_as(cfg, "", &["-"], &["-"], input);
    assert!(
        r.stdout.starts_with("<?php unset($a ); unset( $b );") && r.stderr.is_empty(),
        "{r:?}"
    );
    let pun = [env!("CARGO_BIN_EXE_pun"), "check", "--fix"];
    assert_eq!(run(&pun, cfg, "", &["-"], input, Some(&[])), r);
}

#[test]
fn staged_matches_pint_on_the_same_files() {
    let Some(cx) = Cx::new() else { return };
    let json = ["--test", "--format=json"];
    let files = ["app/Renamed.php", "sub/T.php"];
    let got = cx.ours(DIVERGED, "", &[&json[..], &["--staged"]].concat(), b"");
    let want = cx.pint(DIVERGED, "", &[&json[..], &files].concat(), b"");
    assert_eq!((listed(&got), got.code), (listed(&want), want.code));
    assert_eq!(listed(&got), files);
}
