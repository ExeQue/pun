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
    assert!(
        home.join("cache/x/acme/tool/latest/composer.lock")
            .is_file()
    );

    let punx = home.join("punx");
    std::os::unix::fs::symlink(pun, &punx).unwrap();
    let (out, err, code) = run(&punx, &work, &home, &["acme/tool:*", "other", "x"]);
    assert_eq!((out.as_str(), code), ("other x\n", Some(0)), "{err}");
    assert!(!err.contains("Installing"), "{err}");

    let (out, err, code) = run(&punx, &work, &home, &["acme/missing"]);
    assert_eq!((out.as_str(), code), ("", Some(2)), "{err}");
    assert!(!home.join("cache/x/acme/missing").exists());

    let (_, err, code) = run(&punx, &work, &home, &["acme/tool:^1.0 || >=9", "other"]);
    assert_eq!(code, Some(0), "{err}");
    let (out, err, code) = run(&punx, &work, &home, &["list"]);
    assert_eq!(
        (out.as_str(), code),
        (
            "acme/tool              1.0.0\nacme/tool:^1.0 || >=9  1.0.0\n",
            Some(0)
        ),
        "{err}"
    );
    let (out, err, code) = run(pun, &work, &home, &["x", "clean", "acme/tool:*"]);
    assert_eq!(
        (out.as_str(), code),
        ("Removed 1 install.\n", Some(0)),
        "{err}"
    );
    let (out, _, _) = run(&punx, &work, &home, &["list"]);
    assert_eq!(out, "acme/tool:^1.0 || >=9  1.0.0\n");
    let (out, err, code) = run(&punx, &work, &home, &["clean"]);
    assert_eq!(
        (out.as_str(), code),
        ("Removed 1 install.\n", Some(0)),
        "{err}"
    );
    assert!(!home.join("cache/x/acme").exists());
    let (out, _, _) = run(&punx, &work, &home, &["list"]);
    assert_eq!(out, "No packages are installed.\n");
    let (out, err, code) = run(&punx, &work, &home, &["clean", "not-a-package"]);
    assert_eq!((out.as_str(), code), ("", Some(2)), "{err}");
}
