mod common;

use common::pty::{pun, pun_with};
use common::{read, write_files};
use std::path::Path;

fn lock(lib_version: &str) -> String {
    lock_with(lib_version, "https://example.invalid/lib.tar")
}

fn lock_with(lib_version: &str, lib_url: &str) -> String {
    format!(
        r#"{{
    "content-hash": "0",
    "packages": [
        {{"name": "acme/lib", "version": "{lib_version}", "type": "library",
          "dist": {{"type": "tar", "url": "{lib_url}", "reference": "aaa"}}}},
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

const MANIFEST: &str = r#"{"require": {"acme/lib": "^1.0", "acme/local": "*"}, "require-dev": {"acme/dev": "dev-main"}}"#;

/// `pun vet` and `init` need a lock, and `init` a supply-chain/ it can create, without overwriting one.
#[test]
fn vet_init_needs_a_lock_and_a_new_supply_chain() {
    let d = common::scratch("vet", "init");
    write_files(&d, &[("composer.json", MANIFEST)]);
    for args in [[].as_slice(), &["init"]] {
        let (_, err, code) = vet(&d, args);
        assert_eq!(code, Some(2), "{err}");
        assert!(
            err.contains("no lock to vet: run `pun install` or `pun update` first"),
            "{err}"
        );
    }
    std::fs::write(d.join("composer.lock"), lock("v1.2.0")).unwrap();
    std::os::unix::fs::symlink(d.join("nowhere/supply-chain"), d.join("supply-chain")).unwrap();
    let (_, err, code) = vet(&d, &["init"]);
    assert_eq!(code, Some(2), "{err}");
    let supply_chain = d.join("supply-chain");
    assert!(
        err.contains(&format!("can't create {}", supply_chain.display())),
        "{err}"
    );
    std::fs::remove_file(&supply_chain).unwrap();
    let (_, err, code) = vet(&d, &["init"]);
    assert_eq!(code, Some(0), "{err}");
    std::fs::write(d.join("supply-chain/audits.toml"), "# mine\n").unwrap();
    let (_, err, code) = vet(&d, &["init"]);
    assert_eq!(code, Some(2), "{err}");
    assert!(err.contains("supply-chain/ exists already"), "{err}");
    assert_eq!(read(&d, "supply-chain/audits.toml"), "# mine\n");
}

#[test]
fn vets_the_lock() {
    let d = common::scratch("vet", "lock");
    let lib = lock("v1.2.0");
    write_files(
        &d,
        &[("composer.json", MANIFEST), ("composer.lock", lib.as_str())],
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

    let (_, err, code) = vet(
        &d,
        &["certify", "acme/lib", "1.2.0", "1.3.0", "--who", "Ada"],
    );
    assert_eq!(code, Some(0), "{err}");
    let (out, err, code) = git_vet(
        &d,
        "[user]\n\tname = Ada Lovelace\n\temail = ada@example.com\n",
        &["certify", "acme/dev", "dev-main#0123abcd"],
    );
    assert_eq!(code, Some(0), "{err}");
    assert_eq!(
        out,
        "Recorded your safe-to-run review of acme/dev dev-main#0123abcd\n"
    );
    let audits = read(&d, "supply-chain/audits.toml");
    assert!(
        audits.contains("[[audits.\"acme/dev\"]]\nwho = \"Ada Lovelace <ada@example.com>\"\ncriteria = \"safe-to-run\"\nversion = \"dev-main#0123abcd\"\n"),
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

/// An import changes only with `fetch-imports`, and until then the check reads imports.lock, offline; a package no
/// review reaches gets `inspect`; `prune` drops the exemptions of packages the lock no longer has; and `certify`'s
/// reviewer is git's `user.name` alone when git has no `user.email`.
#[test]
fn vet_keeps_imports_until_fetched() {
    let d = common::scratch("vet", "fetch");
    write_files(
        &d,
        &[
            ("composer.json", MANIFEST),
            ("composer.lock", &lock("v1.3.0")),
            (
                "supply-chain/config.toml",
                "[[exemptions.\"acme/dev\"]]\nversion = \"dev-main#0123abcd\"\ncriteria = \"safe-to-run\"\n\n[imports.friends]\nurl = \"../shared/audits.toml\"\n",
            ),
            (
                "shared/audits.toml",
                "[[audits.\"acme/lib\"]]\ncriteria = \"safe-to-deploy\"\nversion = \"1.3.0\"\n",
            ),
        ],
    );
    let vetted = "ok Vetted 2 packages: 1 audited, 1 exempted\n";
    let (out, _, code) = vet(&d, &[]);
    assert_eq!((out.as_str(), code), (vetted, Some(0)));
    let locked = read(&d, "supply-chain/imports.lock");
    assert!(
        locked.contains("[[imports.friends.audits.\"acme/lib\"]]"),
        "{locked}"
    );
    write_files(
        &d,
        &[(
            "shared/audits.toml",
            "[[audits.\"not/locked\"]]\ncriteria = \"safe-to-run\"\nversion = \"1.0.0\"\n",
        )],
    );
    let (out, _, code) = vet(&d, &[]);
    assert_eq!((out.as_str(), code), (vetted, Some(0)));
    assert_eq!(read(&d, "supply-chain/imports.lock"), locked);
    let (out, _, code) = vet(&d, &["fetch-imports"]);
    assert_eq!((out.as_str(), code), ("Fetched 1 import\n", Some(0)));
    assert!(!read(&d, "supply-chain/imports.lock").contains("acme/lib"));
    let (out, _, code) = vet(&d, &[]);
    assert_eq!(code, Some(1), "{out}");
    assert!(
        out.starts_with(
            "x 1 of 2 packages is not vetted:\n\n  acme/lib v1.3.0 needs safe-to-deploy\n"
        ) && out.contains("review it: pun vet inspect acme/lib v1.3.0\n"),
        "{out}"
    );

    let mut config = read(&d, "supply-chain/config.toml");
    config.push_str(
        "\n[[exemptions.\"acme/gone\"]]\nversion = \"1.0.0\"\ncriteria = \"safe-to-run\"\n",
    );
    std::fs::write(d.join("supply-chain/config.toml"), config).unwrap();
    let (out, _, _) = vet(&d, &["prune"]);
    assert_eq!(out, "Removed 1 exemption the lock no longer needs\n");
    assert!(!read(&d, "supply-chain/config.toml").contains("acme/gone"));

    let (_, err, code) = git_vet(
        &d,
        "[user]\n\tname = Ada Lovelace\n",
        &["certify", "acme/lib", "v1.3.0"],
    );
    assert_eq!(code, Some(0), "{err}");
    assert!(
        read(&d, "supply-chain/audits.toml").ends_with("[[audits.\"acme/lib\"]]\nwho = \"Ada Lovelace\"\ncriteria = \"safe-to-deploy\"\nversion = \"v1.3.0\"\n"),
        "{}",
        read(&d, "supply-chain/audits.toml")
    );
    let (out, _, code) = vet(&d, &[]);
    assert_eq!((out.as_str(), code), (vetted, Some(0)));
}

/// `pun vet` with git's global config set to `gitconfig` and no system config.
fn git_vet(d: &Path, gitconfig: &str, args: &[&str]) -> (String, String, Option<i32>) {
    let config = d.join("gitconfig");
    std::fs::write(&config, gitconfig).unwrap();
    let mut all = vec!["vet"];
    all.extend(args);
    pun_with(
        d,
        &all,
        &[
            ("GIT_CONFIG_GLOBAL", config.to_str().unwrap()),
            ("GIT_CONFIG_NOSYSTEM", "1"),
        ],
    )
}

/// Serves the routes (path, body) that `routes` makes from the server's URL, on 127.0.0.1 until the test binary
/// exits; any other path is a 404.
fn serve(routes: impl FnOnce(&str) -> Vec<(&'static str, Vec<u8>)>) -> String {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let routes = routes(&base);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let (mut head, mut byte) = (vec![], [0]);
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap() == 1 {
                head.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&head).into_owned();
            let path = head.split(' ').nth(1).unwrap_or_default();
            let (status, body) = (routes.iter().find(|r| r.0 == path))
                .map_or((404, &[][..]), |r| (200, r.1.as_slice()));
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(body);
        }
    });
    base
}

/// The project of `vets_the_lock` at acme/lib v1.2.0 from `lib_url`, with `config.toml` in supply-chain/ and
/// `secure-http` off so the test's server can serve it.
fn project(name: &str, lib_url: &str, config: &str) -> std::path::PathBuf {
    let d = common::scratch("vet", name);
    write_files(
        &d,
        &[
            (
                "composer.json",
                r#"{"require": {"acme/lib": "^1.0"}, "require-dev": {"acme/dev": "dev-main"}, "config": {"secure-http": false}}"#,
            ),
            ("composer.lock", &lock_with("v1.2.0", lib_url)),
            ("supply-chain/config.toml", config),
        ],
    );
    d
}

const EXEMPT: &str = "[[exemptions.\"acme/dev\"]]\nversion = \"dev-main#0123abcd\"\ncriteria = \"safe-to-run\"\n\n[[exemptions.\"acme/lib\"]]\nversion = \"1.2.0\"\ncriteria = \"safe-to-deploy\"\n";

/// supply-chain/ needs only the files that hold something; an import is read once from supply-chain/ or over HTTP,
/// and one that can't be read or isn't an audits.toml fails the check with exit 2.
#[test]
fn vet_imports_audits() {
    let d = project("imports", "https://example.invalid/lib.zip", EXEMPT);
    let (out, err, code) = vet(&d, &[]);
    assert_eq!(
        (out.as_str(), code),
        ("ok Vetted 2 packages: 0 audited, 2 exempted\n", Some(0)),
        "{err}"
    );

    let base = serve(|_| {
        vec![(
            "/audits.toml",
            b"[[audits.\"acme/lib\"]]\ncriteria = \"safe-to-deploy\"\nversion = \"1.2.0\"\n"
                .to_vec(),
        )]
    });
    let failing = [
        (
            "../missing.toml".to_string(),
            format!(
                "can't read the audits friends imports from {}",
                d.join("supply-chain/../missing.toml").display()
            ),
        ),
        (
            "../composer.json".to_string(),
            "../composer.json is not an audits.toml".to_string(),
        ),
        (
            format!("{base}/missing.toml"),
            format!("{base}/missing.toml has no audits to import"),
        ),
    ];
    for (url, error) in failing {
        let config = format!("{EXEMPT}\n[imports.friends]\nurl = \"{url}\"\n");
        std::fs::write(d.join("supply-chain/config.toml"), config).unwrap();
        let (_, err, code) = vet(&d, &[]);
        assert_eq!(code, Some(2), "{err}");
        assert!(err.contains(&error), "{err}");
        assert!(!d.join("supply-chain/imports.lock").exists());
    }

    let config = format!("{EXEMPT}\n[imports.friends]\nurl = \"{base}/audits.toml\"\n");
    std::fs::write(d.join("supply-chain/config.toml"), config).unwrap();
    let (out, err, code) = vet(&d, &[]);
    assert_eq!(
        (out.as_str(), code),
        ("ok Vetted 2 packages: 1 audited, 1 exempted\n", Some(0)),
        "{err}"
    );
    assert!(read(&d, "supply-chain/imports.lock").contains(&format!(
        "[imports.friends]\nurl = \"{base}/audits.toml\"\n\n[[imports.friends.audits.\"acme/lib\"]]\n"
    )));

    // Over plain HTTP only when `secure-http` is off, as for Composer's downloads.
    let manifest = read(&d, "composer.json").replace(", \"config\": {\"secure-http\": false}", "");
    std::fs::write(d.join("composer.json"), manifest).unwrap();
    std::fs::remove_file(d.join("supply-chain/imports.lock")).unwrap();
    let (_, err, code) = vet(&d, &[]);
    assert_eq!(code, Some(2), "{err}");
    assert!(
        err.contains(&format!(
            "fetching the audits friends imports from {base}/audits.toml"
        )),
        "{err}"
    );
}

/// `inspect` unpacks a locked version into Pun's cache once and prints the directory; `diff` compares two versions
/// with `git diff`, and exits 2 when git is missing or fails.
#[test]
fn vet_shows_the_code_to_review() {
    let base = serve(|_| vec![("/lib.tar", tarball("lib 1.2.0\n"))]);
    let d = project("review", &format!("{base}/lib.tar"), EXEMPT);
    let cache = d.join("cache");
    let env = [("PUN_CACHE_DIR", cache.to_str().unwrap())];
    let (out, err, code) = pun_with(&d, &["vet", "inspect", "acme/lib", "v1.2.0"], &env);
    assert_eq!(code, Some(0), "{err}");
    let unpacked = cache.join("vet/acme/lib/v1.2.0");
    assert_eq!(out, format!("{}\n", unpacked.display()));
    assert_eq!(read(&unpacked, "VERSION"), "lib 1.2.0\n");
    std::fs::write(
        d.join("composer.lock"),
        lock_with("v1.2.0", &format!("{base}/gone.tar")),
    )
    .unwrap();
    let (again, err, code) = pun_with(&d, &["vet", "inspect", "acme/lib", "v1.2.0"], &env);
    assert_eq!((again, code), (out, Some(0)), "{err}");

    std::fs::write(
        d.join("composer.lock"),
        lock_with("v1.2.0", &format!("{base}/lib.tar")),
    )
    .unwrap();
    let (out, err, code) = vet(&d, &["diff", "acme/lib", "v1.2.0", "1.2.0"]);
    assert_eq!((out.as_str(), code), ("", Some(0)), "{err}");

    let bin = d.join("bin");
    write_files(&d, &[("bin/git", "#!/bin/sh\nexit 128\n")]);
    let mut perms = std::fs::metadata(bin.join("git")).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perms, 0o755);
    std::fs::set_permissions(bin.join("git"), perms).unwrap();
    let diff = |path: &Path| {
        let args = ["vet", "diff", "acme/lib", "v1.2.0", "1.2.0"];
        pun_with(&d, &args, &[("PATH", path.to_str().unwrap())])
    };
    let (_, err, code) = diff(&d.join("nowhere"));
    assert_eq!(code, Some(2), "{err}");
    assert!(
        err.contains("can't run git, which pun vet diff shows the changes with"),
        "{err}"
    );
    let (_, err, code) = diff(&bin);
    assert_eq!(code, Some(2), "{err}");
}

/// A tar archive whose one top directory holds `VERSION` with `text`, as GitHub's archives have one.
fn tarball(text: &str) -> Vec<u8> {
    let mut tar = tar::Builder::new(vec![]);
    let mut header = tar::Header::new_gnu();
    header.set_size(text.len() as u64);
    header.set_mode(0o644);
    header.set_cksum();
    tar.append_data(&mut header, "acme-lib-aaa/VERSION", text.as_bytes())
        .unwrap();
    tar.into_inner().unwrap()
}
