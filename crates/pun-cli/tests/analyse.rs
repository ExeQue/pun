//! `pun analyse` against PHPStan 2.3.5 (`.oracle/phpstan`, scripts/oracle.sh): every `--format` (PHPStan's
//! `--error-format`) byte for byte and the exit code, on a project with errors and on a clean one. Without the
//! oracle (or `php`) the comparisons are skipped. The baseline and ignore cases are derived by hand from
//! context/plan/analyse.md Build 2 and rules.md §4.
mod common;

use common::{PUN, clean_env, project};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

const FORMATS: [&str; 9] = [
    "raw",
    "json",
    "prettyJson",
    "checkstyle",
    "junit",
    "gitlab",
    "github",
    "teamcity",
    "table",
];

const ERRORS: &str = "<?php\n\nnamespace App;\n\nfunction f(): void\n{\n    undefined_fn();\n    new Missing();\n}\n\nclass B\n{\n    public function g(): void\n    {\n        $this->nope();\n    }\n}\n";
const CLEAN: &str = "<?php\n\nnamespace App;\n\nfunction f(): int\n{\n    return 1;\n}\n";
const NON_IGNORABLE: &str = "<?php\nfunction duplicate($value, $value): void {}\nundefined_fn();\n";

fn phpstan() -> Option<PathBuf> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.oracle/phpstan/vendor/phpstan/phpstan/phpstan.phar");
    p.exists().then(|| p.canonicalize().unwrap())
}

fn sample(name: &str, source: &str) -> PathBuf {
    project(
        "analyse",
        name,
        &[
            (
                "phpstan.neon",
                "parameters:\n    level: 0\n    paths:\n        - src\n",
            ),
            ("pun.toml", "[analyse]\npaths = [\"src\"]\n"),
            ("src/a.php", source),
        ],
    )
}

fn run(dir: &Path, program: &[&str], args: &[&str]) -> (String, Option<i32>) {
    let out = clean_env(&mut Command::new(program[0]), &["PATH", "HOME"])
        .current_dir(dir)
        .env("COLUMNS", "80")
        .env("PUN_AGENT", "0")
        .args(&program[1..])
        .args(args)
        .output()
        .unwrap();
    (String::from_utf8(out.stdout).unwrap(), out.status.code())
}

fn formats_match_phpstan(name: &str, source: &str) {
    let Some(phar) = phpstan() else { return };
    let phar = phar.to_str().unwrap();
    let d = sample(name, source);
    for f in FORMATS {
        let format = format!("--error-format={f}");
        let want = run(
            &d,
            &[
                "php",
                phar,
                "analyse",
                "--no-progress",
                "-c",
                "phpstan.neon",
            ],
            &[&format],
        );
        let got = run(&d, &[PUN, "analyse"], &[&format]);
        assert_eq!(got, want, "{name} {f}");
    }
    let want = run(
        &d,
        &[
            "php",
            phar,
            "analyse",
            "--no-progress",
            "-c",
            "phpstan.neon",
            "-v",
        ],
        &["--error-format=raw"],
    );
    assert_eq!(
        run(&d, &[PUN, "analyse"], &["--error-format=raw", "-v"]),
        want,
        "{name} raw -v"
    );
}

#[test]
fn formats_match_phpstan_errors() {
    formats_match_phpstan("errors", ERRORS);
}

#[test]
fn formats_match_phpstan_clean() {
    formats_match_phpstan("clean", CLEAN);
}

#[test]
fn formats_match_phpstan_non_ignorable() {
    formats_match_phpstan("non-ignorable", NON_IGNORABLE);
}

#[test]
fn formats_match_phpstan_trait_context() {
    formats_match_phpstan(
        "trait-context",
        "<?php\n\nnamespace App\\Models;\n\ntrait T\n{\n    public function make(): static\n    {\n        return new static();\n    }\n}\n\nclass C\n{\n    use T;\n}\n",
    );
}

