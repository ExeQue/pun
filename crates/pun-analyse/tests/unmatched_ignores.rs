//! Unmatched ignoreErrors entries when only some files of the configured paths are analysed: PHPStan 2.3.6's
//! UnmatchedIgnoredErrorJudge reports an analysed file's entries only when nothing reported in the file depends on a
//! file left out (a used trait, the declaration of a possibly pure callee and what that one calls, the whole project
//! for a named argument a subtype may rename). The expectations are 2.3.6's output on
//! tests/fixtures/unmatched-ignores (`phpstan analyse <files>`); the oracle, 2.3.5, reports none of these entries.

use std::path::Path;

use pun_analyse::{Project, run};
use pun_phpstan::Array;
use pun_phpstan::config::{Options, resolve};
use pun_phpstan::ignore::IgnoredErrors;

fn analyse(files: &[&str]) -> Vec<(String, i64, String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/unmatched-ignores");
    let dir = root.to_string_lossy().into_owned();
    let config = resolve(&Options {
        config: Some(format!("{dir}/phpstan.neon")),
        ..Options::for_project(&dir)
    })
    .unwrap();
    let mut project = Project::from_config(&root, &config.parameters);
    let files: Vec<String> = files.iter().map(|f| format!("{dir}/{f}")).collect();
    // As `pun analyse <files>` does: the configured paths' other files are scanned.
    let configured = std::mem::take(&mut project.analysed);
    project.analysed = files.iter().map(Into::into).collect();
    project.scanned.extend(
        configured
            .into_iter()
            .filter(|f| !project.analysed.contains(f)),
    );
    project.only_files = true;
    let ignores = std::mem::replace(
        &mut project.ignore_errors,
        IgnoredErrors::new(&Array::new(), true),
    );
    project.ignore_errors = ignores.with_analysed_paths(
        &files,
        &[format!("{dir}/src")],
        Some(std::sync::Arc::new(|_: &str| false)),
    );
    let prefix = format!("{dir}/");
    let mut got: Vec<_> = (run(&project).unwrap().into_iter())
        .map(|e| {
            let file = e.file.strip_prefix(&prefix).unwrap_or(&e.file).to_string();
            (file, e.line, e.identifier, e.message.replace(&prefix, ""))
        })
        .collect();
    got.sort();
    got
}

fn unmatched(file: &str) -> (String, i64, String, String) {
    (
        file.to_string(),
        0,
        "ignore.unmatched".to_string(),
        format!(
            "Ignored error pattern #^Never$# in path {file} was not matched in reported errors."
        ),
    )
}

#[test]
fn entries_of_entirely_analysed_files() {
    // A trait outside the configured paths does not count; one inside them, a possibly pure callee and a named
    // argument do.
    let got = analyse(&[
        "src/plain.php",
        "src/uses_lib_trait.php",
        "src/uses_src_trait.php",
        "src/calls_pure.php",
        "src/named.php",
    ]);
    let not_found = (
        "src/uses_src_trait.php".to_string(),
        9,
        "trait.notFound".to_string(),
        "Class App\\UsesSrc uses unknown trait App\\Missing.".to_string(),
    );
    assert_eq!(
        got,
        [
            unmatched("src/plain.php"),
            unmatched("src/uses_lib_trait.php"),
            not_found
        ]
    );
}

#[test]
fn declarations_of_callees() {
    // What pure() calls is collected about its declaration: it decides calls_pure.php but not pure.php.
    let got = analyse(&["src/calls_pure.php", "src/pure.php"]);
    assert_eq!(got, [unmatched("src/pure.php")]);
    let got = analyse(&["src/calls_pure.php", "src/pure.php", "src/other.php"]);
    let unused = (
        "src/calls_pure.php".to_string(),
        7,
        "function.resultUnused".to_string(),
        "Call to function App\\pure() on a separate line has no effect.".to_string(),
    );
    assert_eq!(
        got,
        [
            unmatched("src/calls_pure.php"),
            unused,
            unmatched("src/pure.php")
        ]
    );
}
