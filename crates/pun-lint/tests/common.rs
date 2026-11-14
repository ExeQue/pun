use std::path::{Path, PathBuf};

use pun_format::Pipeline;
use pun_lint::rules::facts::with_sources;
use pun_lint::{Linter, Rule};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn fixtures(dir: &Path) -> Vec<(PathBuf, String, String)> {
    let mut paths = Vec::new();
    files(dir, &mut paths);
    paths.sort();
    assert!(!paths.is_empty(), "no fixtures in {}", dir.display());
    paths
        .into_iter()
        .map(|path| {
            let content = std::fs::read_to_string(&path)
                .unwrap()
                .replace("\r\n", "\n");
            let (input, expected) = match content.split_once("-----\n") {
                Some((i, e)) => (i.to_string(), e.to_string()),
                None => (content.clone(), content),
            };
            (path, input, expected)
        })
        .collect()
}

fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.map(|e| e.unwrap().path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "php") {
            out.push((
                path.to_string_lossy().into_owned(),
                std::fs::read_to_string(&path).unwrap(),
            ));
        }
    }
}

fn format(pipeline: &Pipeline, code: &str) -> String {
    match pipeline
        .run(Path::new("fixture.php"), code.as_bytes())
        .unwrap()
    {
        Some(fixed) => String::from_utf8(fixed.code).unwrap(),
        None => code.to_string(),
    }
}

fn laravel() -> Pipeline {
    let rules = pun_config::rule_set(pun_config::preset_rules("laravel").unwrap()).unwrap();
    let pipeline = Pipeline::new(&rules, &pun_fixers::all).unwrap();
    assert!(pipeline.not_ported.is_empty(), "{:?}", pipeline.not_ported);
    pipeline
}

pub(crate) fn run_class(rules: Vec<Box<dyn Rule>>, class: &str) {
    run_class_at(rules, class, 80400);
}

pub(crate) fn run_class_at(rules: Vec<Box<dyn Rule>>, class: &str, php_version: u32) {
    run_class_full(rules, class, php_version, &[], &[], true);
}

pub(crate) fn run_class_with(
    rules: Vec<Box<dyn Rule>>,
    class: &str,
    php_version: u32,
    preset_too: &[&str],
) {
    run_class_full(rules, class, php_version, &[], preset_too, true);
}

pub(crate) fn run_class_stubs(
    rules: Vec<Box<dyn Rule>>,
    class: &str,
    php_version: u32,
    stubs: &[&str],
) {
    run_class_full(rules, class, php_version, stubs, &[], true);
}

pub(crate) fn run_class_paths(
    rules: Vec<Box<dyn Rule>>,
    class: &str,
    php_version: u32,
    stubs: &[&Path],
) {
    check_dirs(class, true);
    run_dirs(
        rules,
        class,
        &["Fixture", "Oracle", "Pun"],
        php_version,
        stubs,
    );
}

pub(crate) fn run_oracle_class(rules: Vec<Box<dyn Rule>>, class: &str, php_version: u32) {
    run_class_full(rules, class, php_version, &[], &[], false);
}

fn run_class_full(
    rules: Vec<Box<dyn Rule>>,
    class: &str,
    php_version: u32,
    stubs: &[&str],
    preset_too: &[&str],
    fixture: bool,
) {
    check_dirs(class, fixture);
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let stubs: Vec<PathBuf> = stubs.iter().map(|d| tests.join(d)).collect();
    let stubs: Vec<&Path> = stubs.iter().map(PathBuf::as_path).collect();
    run_dirs_full(
        rules,
        class,
        &["Fixture", "Oracle", "Pun"],
        php_version,
        &stubs,
        preset_too,
    );
}

fn check_dirs(class: &str, fixture: bool) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rector")
        .join(class);
    assert!(
        root.join("Fixture").is_dir() == fixture || (fixture && root.join("Oracle").is_dir()),
        "{class}: a Fixture directory where upstream has fixtures, and only there"
    );
    assert!(
        root.join("Oracle").is_dir() || fixture,
        "{class}: no Oracle directory"
    );
}

pub(crate) fn run_dirs(
    rules: Vec<Box<dyn Rule>>,
    class: &str,
    subs: &[&str],
    php_version: u32,
    stubs: &[&Path],
) {
    run_dirs_full(rules, class, subs, php_version, stubs, &[]);
}

fn run_dirs_full(
    rules: Vec<Box<dyn Rule>>,
    class: &str,
    subs: &[&str],
    php_version: u32,
    stubs: &[&Path],
    preset_too: &[&str],
) {
    let linter = Linter::new(rules, php_version);
    let pipeline = laravel();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/rector")
        .join(class);
    for &sub in subs {
        let dir = root.join(sub);
        if !dir.is_dir() {
            continue;
        }
        for (path, input, expected) in fixtures(&dir) {
            let name = format!("{sub}/{}", path.file_name().unwrap().to_string_lossy());
            let mut extra = Vec::new();
            for dir in stubs {
                sources(dir, &mut extra);
            }
            sources(&root.join("Source"), &mut extra);
            sources(&root.join("Stubs"), &mut extra);
            let fixed = with_sources(extra, || linter.fix(&path, &input)).unwrap();
            let expected = match fixed.source.strip_suffix('\n') {
                Some(without) if without == expected => fixed.source.clone(),
                _ => expected,
            };
            assert_eq!(fixed.source, expected, "{class} {name}: lint --fix");
            assert_eq!(
                format(&pipeline, &fixed.source),
                format(&pipeline, &expected),
                "{class} {name}: format(lint --fix)"
            );
            if input != expected {
                assert!(
                    preset_too.contains(&name.as_str())
                        || FORMAT_OVERLAP.contains(&format!("{class}/{name}").as_str())
                        || format(&pipeline, &input) != format(&pipeline, &expected),
                    "{class} {name}"
                );
                assert!(!fixed.applied.is_empty(), "{class} {name}");
            } else {
                assert!(fixed.applied.is_empty(), "{class} {name}");
            }
        }
    }
}

const FORMAT_OVERLAP: &[&str] = &[
    "RenameClassRector/FixtureAutoImportNames/auto_import_conflict_name_all_fqcn_same_namespace.php.inc",
    "RenameClassRector/OracleAutoImportNames/doc_only_comment.php.inc",
    "RenameClassRector/OracleAutoImportNames/doc_on_only_use.php.inc",
    "RenameClassRector/OracleAutoImportNames/doc_on_use.php.inc",
    "RenameClassRector/OracleAutoImportNames/doc_on_used_use.php.inc",
    "RenameClassRector/OracleAutoImportNames/doc_on_second_namespace.php.inc",
    "RenameClassRector/Oracle/namespaces_f2.php.inc",
    "RenameClassRector/Oracle/namespaces_f3.php.inc",
    "RenameClassRector/Oracle/namespaces_f5.php.inc",
    "FixClassCaseSensitivityVarDocblockRector/Fixture/inline_var.php.inc",
    "FixClassCaseSensitivityVarDocblockRector/Fixture/property_var.php.inc",
];
