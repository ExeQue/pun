//! Larastan 3.10.0's own rule tests, run through PHPStan 2.3.1 with Larastan by
//! scripts/export-larastan-fixtures.php: every case's errors with an identifier of a ported Larastan rule must be
//! exactly PHPStan's. Pun knows Laravel through tests/fixtures/larastan/stubs.php. Cases that still differ are
//! listed in tests/fixtures/larastan/gaps.txt, and the list must be exact (a fixed case has to leave it). The app root
//! is tests/fixtures/larastan/application, with the migrations of Larastan's test application.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use pun_analyse::{Larastan, Project, run};
use serde_json::Value;

type Key = (String, i64, String, String, Option<String>);

const PORTED: &[&str] = &[
    "larastan.uselessConstructs.with",
    "larastan.uselessConstructs.value",
    "larastan.noEnvCallsOutsideOfConfig",
    "larastan.noModelMake",
    "larastan.deferrableServiceProvider.missingProvides",
    "larastan.noAuthFacadeInRequestScope",
    "larastan.noAuthHelperInRequestScope",
    "larastan.noPublicModelScopeMethod",
    "larastan.noPublicModelAccessorMethod",
    "larastan.octaneCompatibility",
    "rules.modelAppends",
    "larastan.jobs.noConstructor",
    "larastan.events.noConstructor",
    "larastan.console.undefinedArgument",
    "larastan.console.undefinedOption",
    "larastan.configCollection",
    "larastan.unnecessaryEnumerableToArrayCall",
    "larastan.relationExistence",
    "larastan.noUnnecessaryCollectionCall",
    "larastan.unusedViews",
    "larastan.missingTranslations",
];

fn is_compared(identifier: &str, message: &str) -> bool {
    PORTED.contains(&identifier)
        || identifier == "arguments.count"
            && (message.starts_with("Job class ") || message.starts_with("Event class "))
}

fn scanned(root: &Path) -> Vec<PathBuf> {
    vec![
        root.join("stubs.php"),
        root.join("application/database/migrations/Schema.php"),
    ]
}

fn fixtures(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() && p.file_name().unwrap() != "data" {
            fixtures(&p, out);
        } else if p.extension().is_some_and(|x| x == "json") {
            out.push(p);
        }
    }
}

