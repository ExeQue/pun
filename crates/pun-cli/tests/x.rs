//! `pun x` and `punx` on a tool from a path repository that Composer's global config.json lists, so nothing is
//! downloaded. The tool has two commands; the expected outputs are what its scripts print.
mod common;

use common::{clean_env, write_files};
use std::path::Path;
use std::process::Command;

fn run(program: &Path, dir: &Path, home: &Path, args: &[&str]) -> (String, String, Option<i32>) {
    let out = clean_env(Command::new(program).args(args), &["PATH"])
        .current_dir(dir)
        .env("PUN_AGENT", "0")
        .env("COMPOSER_HOME", home.join("composer"))
        .env("PUN_CACHE_DIR", home.join("cache"))
        .output()
        .unwrap();
    let s = |b: Vec<u8>| String::from_utf8(b).unwrap();
    (s(out.stdout), s(out.stderr), out.status.code())
}

/// The first run installs the tool into Pun's cache and runs the command named after the package, in the caller's
/// directory and with its exit code; a later run under the name `punx` reuses the install and picks the command
/// its first argument names.
#[test]
fn runs_a_packages_command() {
    let home = common::scratch("x", "home");
    let tool = home.join("tool");
    let config = format!(
        r#"{{"repositories": [{{"type": "path", "url": "{}"}}, {{"packagist.org": false}}]}}"#,
        tool.display()
    );
    write_files(
        &home,
        &[
            ("composer/config.json", config.as_str()),
            (
                "tool/composer.json",
                r#"{"name": "acme/tool", "version": "1.0.0", "bin": ["bin/tool", "bin/other"]}"#,
            ),
            (
                "tool/bin/tool",
                "#!/usr/bin/env php\n<?php\necho 'tool ', implode(' ', array_slice($argv, 1)), ' in ', basename(getcwd()), \"\\n\";\nexit(3);\n",
            ),
            (
                "tool/bin/other",
                "#!/usr/bin/env php\n<?php\necho 'other ', implode(' ', array_slice($argv, 1)), \"\\n\";\n",
            ),
            ("work/.keep", ""),
        ],
    );
    let work = home.join("work");

    let pun = Path::new(env!("CARGO_BIN_EXE_pun"));
    let (out, err, code) = run(pun, &work, &home, &["x", "acme/tool", "one", "--two"]);
    assert_eq!(
        (out.as_str(), code),
        ("tool one --two in work\n", Some(3)),
        "{err}"
    );
    assert!(err.contains("Installing acme/tool into "), "{err}");
    assert!(home.join("cache/x/acme/tool/latest/pun.lock").is_file());

    let punx = home.join("punx");
    std::os::unix::fs::symlink(pun, &punx).unwrap();
    let (out, err, code) = run(&punx, &work, &home, &["acme/tool:*", "other", "x"]);
    assert_eq!((out.as_str(), code), ("other x\n", Some(0)), "{err}");
    assert!(!err.contains("Installing"), "{err}");

    let (out, err, code) = run(&punx, &work, &home, &["acme/missing"]);
    assert_eq!((out.as_str(), code), ("", Some(2)), "{err}");
    assert!(!home.join("cache/x/acme/missing").exists());
}
