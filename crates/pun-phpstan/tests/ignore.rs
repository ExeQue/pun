//! `@phpstan-ignore*` comment parsing and `ignoreErrors` matching against PHPStan 2.3.5's own classes
//! (tests/fixtures/comments.json and ignore.json, exported by scripts/export-ignore-fixtures.php).

use crate::common::{fixture, placeholders, tagged, untagged};
use pun_phpstan::comments::{self, LinesToIgnore};
use pun_phpstan::ignore::{Error, IgnoredErrors};
use pun_phpstan::{Array, Key, Value};
use serde_json::{Value as J, json};

fn lines_value(lines: &LinesToIgnore) -> Value {
    let mut a = Array::new();
    for (&line, ids) in lines {
        let v = match ids {
            None => Value::Null,
            Some(ids) => Value::Array(Array::list(ids.iter().map(|i| {
                let mut e = Array::new();
                e.set("name".into(), Value::str(&i.name));
                e.set(
                    "comment".into(),
                    i.comment.as_deref().map_or(Value::Null, Value::str),
                );
                Value::Array(e)
            }))),
        };
        a.set(Key::Int(line), v);
    }
    Value::Array(a)
}

#[test]
fn rich_parser_lines_to_ignore() {
    let mut failures = Vec::new();
    for case in fixture("comments.json").as_array().unwrap() {
        let (lines, errors) = comments::lines_to_ignore(case["code"].as_str().unwrap().as_bytes());
        let mut e = Array::new();
        for (&line, messages) in &errors {
            e.set(
                Key::Int(line),
                Value::Array(Array::list(messages.iter().map(Value::str))),
            );
        }
        let got =
            json!({ "lines": tagged(&lines_value(&lines)), "errors": tagged(&Value::Array(e)) });
        let want = json!({ "lines": case["lines"], "errors": case["errors"] });
        if got != want {
            failures.push(format!("{:?}\n  want {want}\n  got  {got}", case["code"]));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn opt(j: &J) -> Option<String> {
    j.as_str().map(String::from)
}

fn error(j: &J) -> Error {
    Error {
        message: j["message"].as_str().unwrap().into(),
        file: j["file"].as_str().unwrap().into(),
        line: j["line"].as_i64(),
        can_be_ignored: j["canBeIgnored"].as_bool().unwrap(),
        file_path: j["filePath"].as_str().unwrap().into(),
        trait_file_path: opt(&j["traitFilePath"]),
        tip: opt(&j["tip"]),
        identifier: opt(&j["identifier"]),
        fixable: false,
    }
}

fn dump(e: &Error) -> J {
    json!({
        "message": e.message, "file": e.file, "line": e.line, "canBeIgnored": e.can_be_ignored,
        "filePath": e.file_path, "traitFilePath": e.trait_file_path, "tip": e.tip, "identifier": e.identifier,
    })
}

#[test]
fn ignored_error_helper() {
    let dir = format!("{}/tests/fixtures/ignore/files", env!("CARGO_MANIFEST_DIR"));
    let to_real = [("{dir}", dir.as_str())];
    let to_placeholder = [(dir.as_str(), "{dir}")];
    let mut failures = Vec::new();
    for case in fixture("ignore.json").as_array().unwrap() {
        let case = placeholders(case.clone(), &to_real);
        let Value::Array(entries) = untagged(&case["ignoreErrors"]) else {
            panic!()
        };
        let helper = IgnoredErrors::new(&entries, case["reportUnmatched"].as_bool().unwrap());
        let analysed: Vec<String> = case["analysedFiles"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f.as_str().unwrap().into())
            .collect();
        let result = helper.process(
            case["errors"]
                .as_array()
                .unwrap()
                .iter()
                .map(error)
                .collect(),
            case["onlyFiles"].as_bool().unwrap(),
            &analysed,
            case["hasInternalErrors"].as_bool().unwrap(),
            false,
            None,
        );
        let got = json!({
            "configErrors": helper.errors,
            "errors": result.errors.iter().map(dump).collect::<Vec<_>>(),
            "ignored": result.ignored.iter().map(|(e, i)| json!([dump(e), tagged(i)])).collect::<Vec<_>>(),
            "stringErrors": result.string_errors,
        });
        let got = placeholders(got, &to_placeholder);
        let mut want = placeholders(case["expected"].clone(), &to_placeholder);
        // The fixture is PHPStan 2.3.5's (the oracle). Pun follows 2.3.6 here: its UnmatchedIgnoredErrorJudge
        // reports `ignore.count` of an entry in an analysis of only files only when the entry's file is analysed,
        // and this case analyses none; 2.3.6 reports no error (scripts/export-ignore-fixtures.php on its phar).
        if case["onlyFiles"] == true
            && case["analysedFiles"] == json!([])
            && want["errors"][0]["identifier"] == "ignore.count"
        {
            want["errors"] = json!([]);
        }
        if got != want {
            failures.push(format!(
                "{}\n  want {want}\n  got  {got}",
                case["ignoreErrors"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The comment pipeline of `FileAnalyser` + `AnalyserResultFinalizer` against a real `phpstan analyse` run
/// (tests/fixtures/local.json, scripts/export-local-ignore-fixtures.php): the raw errors come from the same files with
/// the comments blanked out.
#[test]
fn phpstan_analyse_local_ignores() {
    let dir = format!("{}/tests/fixtures/local", env!("CARGO_MANIFEST_DIR"));
    let key = |j: &J| {
        let s = |k: &str| j.get(k).map_or_default(J::to_string);
        (
            j["line"].as_i64(),
            j["message"].as_str().unwrap().to_string(),
            s("ignorable"),
            s("identifier"),
            s("tip"),
            s("fixable"),
        )
    };
    let mut failures = Vec::new();
    let all = fixture("local.json");
    for (name, data) in all.as_object().unwrap() {
        let file = format!("{dir}/{name}");
        let code = std::fs::read(&file).unwrap();
        for (without_comments, reported) in [(false, "reported"), (true, "reportedWithoutComments")]
        {
            let (mut lines, parse) = comments::lines_to_ignore(&code);
            let mut unmatched = lines.clone();
            let mut errors = pun_phpstan::ignore::parse_errors(&file, &parse);
            let mut raw: Vec<Error> = data["raw"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| Error {
                    can_be_ignored: m["ignorable"].as_bool().unwrap(),
                    tip: opt(&m["tip"]),
                    identifier: opt(&m["identifier"]),
                    ..Error::new(m["message"].as_str().unwrap(), &file, m["line"].as_i64())
                })
                .collect();
            raw.append(&mut errors);
            let mut out = Vec::new();
            if without_comments {
                out.extend(pun_phpstan::ignore::ignores_without_comments(
                    &file,
                    &mut lines,
                    &mut unmatched,
                ));
            }
            let (kept, _) =
                pun_phpstan::ignore::local_ignores(&file, raw, &mut lines, &mut unmatched);
            out.extend(kept);
            pun_phpstan::ignore::fix_all_line_errors(&file, Some(&code), &mut out);
            out.extend(pun_phpstan::ignore::unmatched_line_errors(
                &file,
                Some(&code),
                &unmatched,
            ));
            let mut got: Vec<_> = out
                .iter()
                .map(|e| {
                    let mut j = json!({ "line": e.line, "message": e.message, "ignorable": e.can_be_ignored });
                    if let Some(id) = &e.identifier {
                        j["identifier"] = json!(id);
                    }
                    if let Some(tip) = &e.tip {
                        j["tip"] = json!(tip);
                    }
                    if e.fixable {
                        j["fixable"] = json!(true);
                    }
                    key(&j)
                })
                .collect();
            let mut want: Vec<_> = data[reported].as_array().unwrap().iter().map(key).collect();
            got.sort();
            want.sort();
            if got != want {
                failures.push(format!(
                    "{name} {reported}\n  want {want:?}\n  got  {got:?}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn local_ignores_processor() {
    for case in fixture("local-processor.json").as_array().unwrap() {
        let (mut lines, _) = comments::lines_to_ignore(case["code"].as_str().unwrap().as_bytes());
        let mut unmatched = if case["noUnmatched"] == true {
            LinesToIgnore::new()
        } else {
            lines.clone()
        };
        let errors = case["errors"]
            .as_array()
            .unwrap()
            .iter()
            .map(error)
            .collect();
        let (kept, ignored) =
            pun_phpstan::ignore::local_ignores("f.php", errors, &mut lines, &mut unmatched);
        let got = json!({
            "kept": kept.iter().map(dump).collect::<Vec<_>>(),
            "ignored": ignored.iter().map(dump).collect::<Vec<_>>(),
            "lines": tagged(&lines_value(&lines)),
            "unmatched": tagged(&lines_value(&unmatched)),
        });
        let want = json!({ "kept": case["kept"], "ignored": case["ignored"], "lines": case["lines"], "unmatched": case["unmatched"] });
        assert_eq!(got, want, "{}", case["code"]);
    }
}

#[test]
fn rule_entries() {
    fn rule_of(id: &str) -> Option<&'static str> {
        match id {
            "a.one" | "a.two" => Some("analyse/x/a"),
            _ => None,
        }
    }
    let file = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/ignore.rs");
    let entry = |pairs: &[(&str, Value)]| {
        let mut e = Array::new();
        for (k, v) in pairs {
            e.set(Key::from(*k), v.clone());
        }
        Value::Array(e)
    };
    let rule = |r: &str| ("rule", Value::str(r));
    let at = ("path", Value::str(file));
    let entries = Array::list([
        entry(&[rule("analyse/x/a"), at.clone()]),
        entry(&[rule("analyse/x/a"), at.clone()]),
        entry(&[rule("analyse/x/b")]),
    ]);
    let ignored = IgnoredErrors::new(&entries, true).with_rules(rule_of);
    let e = |id: &str| Error {
        identifier: Some(id.into()),
        ..Error::new("m", file, Some(1))
    };
    let p = ignored.process(
        vec![e("a.one"), e("a.two"), e("b.one")],
        false,
        &[],
        false,
        false,
        None,
    );
    let left: Vec<_> = p
        .errors
        .iter()
        .map(|e| e.identifier.clone().unwrap())
        .collect();
    assert_eq!(left, ["b.one"]);
    assert_eq!(
        p.string_errors,
        ["Ignored error pattern analyse/x/b was not matched in reported errors."]
    );
    let p = IgnoredErrors::new(&entries, true).process(
        vec![e("a.one")],
        false,
        &[],
        false,
        false,
        None,
    );
    assert_eq!(p.errors.len(), 1);
}

/// PHPStan 2.3.6's UnmatchedIgnoredErrorJudge (tests/fixtures/ignore-judge.json, exported by
/// scripts/export-ignore-judge-fixtures.php from the 2.3.6 phar): which unmatched entries an analysis of only some
/// paths reports, from the analysed and configured paths, excludePaths and the collected dependencies.
#[test]
fn unmatched_ignored_error_judge() {
    use pun_phpstan::ignore::AnalysedDependencies;
    let dir = format!("{}/tests/fixtures/ignore/files", env!("CARGO_MANIFEST_DIR"));
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = root.to_str().unwrap();
    let to_real = [("{dir}", dir.as_str()), ("{root}", root)];
    let data = placeholders(fixture("ignore-judge.json"), &to_real);
    let Value::Array(entries) = untagged(&data["ignoreErrors"]) else {
        panic!()
    };
    let strings = |j: &J| -> Vec<String> {
        (j.as_array().unwrap().iter())
            .map(|s| s.as_str().unwrap().to_string())
            .collect()
    };
    let errors: Vec<Error> = (data["errors"].as_array().unwrap().iter())
        .map(|e| {
            Error::new(
                e[0].as_str().unwrap(),
                format!("{dir}/{}", e[1].as_str().unwrap()),
                e[2].as_i64(),
            )
        })
        .collect();
    let mut failures = Vec::new();
    for case in data["cases"].as_array().unwrap() {
        let excludes = strings(&case["excludePaths"]);
        let excluder = pun_phpstan::php::FileExcluder::new(excludes.iter().map(String::as_str));
        let helper = IgnoredErrors::new(&entries, true).with_analysed_paths(
            &strings(&case["analysedPaths"]),
            &strings(&case["analysedPathsFromConfig"]),
            Some(std::sync::Arc::new(move |f: &str| excluder.is_excluded(f))),
        );
        // PHP writes no collected data as `[]`, and object keys keep their placeholders.
        let collected = (!case["collected"].is_null()).then(|| {
            let mut c = AnalysedDependencies::default();
            for (file, data) in case["collected"].as_object().into_iter().flatten() {
                let file = &file.replace("{dir}", &dir);
                if !data["traits"].as_array().unwrap().is_empty() {
                    c.trait_declaring_files.insert(file.clone());
                }
                let deps = data["files"].as_array().unwrap();
                c.files.insert(
                    file.clone(),
                    deps.iter().map(|d| d.as_str().map(String::from)).collect(),
                );
                c.declarations
                    .insert(file.clone(), strings(&data["declarations"]));
            }
            c
        });
        let result = helper.process(
            errors.clone(),
            case["onlyFiles"].as_bool().unwrap(),
            &strings(&case["analysedFiles"]),
            false,
            case["collectedDataRuleSkipped"].as_bool().unwrap(),
            collected.as_ref(),
        );
        let got = json!({
            "errors": result.errors.iter().map(|e| json!([e.file, e.line, e.message])).collect::<Vec<_>>(),
            "stringErrors": result.string_errors,
        });
        if got != case["expected"] {
            failures.push(format!(
                "{}\n  want {}\n  got  {got}",
                case["name"], case["expected"]
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn validate_ignore_errors() {
    use pun_phpstan::config::validate_ignore_errors;
    let params = |entry: &[(&str, Value)]| {
        let mut e = Array::new();
        for (k, v) in entry {
            e.set(Key::from(*k), v.clone());
        }
        let mut p = Array::new();
        p.set(
            Key::from("ignoreErrors"),
            Value::Array(Array::list([Value::Array(e)])),
        );
        p
    };
    assert_eq!(
        validate_ignore_errors(&params(&[("rule", Value::str("analyse/x/a"))])),
        Ok(())
    );
    assert_eq!(
        validate_ignore_errors(&params(&[
            ("rule", Value::str("analyse/x/a")),
            ("count", Value::Int(2))
        ]))
        .map_err(|e| e.to_string()),
        Err("Invalid configuration:\nAn ignoreErrors entry with count field must also contain path field.".into())
    );
    let e = validate_ignore_errors(&params(&[("message", Value::str("#(#"))]))
        .unwrap_err()
        .to_string();
    assert!(e.starts_with("Invalid entry in ignoreErrors:\n"), "{e}");
}

#[test]
fn unmatched_entries_are_reported_in_their_config_file() {
    let dir = std::env::temp_dir().join(format!("pun-ignore-location-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let write = |path: &str, contents: &str| {
        let path = dir.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    };
    write(
        "phpstan.neon",
        "parameters:\n\tignoreErrors:\n\t\t- '#^A$#'\n\t\t-\n\t\t\tidentifier: x.y\n\t\t-\n\t\t\tmessage: '#^B$#'\n\t\t\tpath: missing.php\n\t\t-\n\t\t\tmessages: ['#^C$#', '#^D$#']\n",
    );
    write("composer.json", "{}");
    write(
        "vendor/composer/installed.json",
        r#"{"packages": [{"name": "acme/pkg", "install-path": "../acme/pkg"}, {"name": "acme/own", "install-path": "../acme/own", "dist": {"type": "path"}}]}"#,
    );
    for package in ["pkg", "own"] {
        write(
            &format!("vendor/acme/{package}/extension.neon"),
            &format!("parameters:\n\tignoreErrors:\n\t\t- identifier: {package}.id\n"),
        );
    }
    let d = dir.to_string_lossy().into_owned();
    let entry = |fields: &[(&str, Value)]| {
        let mut e = Array::new();
        for (k, v) in fields {
            e.set((*k).into(), v.clone());
        }
        Value::Array(e)
    };
    let entries = Array::list([
        Value::str("#^A$#"),
        entry(&[("identifier", Value::str("x.y"))]),
        entry(&[
            ("message", Value::str("#^B$#")),
            ("path", Value::str(format!("{d}/missing.php"))),
        ]),
        entry(&[(
            "messages",
            Value::Array(Array::list([Value::str("#^C$#"), Value::str("#^D$#")])),
        )]),
        entry(&[("identifier", Value::str("pkg.id"))]),
        entry(&[("identifier", Value::str("own.id"))]),
    ]);
    let mut parameters = Array::new();
    parameters.set("currentWorkingDirectory".into(), Value::str(&d));
    parameters.set("rootDir".into(), Value::str("phar:///phpstan.phar"));
    parameters.set(
        "composerAutoloaderProjectPaths".into(),
        Value::Array(Array::list([Value::str(&d)])),
    );
    let files = [
        "phar:///phpstan.phar/conf/config.neon",
        "phpstan.neon",
        "vendor/acme/pkg/extension.neon",
        "vendor/acme/own/extension.neon",
    ];
    parameters.set(
        "allConfigFiles".into(),
        Value::Array(Array::list(files.iter().map(|f| {
            Value::str(&if f.starts_with("phar:") {
                f.to_string()
            } else {
                format!("{d}/{f}")
            })
        }))),
    );
    let p = IgnoredErrors::new(&entries, true)
        .with_config_files(&parameters)
        .process(Vec::new(), false, &[], false, false, None);
    let not_matched =
        |s: &str| format!("Ignored error pattern {s} was not matched in reported errors.");
    let got: Vec<_> = p
        .errors
        .iter()
        .map(|e| {
            (
                e.file.clone(),
                e.line,
                e.message.clone(),
                e.identifier.clone(),
                e.can_be_ignored,
            )
        })
        .collect();
    let at = |file: &str, line: i64, s: &str| {
        (
            format!("{d}/{file}"),
            Some(line),
            not_matched(s),
            Some("ignore.unmatched".to_string()),
            false,
        )
    };
    assert_eq!(
        got,
        [
            at("phpstan.neon", 3, "#^A$#"),
            at("phpstan.neon", 4, "x.y"),
            at("phpstan.neon", 6, &format!("#^B$# in path {d}/missing.php")),
            at("phpstan.neon", 9, "#^C$#"),
            at("phpstan.neon", 9, "#^D$#"),
            at("vendor/acme/own/extension.neon", 3, "own.id"),
        ]
    );
    assert_eq!(p.string_errors, [not_matched("pkg.id")]);
    std::fs::remove_dir_all(&dir).unwrap();
}