#[test]
fn larastan_rule_fixtures() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/larastan");
    let data = root.join("data");
    let mut files = Vec::new();
    fixtures(&root, &mut files);
    files.sort();
    let gaps: BTreeSet<String> = std::fs::read_to_string(root.join("gaps.txt"))
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
        .collect();
    let prefix = format!("{}/", data.display());
    let mut larastan = Larastan::new(&root.join("application"));
    larastan.config_directories = ["Rules/data/config", "Rules/data/module/*/config"]
        .iter()
        .map(|d| data.join(d).to_string_lossy().into_owned())
        .collect();
    larastan.switches.set(
        pun_analyse::LarastanSwitches::CHECK_OCTANE_COMPATIBILITY,
        true,
    );
    larastan.switches.set(
        pun_analyse::LarastanSwitches::CHECK_MODEL_METHOD_VISIBILITY,
        true,
    );
    larastan.switches.set(
        pun_analyse::LarastanSwitches::CHECK_AUTH_CALLS_WHEN_IN_REQUEST_SCOPE,
        true,
    );
    larastan
        .switches
        .set(pun_analyse::LarastanSwitches::CHECK_CONFIG_TYPES, true);
    larastan.switches.set(
        pun_analyse::LarastanSwitches::NO_UNNECESSARY_ENUMERABLE_TO_ARRAY_CALLS,
        true,
    );
    let mut failing = BTreeSet::new();
    let mut details = Vec::new();
    let mut compared = 0;
    for f in files {
        let test: Value = serde_json::from_str(&std::fs::read_to_string(&f).unwrap()).unwrap();
        for case in test["cases"].as_array().unwrap() {
            let want: BTreeSet<Key> = case["errors"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| {
                    e["identifier"]
                        .as_str()
                        .is_some_and(|id| is_compared(id, e["message"].as_str().unwrap()))
                })
                .map(|e| {
                    (
                        e["file"].as_str().unwrap().to_string(),
                        e["line"].as_i64().unwrap(),
                        e["identifier"].as_str().unwrap().to_string(),
                        e["message"].as_str().unwrap().to_string(),
                        e["tip"].as_str().map(str::to_string),
                    )
                })
                .collect();
            let analysed = case["files"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| data.join(p.as_str().unwrap()))
                .collect();
            let mut scanned = scanned(&root);
            scanned.extend(
                case["scanned"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|p| data.join(p.as_str().unwrap())),
            );
            let mut project = Project::new(analysed, scanned);
            project.larastan = Some(larastan.clone());
            let got: BTreeSet<Key> = run(&project)
                .unwrap()
                .into_iter()
                .filter(|e| is_compared(&e.identifier, &e.message))
                .map(|e| {
                    let file = e.file.strip_prefix(&prefix).unwrap_or(&e.file).to_string();
                    (file, e.line, e.identifier, e.message, e.tip)
                })
                .collect();
            let name = format!(
                "{}::{}",
                test["test"].as_str().unwrap(),
                case["method"].as_str().unwrap()
            );
            compared += want.len();
            if want != got {
                details.push(format!(
                    "{name}\n  missing {:#?}\n  extra   {:#?}",
                    want.difference(&got).collect::<Vec<_>>(),
                    got.difference(&want).collect::<Vec<_>>()
                ));
                failing.insert(name);
            }
        }
    }
    assert!(compared >= 30, "only {compared} expected errors");
    let fixed: Vec<_> = gaps.difference(&failing).collect();
    let new: Vec<_> = details
        .iter()
        .filter(|d| !gaps.contains(d.lines().next().unwrap()))
        .collect();
    assert!(
        fixed.is_empty() && new.is_empty(),
        "fixed (remove from gaps.txt): {fixed:#?}\nnew failures:\n{}",
        new.iter()
            .map(|d| d.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn larastan_switches_gate_the_rules() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/larastan");
    let data = root.join("data");
    let test: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("Pun/Cases.json")).unwrap())
            .unwrap();
    const OPT_IN: &[&str] = &[
        "larastan.noAuthFacadeInRequestScope",
        "larastan.noAuthHelperInRequestScope",
        "larastan.noPublicModelScopeMethod",
        "larastan.noPublicModelAccessorMethod",
        "larastan.octaneCompatibility",
    ];
    for case in test["cases"].as_array().unwrap() {
        let want: BTreeSet<(i64, String)> = case["errors"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|e| Some((e["line"].as_i64()?, e["identifier"].as_str()?.to_string())))
            .filter(|(_, id)| PORTED.contains(&id.as_str()) && !OPT_IN.contains(&id.as_str()))
            .collect();
        let files: Vec<PathBuf> = case["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| data.join(p.as_str().unwrap()))
            .collect();
        let got = |larastan: Option<Larastan>| {
            let mut project = Project::new(files.clone(), scanned(&root));
            project.larastan = larastan;
            run(&project)
                .unwrap()
                .into_iter()
                .filter(|e| PORTED.contains(&e.identifier.as_str()))
                .map(|e| (e.line, e.identifier))
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(
            got(Some(Larastan::new(&root.join("application")))),
            want,
            "{}",
            case["method"]
        );
        assert!(got(None).is_empty(), "{}", case["method"]);
    }
}

#[test]
fn larastan_switches_from_the_config() {
    let root = Path::new("/app");
    let switches = |neon: &str| {
        let config = pun_phpstan::neon::decode(neon).unwrap();
        let params = config.as_array().unwrap().get_str("parameters").unwrap();
        Larastan::from_params(root, params.as_array().unwrap())
    };
    assert!(switches("parameters:\n\tlevel: 0\n").is_none());
    let l = switches(
        "parameters:\n\tnoModelMake: false\n\tcheckOctaneCompatibility: true\n\tconfigDirectories:\n\t\t- modules/*/config\n",
    )
    .unwrap();
    assert!(
        !l.switches
            .contains(pun_analyse::LarastanSwitches::NO_MODEL_MAKE)
    );
    assert!(
        l.switches
            .contains(pun_analyse::LarastanSwitches::CHECK_OCTANE_COMPATIBILITY)
    );
    assert!(
        l.switches
            .contains(pun_analyse::LarastanSwitches::NO_ENV_CALLS_OUTSIDE_OF_CONFIG)
    );
    assert!(
        !l.switches
            .contains(pun_analyse::LarastanSwitches::CHECK_MODEL_METHOD_VISIBILITY)
    );
    assert_eq!(l.config_directories, ["/app/modules/*/config"]);
    let defaults = switches("parameters:\n\tnoModelMake: true\n").unwrap();
    assert_eq!(defaults.config_directories, ["/app/config"]);
}

fn oracle_case(dir: &Path, larastan: Larastan, prefix: &str) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/larastan");
    let expected: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();
    let want: BTreeSet<(String, i64, String, String)> = expected["errors"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["file"].as_str().unwrap().to_string(),
                e["line"].as_i64().unwrap(),
                e["identifier"].as_str().unwrap().to_string(),
                e["message"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "php"))
        .collect();
    files.sort();
    let mut project = Project::new(files, scanned(&root));
    project.larastan = Some(larastan);
    let got: BTreeSet<_> = run(&project)
        .unwrap()
        .into_iter()
        .filter(|e| e.identifier.starts_with(prefix))
        .map(|e| {
            let file = Path::new(&e.file)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            (file, e.line, e.identifier, e.message)
        })
        .collect();
    assert_eq!(got, want);
}

