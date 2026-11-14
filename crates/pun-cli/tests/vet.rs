//! `pun vet` on a hand-written lock: a tagged package from `require`, a branch from `require-dev`, and a path
//! package, which is the project's own and needs no review. Nothing is downloaded.
mod common;

use common::pty::pun;
use common::{read, write_files};
use std::path::Path;

fn lock(lib_version: &str) -> String {
    format!(
        r#"{{
    "content-hash": "0",
    "packages": [
        {{"name": "acme/lib", "version": "{lib_version}", "type": "library",
          "dist": {{"type": "zip", "url": "https://example.invalid/lib.zip", "reference": "aaa"}}}},
        {{"name": "acme/local", "version": "1.0.0", "dist": {{"type": "path", "url": "../local", "reference": "bbb"}}}}
    ],
    "packages-dev": [
        {{"name": "acme/dev", "version": "dev-main", "type": "library",
          "source": {{"type": "git", "url": "https://example.invalid/dev.git", "reference": "0123abcd"}},
          "dist": {{"type": "zip", "url": "https://example.invalid/dev.zip", "reference": "0123abcd"}}}}
    ],
    "aliases": [], "minimum-stability": "stable", "stability-flags": {{}}, "prefer-stable": false,
    "prefer-lowest": false, "platform": {{}}, "platform-dev": {{}}, "plugin-api-version": "2.6.0"
}}"#
    )
}

fn vet(d: &Path, args: &[&str]) -> (String, String, Option<i32>) {
    let mut all = vec!["vet"];
    all.extend(args);
    pun(d, &all)
}

/// `init` exempts every package at its locked version and criterion; a new version needs a review of the changes
/// from the exempted one (`v1.2.0` and `1.2.0` being one version); a full audit replaces an exemption, which
/// `prune` then removes, while it keeps the exemption a delta audit starts from.
#[test]
fn vets_the_lock() {
    let d = common::scratch("vet", "lock");
    let lib = lock("v1.2.0");
    write_files(
        &d,
        &[
            (
                "composer.json",
                r#"{"require": {"acme/lib": "^1.0", "acme/local": "*"}, "require-dev": {"acme/dev": "dev-main"}}"#,
            ),
            ("composer.lock", lib.as_str()),
        ],
    );
    let (out, err, code) = vet(&d, &[]);
    assert_eq!(code, Some(2), "{out}");
    assert!(err.contains("run `pun vet init`"), "{err}");

    let (_, err, code) = vet(&d, &["init"]);
    assert_eq!(code, Some(0), "{err}");
    let config = read(&d, "supply-chain/config.toml");
    for line in [
        "[[exemptions.\"acme/dev\"]]\nversion = \"dev-main#0123abcd\"\ncriteria = \"safe-to-run\"\n",
        "[[exemptions.\"acme/lib\"]]\nversion = \"v1.2.0\"\ncriteria = \"safe-to-deploy\"\n",
    ] {
        assert!(config.contains(line), "{config}");
    }
    assert!(!config.contains("acme/local"), "{config}");
    let (out, _, code) = vet(&d, &[]);
    assert_eq!(
        (out.as_str(), code),
        ("ok Vetted 2 packages: 0 audited, 2 exempted\n", Some(0))
    );

    std::fs::write(d.join("composer.lock"), lock("v1.3.0")).unwrap();
    let (out, _, code) = vet(&d, &[]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.starts_with(
            "x 1 of 2 packages is not vetted:\n\n  acme/lib v1.3.0 needs safe-to-deploy\n"
        ) && out.contains("pun vet diff acme/lib v1.2.0 v1.3.0"),
        "{out}"
    );

    for args in [
        ["certify", "acme/lib", "1.2.0", "1.3.0", "--who", "Ada"].as_slice(),
        &["certify", "acme/dev", "dev-main#0123abcd", "--who", "Ada"],
    ] {
        let (_, err, code) = vet(&d, args);
        assert_eq!(code, Some(0), "{err}");
    }
    let audits = read(&d, "supply-chain/audits.toml");
    assert!(
        audits.contains("[[audits.\"acme/dev\"]]\nwho = \"Ada\"\ncriteria = \"safe-to-run\"\nversion = \"dev-main#0123abcd\"\n"),
        "{audits}"
    );
    let (out, _, code) = vet(&d, &[]);
    assert_eq!(
        (out.as_str(), code),
        ("ok Vetted 2 packages: 1 audited, 1 exempted\n", Some(0))
    );

    let (out, _, code) = vet(&d, &["prune"]);
    assert_eq!(
        (out.as_str(), code),
        ("Removed 1 exemption the lock no longer needs\n", Some(0))
    );
    let config = read(&d, "supply-chain/config.toml");
    assert!(
        !config.contains("acme/dev") && config.contains("acme/lib"),
        "{config}"
    );

    // An imported audit file, here a path from supply-chain/, vets what it audits; imports.lock keeps the audits of
    // the locked packages only.
    write_files(
        &d,
        &[(
            "shared/audits.toml",
            "[[audits.\"acme/lib\"]]\ncriteria = \"safe-to-deploy\"\nversion = \"1.3.0\"\n\n[[audits.\"not/locked\"]]\ncriteria = \"safe-to-run\"\nversion = \"1.0.0\"\n",
        )],
    );
    let mut config = read(&d, "supply-chain/config.toml");
    config.push_str("\n[imports.friends]\nurl = \"../shared/audits.toml\"\n");
    std::fs::write(d.join("supply-chain/config.toml"), config).unwrap();
    let (out, _, code) = vet(&d, &[]);
    assert_eq!(
        (out.as_str(), code),
        ("ok Vetted 2 packages: 2 audited, 0 exempted\n", Some(0))
    );
    let locked = read(&d, "supply-chain/imports.lock");
    assert!(
        locked.contains("[[imports.friends.audits.\"acme/lib\"]]")
            && !locked.contains("not/locked"),
        "{locked}"
    );
    let (out, _, _) = vet(&d, &["prune"]);
    assert_eq!(out, "Removed 1 exemption the lock no longer needs\n");
}
