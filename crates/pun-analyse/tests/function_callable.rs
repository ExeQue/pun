use std::path::Path;

use pun_analyse::{Project, run};
use pun_phpstan::config::{Options, resolve};

type Diagnostic = (String, i64, String, String, Option<String>, bool);

#[test]
fn first_class_function_callables_match_phpstan() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/function-callable");
    let expected: std::collections::BTreeMap<String, Vec<Diagnostic>> =
        serde_json::from_slice(&std::fs::read(root.join("expected.json")).unwrap()).unwrap();
    for (case, mut want) in expected {
        let config = resolve(&Options {
            config: Some(
                root.join(format!("{case}.neon"))
                    .to_string_lossy()
                    .into_owned(),
            ),
            ..Options::for_project(root.to_str().unwrap())
        })
        .unwrap();
        let project = Project::from_config(&root, &config.parameters);
        let mut got: Vec<_> = run(&project)
            .unwrap()
            .into_iter()
            .filter(|e| {
                matches!(
                    e.identifier.as_str(),
                    "callable.nonCallable"
                        | "callable.notSupported"
                        | "function.notFound"
                        | "function.nameCase"
                ) || e.identifier == "class.notFound" && e.message.starts_with("Creating callable")
            })
            .map(|e| {
                (
                    Path::new(&e.file)
                        .file_name()
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    e.line,
                    e.identifier,
                    e.message,
                    e.tip,
                    e.can_be_ignored,
                )
            })
            .collect();
        got.sort();
        want.sort();
        assert_eq!(got, want, "{case}");
    }
}

#[test]
fn partial_applications_are_not_first_class_callables() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/function-callable");
    let mut project = Project::new(vec![root.join("partial.php")], Vec::new());
    project.php_version = pun_analyse::PhpVersion::from_id(80500);
    project.level = 7;
    let errors: Vec<_> = run(&project)
        .unwrap()
        .into_iter()
        .filter(|e| e.identifier.starts_with("callable."))
        .collect();
    assert_eq!(errors, []);
}