#[test]
fn anonymous_class_names_match_phpstan() {
    let Some(phar) = phpstan() else { return };
    let source = |class: &str, function: &str| {
        format!(
            "<?php\n\nnamespace App;\n\nclass {class}\n{{\n    public function __construct(public int $x)\n    {{\n    }}\n}}\n\nfunction {function}(): void\n{{\n    new class('a') extends {class} {{}};\n}}\n"
        )
    };
    let d = project(
        "analyse",
        "anonymous-names",
        &[
            (
                "phpstan.neon",
                "parameters:\n    level: max\n    paths:\n        - src\n",
            ),
            (
                "pun.toml",
                "[analyse]\npreset = \"max\"\npaths = [\"src\"]\n",
            ),
            ("src/Sub/a.php", &source("R", "f")),
            ("src/b.php", &source("Q", "g")),
        ],
    );
    let want = run(
        &d,
        &[
            "php",
            phar.to_str().unwrap(),
            "analyse",
            "--no-progress",
            "-c",
            "phpstan.neon",
        ],
        &["--error-format=raw"],
    );
    assert!(
        want.0.contains("App\\R@anonymous/Sub/a.php:14 constructor"),
        "{}",
        want.0
    );
    assert_eq!(run(&d, &[PUN, "analyse"], &["--error-format=raw"]), want);
}

#[test]
fn unused_traits_follow_the_analysed_paths() {
    let Some(phar) = phpstan() else { return };
    let d = project(
        "analyse",
        "unused-trait",
        &[
            (
                "phpstan.neon",
                "parameters:\n    level: max\n    paths:\n        - src\n",
            ),
            (
                "pun.toml",
                "[analyse]\npreset = \"max\"\npaths = [\"src\"]\n",
            ),
            (
                "src/a.php",
                "<?php\n\nnamespace App;\n\ntrait Unused\n{\n}\n",
            ),
        ],
    );
    let phpstan = [
        "php",
        phar.to_str().unwrap(),
        "analyse",
        "--no-progress",
        "-c",
        "phpstan.neon",
    ];
    let want = run(&d, &phpstan, &["--error-format=raw"]);
    assert!(
        want.0.contains("Trait App\\Unused is used zero times"),
        "{}",
        want.0
    );
    assert_eq!(run(&d, &[PUN, "analyse"], &["--error-format=raw"]), want);
    let want = run(&d, &phpstan, &["--error-format=raw", "src/a.php"]);
    assert!(!want.0.contains("Trait App\\Unused"), "{}", want.0);
    assert_eq!(
        run(&d, &[PUN, "analyse"], &["--error-format=raw", "src/a.php"]),
        want
    );
}

#[test]
fn formats_match_phpstan_dumps() {
    formats_match_phpstan(
        "dumps",
        "<?php\n\\PHPStan\\dumpType(1, true); // @phpstan-ignore phpstan.dumpType\n\\PHPStan\\dumpPhpDocType(['value' => 2]);\n",
    );
}

#[test]
fn unknown_format_is_a_run_error() {
    let d = sample("unknown", CLEAN);
    let (out, code) = run(&d, &[PUN, "analyse"], &["--format=nope"]);
    assert_eq!((out.as_str(), code), ("", Some(2)));
}