#[test]
fn larastan_console_commands() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/larastan");
    oracle_case(
        &root.join("data/Console"),
        Larastan::new(&root.join("application")),
        "larastan.console.",
    );
    oracle_case(
        &root.join("data/ConsoleEdge"),
        Larastan::new(&root.join("application")),
        "larastan.console.",
    );
    let dir = root.join("data/ConsoleBroken");
    let mut project = Project::new(
        vec![
            dir.join("DashShortcutCommand.php"),
            dir.join("EmptyOptionCommand.php"),
            dir.join("NamelessCommand.php"),
        ],
        scanned(&root),
    );
    project.larastan = Some(Larastan::new(&root.join("application")));
    assert!(
        run(&project)
            .unwrap()
            .iter()
            .all(|e| !e.identifier.starts_with("larastan.console."))
    );
}

#[test]
fn larastan_config_collection() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/larastan/data/Config");
    let mut larastan = Larastan::new(&dir);
    larastan
        .switches
        .set(pun_analyse::LarastanSwitches::CHECK_CONFIG_TYPES, true);
    oracle_case(&dir, larastan.clone(), "larastan.configCollection");
    larastan.config_directories = vec![dir.join("missing").to_string_lossy().into_owned()];
    oracle_case(&dir, larastan.clone(), "larastan.configCollection");
    let extra = dir.with_file_name("ConfigExtra");
    let mut configured = Larastan::new(&dir);
    configured
        .switches
        .set(pun_analyse::LarastanSwitches::CHECK_CONFIG_TYPES, true);
    configured.config_directories = vec![extra.join("config").to_string_lossy().into_owned()];
    oracle_case(&extra, configured, "larastan.configCollection");
    let mut project = Project::new(vec![dir.join("ConfigCases.php")], Vec::new());
    project.larastan = Some(Larastan::new(&dir));
    assert!(
        run(&project)
            .unwrap()
            .iter()
            .all(|e| e.identifier != "larastan.configCollection")
    );
}

#[test]
fn larastan_relation_existence() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/larastan");
    oracle_case(
        &root.join("data/Relations"),
        Larastan::new(&root.join("application")),
        "larastan.relationExistence",
    );
}

