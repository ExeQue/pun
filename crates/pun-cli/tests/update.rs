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

fn as_pun_lock(text: &str) -> String {
    pun_packages::lock::Lock::from_composer_lock(text)
        .unwrap()
        .to_toml()
}

fn migrated(name: &str) -> PathBuf {
    let d = fixture(name);
    let (out, err, code) = pun(&d, &["migrate", "--yes"]);
    assert_eq!(code, Some(0), "{out}{err}");
    d
}

#[test]
fn update_writes_composers_lock() {
    let d = fixture("update");
    for command in ["install", "update", "remove acme/lib"] {
        let args: Vec<&str> = command.split(' ').collect();
        let (out, err, code) = pun(&d, &args);
        assert_eq!(code, Some(2), "{out}{err}");
        assert!(
            err.contains("the dependencies are still in composer.json: run `pun migrate`"),
            "{err}"
        );
    }
    let (out, err, code) = pun(&d, &["migrate", "--yes"]);
    assert_eq!(code, Some(0), "{out}{err}");
    let lock = as_pun_lock(&read(&d, "composer.lock"));
    assert_eq!(read(&d, "pun.lock"), lock);
    std::fs::remove_file(d.join("pun.lock")).unwrap();
    let (_, err, code) = pun(&d, &["install"]);
    assert_eq!(code, Some(2));
    assert!(
        err.contains("there is a composer.lock but no pun.lock"),
        "{err}"
    );
    std::fs::remove_file(d.join("composer.lock")).unwrap();

    let (out, err, code) = pun(&d, &["update", "--dry-run"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(err.contains(" + acme/lib 1.2.0\n"), "{err}");
    assert!(!d.join("pun.lock").exists());

    let (out, err, code) = pun(&d, &["update", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(read(&d, "pun.lock"), lock);
    assert!(!d.join("vendor").exists() && !d.join("composer.lock").exists());

    let (out, err, code) = pun(&d, &["update", "acme/lib", "acme/nope"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(
        err.contains(" ! acme/nope is not in the lock, so there is nothing of it to update\n")
            && err.contains("Nothing to change: the lock is up to date\n")
            && err.contains(" + packages: 2 installed\n"),
        "{err}"
    );
    assert_eq!(read(&d, "pun.lock"), lock);
    assert!(d.join("vendor/composer/installed.php").exists());
}

#[test]
fn remove_matches_composer() {
    let d = migrated("remove");
    let toml = read(&d, "pun.toml");
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
        read(&d, "pun.toml"),
        toml.replace("\n[require-dev]\n\"acme/tools\" = \"^0.3\"\n", "")
    );
    assert_eq!(
        read(&d, "pun.lock"),
        as_pun_lock(&expected("remove-tools.composer.lock"))
    );
    assert!(!d.join("vendor/acme/tools").exists());
    let (_, err, code) = pun(&d, &["remove", "acme/absent"]);
    assert_eq!(code, Some(0), "{err}");
}

#[test]
fn require_edits_pun_toml() {
    let d = migrated("require");
    let (toml, lock) = (read(&d, "pun.toml"), read(&d, "pun.lock"));

    let (out, err, code) = pun(&d, &["remove", "acme/lib", "--no-install"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(
        read(&d, "pun.toml"),
        toml.replace("\"acme/lib\" = \"^1.2\"\n", "")
    );
    let (out, err, code) = pun(&d, &["require", "acme/lib"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert!(err.contains("Using version ^1.2 for acme/lib\n"), "{err}");
    assert_eq!(read(&d, "pun.toml"), toml);
    assert_eq!(read(&d, "pun.lock"), lock);
    assert!(d.join("vendor/acme/lib").exists());

    let (out, err, code) = pun(&d, &["require", "acme/lib:^9.0"]);
    assert_eq!(code, Some(2), "{out}{err}");
    assert!(
        err.contains("x The requirements can't be resolved:\n\n  Problem 1\n    - your project requires acme/lib ^9.0, but the only version found is 1.2.0\n"),
        "{err}"
    );
    assert_eq!(
        (read(&d, "pun.toml"), read(&d, "pun.lock")),
        (toml.clone(), lock)
    );

    let (_, err, code) = pun(&d, &["require", "--no-update", "--dev", "acme/new:^1.0"]);
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(
        read(&d, "pun.toml"),
        toml.replace(
            "\"acme/tools\" = \"^0.3\"\n",
            "\"acme/tools\" = \"^0.3\"\n\"acme/new\" = \"^1.0\"\n"
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
            (
                "repo/pun.toml",
                "[workspace]\nmembers = [\"packages/*\"]\n\n[[repositories]]\ntype = \"path\"\nurl = \"../ext/log\"\n\n[[repositories]]\n\"packagist.org\" = false\n",
            ),
            (
                "repo/packages/core/pun.toml",
                "[package]\nname = \"acme/core\"\n",
            ),
            (
                "repo/packages/billing/pun.toml",
                "[package]\nname = \"acme/billing\"\n\n[require]\n\"acme/core\" = \"*\"\n",
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
    assert!(!billing.join("vendor").exists() && !billing.join("pun.lock").exists());

    let (out, err, code) = pun(&billing, &["require", "ext/log"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(
        read(&billing, "pun.toml"),
        "[package]\nname = \"acme/billing\"\n\n[require]\n\"acme/core\" = \"*\"\n\"ext/log\" = \"^2.3\"\n"
    );
    let lock = read(&root, "pun.lock");
    assert!(lock.contains("require.\"ext/log\" = \"^2.3\""), "{lock}");
    assert!(root.join("vendor/ext/log").exists());

    let core = read(&root, "packages/core/pun.toml");
    let (out, err, code) = pun(&root, &["require", "-p", "acme/core", "ext/log:^9"]);
    assert_eq!(code, Some(2), "{out}{err}");
    assert!(
        err.contains("ext/log ^9, but the only version found is 2.3.0"),
        "{err}"
    );
    assert_eq!(
        (
            read(&root, "packages/core/pun.toml"),
            read(&root, "pun.lock")
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