#[test]
fn parser_failures_are_severe_not_configurable_rule_errors() {
    let Some(phar) = phpstan() else { return };
    let phar = phar.to_str().unwrap();
    let d = sample("parser-severe", "<?php\n#[\\self] class InvalidA {}\n");
    common::write_files(
        &d,
        &[
            ("src/b.php", "<?php\n#[\\self] class InvalidB {}\n"),
            ("src/c.php", "<?php\nnew \\parent;\n"),
            ("src/control.php", ERRORS),
            (
                "phpstan.neon",
                "parameters:\n    level: 0\n    paths: [src]\n    ignoreErrors:\n        - '#^never matches$#'\n        -\n            identifier: phpstan.parse\n",
            ),
            (
                "pun.toml",
                "[analyse]\npaths = [\"src\"]\n[[analyse.ignore]]\nmessage = \"#^never matches$#\"\n[[analyse.ignore]]\nidentifier = \"phpstan.parse\"\n[analyse.rules]\n\"analyse/undefined/function\" = \"off\"\n",
            ),
        ],
    );
    for format in FORMATS {
        let arg = format!("--error-format={format}");
        let want = run(
            &d,
            &[
                "php",
                phar,
                "analyse",
                "--no-progress",
                "-c",
                "phpstan.neon",
            ],
            &[&arg],
        );
        assert_eq!(want.1, Some(1));
        assert_eq!(run(&d, &[PUN, "analyse"], &[&arg]), want, "{format}");
    }
    std::fs::write(
        d.join("src/a.php"),
        "<?php\n#[\\self] // @pun-ignore\nclass InvalidA {}\n",
    )
    .unwrap();
    let (out, code) = run(&d, &[PUN, "analyse"], &["--format=json"]);
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(code, Some(1));
    assert_eq!(json["totals"]["file_errors"], 2, "{out}");
    assert_eq!(json["totals"]["errors"], 0, "{out}");
    let (_, code) = run(&d, &[PUN, "analyse"], &["--generate-baseline", "--quiet"]);
    assert_eq!(code, Some(1));
    assert!(!d.join("pun-baseline.toml").exists());
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn syntax_errors_fail_the_run() {
    let d = sample("syntax-errors", "<?php\n$a = 1\n$b = 2;\n");
    common::write_files(
        &d,
        &[
            ("src/b.php", "<?php\nfunction f() {\n"),
            ("src/control.php", ERRORS),
        ],
    );
    let (out, code) = run(&d, &[PUN, "analyse"], &["--format=json"]);
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(code, Some(1));
    let mut found: Vec<(String, String, u64)> = (json["files"].as_object().unwrap().iter())
        .flat_map(|(file, entry)| {
            let name = file.rsplit('/').next().unwrap().to_string();
            (entry["messages"].as_array().unwrap().iter()).map(move |m| {
                let message = m["message"].as_str().unwrap().to_string();
                (name.clone(), message, m["line"].as_u64().unwrap())
            })
        })
        .collect();
    found.sort();
    let want = [
        (
            "a.php",
            "Syntax error, unexpected variable \"$b\" on line 3",
            3,
        ),
        ("b.php", "Unclosed '{' on line 2 on line 3", 3),
    ];
    let want: Vec<_> = (want.iter())
        .map(|(f, m, l)| (f.to_string(), m.to_string(), *l))
        .collect();
    assert_eq!(found, want, "{out}");
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn non_ignorable_errors_survive_comments_and_baselines() {
    let d = sample("non-ignorable-baseline", NON_IGNORABLE);
    let source = NON_IGNORABLE
        .replace(
            "void {}",
            "void {} // @pun-ignore analyse/functions/duplicate-parameter",
        )
        .replace(
            "undefined_fn();",
            "undefined_fn(); // @pun-ignore analyse/undefined/function",
        );
    std::fs::write(d.join("src/a.php"), source).unwrap();
    let (out, code) = run(&d, &[PUN, "analyse"], &["--format=json"]);
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(code, Some(1), "{out}");
    assert_eq!(json["totals"]["file_errors"], 1, "{out}");
    let file = d.join("src/a.php").to_string_lossy().into_owned();
    assert_eq!(
        json["files"][&file]["messages"][0]["identifier"], "parameter.duplicate",
        "{out}"
    );
    assert_eq!(
        json["files"][&file]["messages"][0]["ignorable"], false,
        "{out}"
    );

    std::fs::write(d.join("src/a.php"), NON_IGNORABLE).unwrap();
    let (out, code) = run(
        &d,
        &[PUN, "analyse"],
        &["--generate-baseline", "--color=never"],
    );
    assert_eq!(code, Some(0), "{out}");
    assert!(out.contains("Baseline generated with 1 error in"), "{out}");
    let mut baseline = std::fs::read_to_string(d.join("pun-baseline.toml")).unwrap();
    assert!(
        baseline.contains("Function undefined_fn not found."),
        "{baseline}"
    );
    assert!(!baseline.contains("Redefinition"), "{baseline}");
    baseline.push_str("\n[[issues]]\nfile = 'src/a.php'\ncode = 'analyse/functions/duplicate-parameter'\nmessage = 'Redefinition of parameter $value.'\ncount = 1\n");
    std::fs::write(d.join("pun-baseline.toml"), baseline).unwrap();
    std::fs::write(
        d.join("pun.toml"),
        "[analyse]\npaths = [\"src\"]\nbaseline = \"pun-baseline.toml\"\n",
    )
    .unwrap();
    let (out, code) = run(&d, &[PUN, "analyse"], &["--format=json"]);
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(code, Some(1), "{out}");
    assert_eq!(json["totals"]["file_errors"], 1, "{out}");
    assert_eq!(
        json["files"][&file]["messages"][0]["identifier"], "parameter.duplicate",
        "{out}"
    );
}

#[test]
fn baseline_covers_the_errors_it_lists() {
    let d = sample("baseline", ERRORS);
    let (out, code) = run(
        &d,
        &[PUN, "analyse"],
        &["--generate-baseline", "--color=never"],
    );
    assert_eq!(code, Some(0), "{out}");
    assert!(
        out.contains("Baseline generated with 3 errors in pun-baseline.toml"),
        "{out}"
    );
    let baseline = std::fs::read_to_string(d.join("pun-baseline.toml")).unwrap();
    assert!(baseline.contains("variant = \"loose\""), "{baseline}");
    assert!(
        baseline.contains("message = 'Call to an undefined method App\\B::nope().'"),
        "{baseline}"
    );
    assert_eq!(run(&d, &[PUN, "analyse"], &["--format=raw"]).1, Some(1));
    std::fs::write(
        d.join("pun.toml"),
        "[analyse]\npaths = [\"src\"]\nbaseline = \"pun-baseline.toml\"\n",
    )
    .unwrap();
    assert_eq!(
        run(&d, &[PUN, "analyse"], &["--format=raw"]),
        (String::new(), Some(0))
    );
    std::fs::write(
        d.join("src/a.php"),
        ERRORS.replace("$this->nope();", "nope2();"),
    )
    .unwrap();
    let (out, code) = run(&d, &[PUN, "analyse"], &["--color=never"]);
    assert!(out.contains("nope2 not found."), "{out}");
    assert!(
        out.contains(
            "1 baseline entry no longer matches: run `pun analyse --generate-baseline` to drop it\n"
        ),
        "{out}"
    );
    assert_eq!(code, Some(1));
    std::fs::write(d.join("src/a.php"), CLEAN).unwrap();
    let (out, _) = run(&d, &[PUN, "analyse"], &["--color=never"]);
    assert!(
        out.contains("! 3 baseline entries no longer match: run `pun analyse --generate-baseline` to drop them\n"),
        "{out}"
    );
}

#[test]
fn ignore_comments_drop_errors() {
    let source = ERRORS
        .replace(
            "    undefined_fn();",
            "    undefined_fn(); // @pun-ignore analyse/undefined/function",
        )
        .replace(
            "    new Missing();",
            "    // @pun-ignore\n    new Missing();",
        );
    let d = sample("ignores", &source);
    let (out, code) = run(&d, &[PUN, "analyse"], &["--format=raw"]);
    let lines: Vec<&str> = out
        .lines()
        .map(|l| l.rsplit_once("/src/").unwrap().1)
        .collect();
    assert_eq!(
        lines,
        [
            "a.php:9:Instantiated class App\\Missing not found.",
            "a.php:16:Call to an undefined method App\\B::nope().",
        ]
    );
    assert_eq!(code, Some(1));
}

fn pun(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> (String, String, Option<i32>) {
    let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
        .current_dir(dir)
        .env("COLUMNS", "80")
        .env("PUN_AGENT", "0")
        .envs(env.iter().copied())
        .arg("analyse")
        .args(args)
        .output()
        .unwrap();
    let s = |b: Vec<u8>| String::from_utf8(b).unwrap();
    (s(out.stdout), s(out.stderr), out.status.code())
}

const MALFORMED: &str = "<?php\n\nnamespace App;\n\nfunction f(): void\n{\n    undefined_fn();\n    new Missing(); // @pun-ignore\n}\n";

#[test]
fn malformed_ignores_show_in_every_output() {
    let d = sample("malformed", MALFORMED);
    let (out, _, code) = pun(&d, &["--color=never"], &[]);
    assert!(
        out.contains(" x invalid-ignore-comment  `@pun-ignore` needs one or more rule IDs\n  ,- src/a.php:8:23\n"),
        "{out}"
    );
    assert_eq!(code, Some(1));
    let (out, _, _) = pun(&d, &["--color=never"], &[("GITHUB_ACTIONS", "true")]);
    let annotations: Vec<&str> = out.lines().filter(|l| l.starts_with("::")).collect();
    assert_eq!(
        annotations,
        [
            "::error file=src/a.php,line=7,col=5,title=analyse/undefined/function::Function undefined_fn not found.",
            "::error file=src/a.php,line=8,col=5,title=analyse/classes/invalid-new::Instantiated class App\\Missing not found.",
            "::error file=src/a.php,line=8,col=23,title=invalid-ignore-comment::`@pun-ignore` needs one or more rule IDs",
        ]
    );
    assert_eq!(
        pun(&d, &["-q"], &[]),
        (String::new(), String::new(), Some(1))
    );
}

#[test]
fn generate_baseline_to_a_named_file() {
    let d = sample("baseline-path", ERRORS);
    let (out, _, code) = pun(&d, &["--generate-baseline", "b.toml", "--color=never"], &[]);
    assert!(
        out.contains("Baseline generated with 3 errors in b.toml; add `baseline = \"b.toml\"` under [analyse] in pun.toml to use it ("),
        "{out}"
    );
    assert_eq!(code, Some(0));
    assert!(d.join("b.toml").exists());
    assert!(!d.join("pun-baseline.toml").exists());
    // The configured baseline is the default file, and needs no hint.
    let toml = "[analyse]\npaths = [\"src\"]\nbaseline = \"b.toml\"\n";
    std::fs::write(d.join("pun.toml"), toml).unwrap();
    std::fs::remove_file(d.join("b.toml")).unwrap();
    let (out, _, code) = pun(&d, &["--generate-baseline", "--color=never"], &[]);
    assert!(
        out.contains("Baseline generated with 3 errors in b.toml ("),
        "{out}"
    );
    assert_eq!(code, Some(0));
    assert!(d.join("b.toml").exists());
    std::fs::write(d.join("pun.toml"), "[analyse]\npaths = [\"src\"]\n").unwrap();
    assert_eq!(
        pun(&d, &["--generate-baseline", "-q"], &[]),
        (String::new(), String::new(), Some(0))
    );
    assert!(d.join("pun-baseline.toml").exists());
}

#[test]
fn run_errors_exit_2() {
    use std::os::unix::fs::PermissionsExt;
    let d = sample("bad-config", CLEAN);
    std::fs::write(d.join("pun.toml"), "[analyse]\nnope = 1\n").unwrap();
    let (out, err, code) = pun(&d, &[], &[]);
    assert_eq!((out.as_str(), code), ("", Some(2)));
    assert!(err.contains("unknown field `nope`"), "{err}");
    let d = sample("bad-baseline", ERRORS);
    std::fs::write(
        d.join("pun.toml"),
        "[analyse]\npaths = [\"src\"]\nbaseline = \"b.toml\"\n",
    )
    .unwrap();
    let b = d.join("b.toml");
    let hint = "  run `pun analyse --generate-baseline` to write it again\n";
    let (out, err, code) = pun(&d, &["--color=never"], &[]);
    assert_eq!(
        (out.as_str(), err, code),
        (
            "",
            format!(
                "x {}  can't read the baseline: No such file or directory (os error 2)\n{hint}",
                b.display()
            ),
            Some(2)
        )
    );
    std::fs::write(&b, "x = \n").unwrap();
    let (_, err, code) = pun(&d, &["--color=never"], &[]);
    assert_eq!(
        (err, code),
        (
            format!(
                "x {}  invalid baseline: string values must be quoted, expected literal string\n{hint}",
                b.display()
            ),
            Some(2)
        )
    );
    std::fs::create_dir(d.join("dir.toml")).unwrap();
    let (_, err, code) = pun(
        &d,
        &["--generate-baseline", "dir.toml", "--color=never"],
        &[],
    );
    assert_eq!(
        (err, code),
        (
            format!(
                "x can't write the baseline to {}: Is a directory (os error 21)\n",
                d.join("dir.toml").display()
            ),
            Some(2)
        )
    );
    // PHPStan's "Invalid entries in ignoreErrors" (its output on the same entries as ignoreErrors) is a run error.
    let d = sample("bad-ignores", CLEAN);
    std::fs::write(
        d.join("pun.toml"),
        "[analyse]\npaths = [\"src\"]\n\n[[analyse.ignore]]\nmessage = \"#(#\"\npaths = [\"src/a.php\"]\n\n[[analyse.ignore]]\nmessage = \"#^Nope#\"\npaths = [\"src/nothere.php\"]\n",
    )
    .unwrap();
    let (out, err, code) = pun(&d, &["--color=never"], &[]);
    assert_eq!((out.as_str(), code), ("", Some(2)));
    for line in [
        "Invalid entries in ignoreErrors:".to_string(),
        "Compilation failed: missing closing parenthesis at offset 1 in pattern: #(#".into(),
        format!(
            "Path \"{}\" is neither a directory, nor a file path, nor a fnmatch pattern.",
            d.join("src/nothere.php").display()
        ),
    ] {
        assert!(err.contains(&line), "{err}");
    }
    let d = sample("unreadable", CLEAN);
    let file = d.join("src/a.php");
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&file).is_err() {
        let (out, _, code) = pun(&d, &[], &[]);
        assert_eq!((out.as_str(), code), ("", Some(2)));
    }
}

#[test]
fn table_and_teamcity_match_phpstan() {
    let Some(phar) = phpstan() else { return };
    let phar = phar.to_str().unwrap();
    let outside = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("analyse-outside-{}", std::process::id()));
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("o.php"), "<?php\nnope2();\n").unwrap();
    let outside = outside.display();
    let long = "<?php\nthis_is_an_extremely_long_function_name_to_wrap();\n";
    #[expect(
        clippy::type_complexity,
        reason = "a table of test cases: name, files, working directory, paths"
    )]
    let cases: [(&str, &[(&str, &str)], &str, &str); 5] = [
        (
            "escapes",
            &[("src/it's [a|b]/x.php", "<?php\nnope();\n")],
            "",
            "src",
        ),
        (
            "file",
            &[("src/a.php", long), ("src/b.php", "<?php\nnope();\n")],
            "",
            "src/a.php",
        ),
        (
            "outside",
            &[
                ("src/a.php", "<?php\nnope();\n"),
                ("lib/b.php", "<?php\nnope3();\n"),
            ],
            "",
            &format!("src\", \"lib\", \"{outside}"),
        ),
        (
            "outside-first",
            &[("src/a.php", "<?php\nnope();\n")],
            "",
            &format!("{outside}\", \"src"),
        ),
        (
            "parent",
            &[
                ("proj/src/a.php", "<?php\nnope();\n"),
                ("x/o.php", "<?php\n\u{20000}();\n\u{30000}();\n"),
            ],
            "proj",
            "../x\", \"src",
        ),
    ];
    for (name, files, cwd, paths) in cases {
        let d = project("analyse", name, files).join(cwd);
        let list: Vec<&str> = paths.split("\", \"").collect();
        let neon: String = list.iter().fold(String::new(), |mut out, p| {
            let _ = writeln!(out, "        - {p}");
            out
        });
        std::fs::write(
            d.join("phpstan.neon"),
            format!("parameters:\n    level: 0\n    paths:\n{neon}"),
        )
        .unwrap();
        std::fs::write(
            d.join("pun.toml"),
            format!("[analyse]\npaths = [\"{paths}\"]\n"),
        )
        .unwrap();
        for f in ["table", "teamcity"] {
            for columns in ["80", "40"] {
                let format = format!("--error-format={f}");
                let with = |program: &[&str]| {
                    let out = clean_env(&mut Command::new(program[0]), &["PATH", "HOME"])
                        .current_dir(&d)
                        .env("COLUMNS", columns)
                        .env("PUN_AGENT", "0")
                        .args(&program[1..])
                        .arg(&format)
                        .output()
                        .unwrap();
                    (String::from_utf8(out.stdout).unwrap(), out.status.code())
                };
                let want = with(&[
                    "php",
                    phar,
                    "analyse",
                    "--no-progress",
                    "-c",
                    "phpstan.neon",
                ]);
                assert_eq!(with(&[PUN, "analyse"]), want, "{name} {f} {columns}");
            }
        }
    }
}