#[test]
fn larastan_unnecessary_collection_calls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/larastan");
    let dir = root.join("data/Collections");
    let mut larastan = Larastan::new(&root.join("application"));
    larastan.switches.set(
        pun_analyse::LarastanSwitches::NO_UNNECESSARY_ENUMERABLE_TO_ARRAY_CALLS,
        true,
    );
    oracle_case(&dir, larastan.clone(), "larastan.");
    let config = pun_phpstan::neon::decode(
        "parameters:\n\tnoModelMake: true\n\tnoUnnecessaryCollectionCallOnly: [Count, take]\n\tnoUnnecessaryCollectionCallExcept: [take]\n",
    )
    .unwrap();
    let params = config.as_array().unwrap().get_str("parameters").unwrap();
    let mut only =
        Larastan::from_params(&root.join("application"), params.as_array().unwrap()).unwrap();
    let mut project = Project::new(
        vec![dir.join("Calls.php"), dir.join("Writer.php")],
        scanned(&root),
    );
    project.larastan = Some(only.clone());
    let lines: BTreeSet<i64> = run(&project)
        .unwrap()
        .into_iter()
        .filter(|e| e.identifier == "larastan.noUnnecessaryCollectionCall")
        .map(|e| e.line)
        .collect();
    assert_eq!(lines, BTreeSet::from([14, 20, 33, 34]));
    only.switches.set(
        pun_analyse::LarastanSwitches::NO_UNNECESSARY_COLLECTION_CALL,
        false,
    );
    project.larastan = Some(only);
    assert!(
        run(&project)
            .unwrap()
            .iter()
            .all(|e| e.identifier != "larastan.noUnnecessaryCollectionCall")
    );
}

#[test]
fn larastan_views_and_translations() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/larastan");
    let dir = root.join("data/Views");
    let mut larastan = Larastan::new(&dir);
    larastan
        .switches
        .set(pun_analyse::LarastanSwitches::CHECK_UNUSED_VIEWS, true);
    larastan.switches.set(
        pun_analyse::LarastanSwitches::CHECK_MISSING_TRANSLATIONS,
        true,
    );
    oracle_case(&dir, larastan.clone(), "larastan.");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "php"))
        .collect();
    files.sort();
    let mut selected = Project::new(files, scanned(&root));
    selected.larastan = Some(larastan.clone());
    selected.reported = Some(std::sync::Arc::new(|id| id == "larastan.unusedViews"));
    let unused: Vec<String> = run(&selected)
        .unwrap()
        .into_iter()
        .map(|e| format!("{}: {}", e.file.rsplit('/').next().unwrap(), e.identifier))
        .collect();
    assert_eq!(unused, ["unused.blade.php: larastan.unusedViews"]);
    let edge = root.join("data/ViewsEdge");
    let mut views = Larastan::new(&edge);
    views
        .switches
        .set(pun_analyse::LarastanSwitches::CHECK_UNUSED_VIEWS, true);
    views.switches.set(
        pun_analyse::LarastanSwitches::CHECK_MISSING_TRANSLATIONS,
        true,
    );
    oracle_case(&edge, views, "larastan.");
    larastan.view_directories = vec![dir.join("resources/views/emails")];
    larastan.translation_directories = vec![dir.join("missing")];
    let mut project = Project::new(vec![dir.join("Translation.php")], scanned(&root));
    project.larastan = Some(larastan.clone());
    let errors = run(&project).unwrap();
    assert!(
        errors
            .iter()
            .all(|e| e.identifier != "larastan.unusedViews")
    );
    assert!(
        errors
            .iter()
            .any(|e| e.message == "Translation \"messages.foo\" has not been found.")
    );
    project.larastan = Some(Larastan::new(&dir));
    assert!(
        run(&project)
            .unwrap()
            .iter()
            .all(|e| !e.identifier.starts_with("larastan."))
    );
}
