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