#[test]
fn check_reports_analyse_findings() {
    let d = project(
        "analyse",
        "check",
        &[
            (
                "pun.toml",
                "[format]\npreset = \"empty\"\nexclude = [\"lib\"]\n\n[lint]\nexclude = [\"lib\"]\n\n[analyse]\npaths = [\"src\", \"lib\"]\nbaseline = \"b.toml\"\n",
            ),
            ("src/a.php", "<?php\n\nnope(); // @pun-ignore\n"),
            ("lib/b.php", "<?php\n\nnope2();\n"),
        ],
    );
    assert_eq!(
        pun(&d, &["--generate-baseline", "b.toml", "-q"], &[]).2,
        Some(0)
    );
    std::fs::write(d.join("lib/b.php"), "<?php\n\nnope3();\n").unwrap();
    let check = |env: &[(&str, &str)]| {
        let out = clean_env(&mut Command::new(PUN), &["PATH", "HOME"])
            .current_dir(&d)
            .env("PUN_AGENT", "0")
            .envs(env.iter().copied())
            .args(["check", "--color=never"])
            .output()
            .unwrap();
        (String::from_utf8(out.stdout).unwrap(), out.status.code())
    };
    let (out, code) = check(&[("PUN_AGENT", "1")]);
    let lines: Vec<&str> = out.lines().skip(1).collect();
    assert_eq!(
        lines,
        [
            "lib/b.php:3:1 analyse/undefined/function Function nope3 not found.",
            "src/a.php:3:12 invalid-ignore-comment `@pun-ignore` needs one or more rule IDs",
        ]
    );
    assert_eq!(code, Some(1));
    let (out, _) = check(&[]);
    assert!(
        out.contains("! 1 baseline entry no longer matches: run `pun analyse --generate-baseline` to drop it\n"),
        "{out}"
    );
}

