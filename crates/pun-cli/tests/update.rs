mod common;

use common::pty::pun;
use common::read;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    let d = common::scratch("update", name);
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../pun-packages/tests/fixtures/project/.");
    let copied = std::process::Command::new("cp")
        .arg("-R")
        .arg(source)
        .arg(&d)
        .status()
        .unwrap();
    assert!(copied.success());
    d
}

fn expected(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/update")
            .join(name),
    )
    .unwrap()
}

#[test]
fn update_writes_composers_lock() {
    let d = fixture("update");
    let composer_lock = read(&d, "composer.lock");
    std::fs::remove_file(d.join("composer.lock")).unwrap();
    let (out, err, code) = pun(&d, &["update", "--dry-run"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(err.contains(" + acme/lib 1.2.0\n"), "{err}");
    assert!(!d.join("composer.lock").exists());

    let (out, err, code) = pun(&d, &["update", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(read(&d, "composer.lock"), composer_lock);
    assert!(!d.join("vendor").exists());

    let (out, err, code) = pun(&d, &["update", "acme/lib", "acme/nope"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(
        err.contains(" ! acme/nope is not in the lock, so there is nothing of it to update\n")
            && err.contains("Nothing to change: the lock is up to date\n")
            && err.contains(" + packages: 2 installed\n"),
        "{err}"
    );
    assert_eq!(read(&d, "composer.lock"), composer_lock);
    assert!(d.join("vendor/composer/installed.php").exists());

    std::fs::remove_file(d.join("composer.json")).unwrap();
    let (_, err, code) = pun(&d, &["install"]);
    assert_eq!(code, Some(2));
    assert!(err.contains("no composer.json in "), "{err}");
}

#[test]
fn remove_matches_composer() {
    let d = fixture("remove");
    let (out, err, code) = pun(&d, &["install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(d.join("vendor/acme/tools").exists());
    let (out, err, code) = pun(&d, &["remove", "acme/tools", "acme/absent"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(
        err.contains(" ! acme/absent is not required, so there is nothing to remove\n")
            && err.contains(" - acme/tools 0.3.1\n"),
        "{err}"
    );
    assert_eq!(
        read(&d, "composer.json"),
        expected("remove-tools.composer.json")
    );
    assert_eq!(
        read(&d, "composer.lock"),
        expected("remove-tools.composer.lock")
    );
    assert!(!d.join("vendor/acme/tools").exists());
    let (_, err, code) = pun(&d, &["remove", "acme/absent"]);
    assert_eq!(code, Some(0), "{err}");
}

#[test]
fn require_edits_composer_json() {
    let d = fixture("require");
    let (json, lock) = (read(&d, "composer.json"), read(&d, "composer.lock"));

    let (out, err, code) = pun(&d, &["remove", "acme/lib", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(
        read(&d, "composer.json"),
        json.replace(",\n        \"acme/lib\": \"^1.2\"", "")
    );
    let (out, err, code) = pun(&d, &["require", "acme/lib"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(err.contains("Using version ^1.2 for acme/lib\n"), "{err}");
    assert_eq!(read(&d, "composer.json"), json);
    assert_eq!(read(&d, "composer.lock"), lock);
    assert!(d.join("vendor/acme/lib").exists());

    let (out, err, code) = pun(&d, &["require", "acme/lib:^9.0"]);
    assert_eq!(code, Some(2), "{out}{err}");
    assert!(
        err.contains("x The requirements can't be resolved:\n\n  Problem 1\n    - your project requires acme/lib ^9.0, but the only version found is 1.2.0\n"),
        "{err}"
    );
    assert_eq!(
        (read(&d, "composer.json"), read(&d, "composer.lock")),
        (json.clone(), lock)
    );

    let (_, err, code) = pun(&d, &["require", "--no-update", "--dev", "acme/new:^1.0"]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(
        read(&d, "composer.json"),
        json.replace(
            "\"acme/tools\": \"^0.3\"\n",
            "\"acme/tools\": \"^0.3\",\n        \"acme/new\": \"^1.0\"\n"
        )
    );
    for bad in ["nope", "acme/app"] {
        let (_, err, code) = pun(&d, &["require", bad]);
        assert_eq!(code, Some(2), "{err}");
    }
}

#[test]
fn workspace_members_share_one_lock() {
    let d = common::scratch("update", "workspace");
    common::write_files(
        &d,
        &[
            (
                "ext/log/composer.json",
                r#"{"name": "ext/log", "version": "2.3.0"}"#,
            ),
            ("repo/pun.toml", "[workspace]\nmembers = [\"packages/*\"]\n"),
            (
                "repo/composer.json",
                r#"{"repositories": [{"type": "path", "url": "../ext/log"}, {"packagist.org": false}]}"#,
            ),
            (
                "repo/packages/core/composer.json",
                "{\n    \"name\": \"acme/core\"\n}\n",
            ),
            (
                "repo/packages/billing/composer.json",
                "{\n    \"name\": \"acme/billing\",\n    \"require\": {\n        \"acme/core\": \"*\"\n    }\n}\n",
            ),
        ],
    );
    let (root, billing) = (d.join("repo"), d.join("repo/packages/billing"));
    let (out, err, code) = pun(&billing, &["install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    for (link, target) in [("billing", "packages/billing"), ("core", "packages/core")] {
        let path = root.join("vendor/acme").join(link);
        assert_eq!(
            std::fs::read_link(&path).unwrap(),
            Path::new("../..").join(target),
            "{link}"
        );
    }
    assert!(!billing.join("vendor").exists() && !billing.join("composer.lock").exists());

    let (out, err, code) = pun(&billing, &["require", "ext/log"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(
        read(&billing, "composer.json"),
        "{\n    \"name\": \"acme/billing\",\n    \"require\": {\n        \"acme/core\": \"*\",\n        \"ext/log\": \"^2.3\"\n    }\n}\n"
    );
    let lock = read(&root, "composer.lock");
    assert!(lock.contains("\"ext/log\": \"^2.3\""), "{lock}");
    assert!(root.join("vendor/ext/log").exists());

    let core = read(&root, "packages/core/composer.json");
    let (out, err, code) = pun(&root, &["require", "-p", "acme/core", "ext/log:^9"]);
    assert_eq!(code, Some(2), "{out}{err}");
    assert!(
        err.contains("ext/log ^9, but the only version found is 2.3.0"),
        "{err}"
    );
    assert_eq!(
        (
            read(&root, "packages/core/composer.json"),
            read(&root, "composer.lock")
        ),
        (core, lock)
    );

    let (_, err, code) = pun(&root, &["require", "-p", "acme/nope", "ext/log"]);
    assert_eq!(code, Some(2));
    assert!(
        err.contains(
            "acme/nope is not a workspace member; the members are acme/billing, acme/core"
        ),
        "{err}"
    );
}

/// An allowed Composer plugin runs through the plugin host, in the order Composer 2.11.2 logged on the same project
/// (tests/fixtures/plugins): listeners at priority 0 and up before the project's scripts and negative ones after,
/// `init` only for a plugin installed before the command started, `--no-scripts` keeping the plugins, a plugin
/// update activating the new version mid-command, and the plugin `allow-plugins` denies never loading.
#[test]
fn composer_plugins_run_as_composer_runs_them() {
    let d = common::scratch("update", "plugins");
    let copied = std::process::Command::new("cp")
        .arg("-R")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../pun-packages/tests/fixtures/plugins/."))
        .arg(&d)
        .status()
        .unwrap();
    assert!(copied.success());
    let cache = Path::new(env!("CARGO_TARGET_TMPDIR")).join("composer-cache");
    let home = d.join(".composer");
    let env = [
        ("COMPOSER_CACHE_DIR", cache.to_str().unwrap()),
        ("COMPOSER_HOME", home.to_str().unwrap()),
    ];
    let log = |args: &[&str]| {
        let (out, err, code) = common::pty::pun_with(&d, args, &env);
        assert_eq!(code, Some(0), "{out}{err}");
        let log = std::fs::read_to_string(d.join("events.log")).unwrap_or_default();
        let _ = std::fs::remove_file(d.join("events.log"));
        log
    };
    let installed = "pre-autoload-dump plugin 0 1\npost-autoload-dump plugin 10 1\npost-autoload-dump plugin 0 1\npost-autoload-dump script 1\npost-autoload-dump plugin -10 1\n";
    assert_eq!(
        log(&["install"]),
        format!(
            "activate acme/app\n{installed}post-install-cmd plugin 0 1\npost-install-cmd script 1\n"
        )
    );
    let started = "activate acme/app\ninit plugin 0 \n";
    assert_eq!(
        log(&["install"]),
        format!(
            "{started}pre-install-cmd plugin 0 1\n{installed}post-install-cmd plugin 0 1\npost-install-cmd script 1\n"
        )
    );
    assert_eq!(
        log(&["install", "--no-scripts"]),
        format!(
            "{started}pre-install-cmd plugin 0 1\n{}post-install-cmd plugin 0 1\n",
            installed.replace("post-autoload-dump script 1\n", "")
        )
    );
    assert_eq!(
        log(&["install", "--no-plugins"]),
        "post-autoload-dump script 1\npost-install-cmd script 1\n"
    );
    std::fs::write(d.join("fail"), "post-install-cmd echo").unwrap();
    let (out, err, code) = common::pty::pun_with(&d, &["install"], &env);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(out.contains("the recorder says hello"), "{out}");
    std::fs::remove_file(d.join("fail")).unwrap();
    std::fs::remove_file(d.join("events.log")).unwrap();
    let manifest = d.join("packages/recorder/composer.json");
    let text = std::fs::read_to_string(&manifest).unwrap();
    std::fs::write(&manifest, text.replace("\"1.0.0\"", "\"1.1.0\"")).unwrap();
    assert_eq!(
        log(&["update"]),
        format!(
            "{started}pre-update-cmd plugin 0 1\nactivate acme/app\n{installed}post-update-cmd plugin 0 1\n"
        )
    );
}
