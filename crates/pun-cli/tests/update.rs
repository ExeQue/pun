//! `pun update`, `pun require` and `pun remove` on punphp-packages' install fixture, a project whose packages come
//! from a path repository, so nothing is downloaded. The expected composer.json and composer.lock texts are what
//! Composer 2.11.2 wrote for the same commands (`composer update`, `composer remove --dev acme/tools`).
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

/// Without a lock, `pun update` writes the composer.lock Composer wrote for the fixture; with one, it changes
/// nothing; `--dry-run` writes nothing.
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
}

/// `pun remove` edits composer.json and the lock as `composer remove --dev` does, and uninstalls the package.
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

/// In a migrated project, `pun remove` then `pun require` without a version puts pun.toml and pun.lock back as they
/// were: the version written is the `^1.2` Composer recommends for 1.2.0, and the lock's hash is pun.toml's.
/// A requirement that can't resolve leaves both files alone and says why.
#[test]
fn require_edits_pun_toml() {
    let d = fixture("require");
    let (out, err, code) = pun(&d, &["migrate", "--yes"]);
    assert_eq!(code, Some(0), "{out}{err}");
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

/// A workspace: the root's pun.toml lists the members, which share the root's pun.lock and vendor/. Commands run
/// in a member work on the whole workspace; `require` edits the member it runs in (or `-p`'s), writes the newest
/// version as `^major.minor`, and leaves both files as they were when the request doesn't resolve.
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
    assert!(!billing.join("vendor").exists() && !billing.join("pun.lock").exists());

    let (out, err, code) = pun(&billing, &["require", "ext/log"]);
    assert_eq!(code, Some(0), "{out}{err}");
    assert_eq!(
        read(&billing, "composer.json"),
        "{\n    \"name\": \"acme/billing\",\n    \"require\": {\n        \"acme/core\": \"*\",\n        \"ext/log\": \"^2.3\"\n    }\n}\n"
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