#[test]
fn ignores_match_phpstan() {
    let neon = "parameters:\n    level: 0\n    paths:\n        - src\n    ignoreErrors:\n        - message: '#^Function undefined_fn not found\\.$#'\n          path: src/a.php\n        - identifier: method.notFound\n          path: src/a.php\n          count: 2\n        - message: '#^Never$#'\n          path: src/a.php\n        - rawMessage: 'Nope'\n          paths: [src/*, lib/*]\n";
    let toml = "[analyse]\npaths = [\"src\"]\npreset = \"basic\"\n\n[[analyse.ignore]]\nmessage = '#^Function undefined_fn not found\\.$#'\npaths = [\"src/a.php\"]\n\n[[analyse.ignore]]\nidentifier = \"method.notFound\"\npaths = [\"src/a.php\"]\ncount = 2\n\n[[analyse.ignore]]\nmessage = \"#^Never$#\"\npaths = [\"src/a.php\"]\n\n[[analyse.ignore]]\nraw-message = \"Nope\"\npaths = [\"src/*\", \"lib/*\"]\n";
    let d = project(
        "analyse",
        "ignores",
        &[
            ("phpstan.neon", neon),
            ("pun.toml", toml),
            ("src/a.php", ERRORS),
        ],
    );
    let (out, _, code) = pun(&d, &["--color=never"], &[]);
    assert_eq!(code, Some(1));
    let general = format!(
        "Ignored error \"Nope\" in paths: {0}/src/*, {0}/lib/* was not matched in reported errors.",
        d.display()
    );
    assert!(out.contains(&format!("x {general}\n")), "{out}");
    assert!(out.contains("4 errors"), "{out}");
    let (out, _, _) = pun(&d, &[], &[("PUN_AGENT", "1")]);
    assert!(
        out.contains(&format!("unmatched-ignore {general}")),
        "{out}"
    );
    let (out, _, code) = pun(&d, &["--generate-baseline", "--color=never"], &[]);
    assert!(out.contains("Baseline generated with 1 error in"), "{out}");
    assert_eq!(code, Some(0));
    std::fs::remove_file(d.join("pun-baseline.toml")).unwrap();
    let Some(phar) = phpstan() else { return };
    let phar = phar.to_str().unwrap();
    for f in FORMATS {
        let format = format!("--error-format={f}");
        let phpstan = [
            "php",
            phar,
            "analyse",
            "--no-progress",
            "-c",
            "phpstan.neon",
        ];
        let mut want = run(&d, &phpstan, &[&format, "-v"]);
        want.0 = want.0.replace("file='phpstan.neon'", "file='pun.toml'");
        let got = run(&d, &[PUN, "analyse"], &[&format, "-v"]);
        assert_eq!(got, want, "{f}");
    }
}

#[test]
fn no_rules_detected() {
    let d = sample("no-rules", ERRORS);
    let groups = "arrays attributes casing casts classes comments control-flow dead-code debug deprecated functions imports laravel operators phpdoc statements strings types undefined";
    let off: String = groups.split(' ').fold(String::new(), |mut out, g| {
        let _ = writeln!(out, "{g} = \"off\"");
        out
    });
    std::fs::write(
        d.join("pun.toml"),
        format!("[analyse]\npaths = [\"src\"]\n\n[analyse.rules]\n{off}"),
    )
    .unwrap();
    let (out, err, code) = pun(&d, &["--color=never"], &[]);
    assert_eq!((out.as_str(), code), ("", Some(2)));
    assert!(err.starts_with("x No rules detected\n"), "{err}");
}

/// PHPStan's fixable errors (a named argument to a `@no-named-arguments` function, at level 5 and the
/// `recommended` preset): the table's 🔧 lines for the reported and the ignored ones, byte for byte with PHPStan on
/// the same ignoreErrors.
#[test]
fn fixable_ignored_errors_match_phpstan() {
    let Some(phar) = phpstan() else { return };
    let phar = phar.to_str().unwrap();
    let named = |f: &str, n: &str| {
        format!("<?php\n\n/** @no-named-arguments */\nfunction {f}(int $a): void {{}}\n\n{n}\n")
    };
    for (name, paths) in [("both", "src"), ("ignored-only", "src/sub")] {
        let d = project(
            "analyse",
            &format!("fixable-{name}"),
            &[
                (
                    "phpstan.neon",
                    &format!(
                        "parameters:\n    level: 5\n    paths: [{paths}]\n    ignoreErrors:\n        - identifier: argument.named\n          path: src/sub/b.php\n"
                    ),
                ),
                (
                    "pun.toml",
                    &format!(
                        "[analyse]\npaths = [\"{paths}\"]\n\n[[analyse.ignore]]\nidentifier = \"argument.named\"\npaths = [\"src/sub/b.php\"]\n"
                    ),
                ),
                ("src/a.php", &named("f", "f(a: 1);\nf(a: 2);")),
                ("src/sub/b.php", &named("g", "g(a: 3);")),
            ],
        );
        let phpstan = [
            "php",
            phar,
            "analyse",
            "--no-progress",
            "-c",
            "phpstan.neon",
        ];
        let want = run(&d, &phpstan, &["--error-format=table"]);
        assert!(want.0.contains("--fix-ignored"), "{}", want.0);
        assert_eq!(
            run(&d, &[PUN, "analyse"], &["--error-format=table"]),
            want,
            "{name}"
        );
    }
}

/// Composer's autoloaded classes are symbols (analyse.md §3): the class under the psr-4 directory is found, and
/// an `[analyse] exclude` elsewhere leaves it alone. PHPStan at level 0, after `composer install`, reports only
/// `Missing` too.
#[test]
fn composer_autoloaded_classes_are_symbols() {
    let d = project(
        "analyse",
        "autoload",
        &[
            (
                "composer.json",
                r#"{"autoload":{"psr-4":{"App\\":"app/"}}}"#,
            ),
            (
                "pun.toml",
                "[analyse]\npreset = \"basic\"\npaths = [\"src\"]\nexclude = [\"app/Other.php\"]\n",
            ),
            ("app/Kept.php", "<?php\n\nnamespace App;\n\nclass Kept {}\n"),
            (
                "app/Other.php",
                "<?php\n\nnamespace App;\n\nclass Other {}\n",
            ),
            (
                "src/a.php",
                "<?php\n\nnew \\App\\Kept();\nnew \\App\\Missing();\n",
            ),
        ],
    );
    let (out, code) = run(&d, &[PUN, "analyse"], &["--error-format=raw"]);
    assert_eq!(
        (out, code),
        (
            format!(
                "{}:4:Instantiated class App\\Missing not found.\n",
                d.join("src/a.php").display()
            ),
            Some(1)
        )
    );
}
