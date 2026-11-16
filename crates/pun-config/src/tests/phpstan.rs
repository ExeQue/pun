use super::*;
use crate::migrate::Parts;
use crate::migrate::phpstan::{Phpstan, compose, from_phpstan};

fn project(neon: &str, extra: &[(&str, &str)]) -> PathBuf {
    let d = dir(&[]);
    for p in [
        "app/Legacy",
        "tests",
        "stubs",
        "vendor/larastan/larastan",
        "vendor/foo/bar",
        "build",
    ] {
        std::fs::create_dir_all(d.join(p)).unwrap();
    }
    std::fs::create_dir_all(d.join("vendor/phpstan/phpstan/conf")).unwrap();
    std::fs::write(
        d.join("vendor/phpstan/phpstan/conf/bleedingEdge.neon"),
        "includes:\n\t- phar://phpstan.phar/conf/bleedingEdge.neon\n",
    )
    .unwrap();
    std::fs::write(d.join("phpstan.neon"), neon).unwrap();
    std::fs::create_dir_all(d.join("app/Models")).unwrap();
    std::fs::write(d.join("app/Models/User.php"), "<?php\n").unwrap();
    for (n, c) in extra {
        std::fs::create_dir_all(d.join(n).parent().unwrap()).unwrap();
        std::fs::write(d.join(n), c).unwrap();
    }
    d
}

const NEON: &str = "includes:
    - vendor/larastan/larastan/extension.neon
    - vendor/phpstan/phpstan/conf/bleedingEdge.neon
    - phpstan-baseline.neon
    - build/custom.neon
    - vendor/phpstan/phpstan/conf/extra.neon
parameters:
    level: 5
    paths:
        - app
        - tests/
    excludePaths:
        - app/Legacy
        - /tmp
    phpVersion: 80400
    tmpDir: build/phpstan
    checkUninitializedProperties: true
    checkMissingTypehints: true
    treatPhpDocTypesAsCertain: true
    universalObjectCratesClasses: [App\\Bag]
    checkBenevolentUnionTypes: true
    additionalConstructors: [App\\Bag::setUp]
    exceptions:
        implicitThrows: false
    stubFiles:
        - stubs/Foo.stub
    ignoreErrors:
        - '#^Call to an undefined method#'
        -
            identifier: method.notFound
            path: app/Models/User.php
            count: 2
        -
            message: '#Access#'
            identifiers: [property.notFound, number.alwaysTrue]
            paths: [app/*]
        -
            rawMessage: 'Exact.'
            identifier: no.suchIdentifier
        -
            identifier: whitespace.bom
        -
            identifier: phpstanApi.method
";

const GENERATED: &str = "<?php declare(strict_types = 1);

namespace PHPStan\\ExtensionInstaller;

final class GeneratedConfig
{

\tpublic const EXTENSIONS = array (
  'foo/bar' => 
  array (
    'install_path' => '/p/vendor/foo/bar',
    'relative_install_path' => '../../../foo/bar',
    'extra' => NULL,
    'version' => '1.0.0',
  ),
  'phpstan/phpstan-mockery' => 
  array (
    'install_path' => '/p/vendor/phpstan/phpstan-mockery',
    'relative_install_path' => '../../phpstan-mockery',
    'extra' => 
    array (
      'includes' => 
      array (
        0 => 'extension.neon',
      ),
    ),
    'version' => '2.0.0',
  ),
  'phpstan/phpstan-strict-rules' => 
  array (
    'install_path' => '/p/vendor/phpstan/phpstan-strict-rules',
    'relative_install_path' => '../../phpstan-strict-rules',
    'extra' => 
    array (
      'includes' => 
      array (
        0 => 'rules.neon',
      ),
    ),
    'version' => '2.0.7',
  ),
);

\tpublic const NOT_INSTALLED = array (
  'phpstan/phpstan-phpunit' => 
  array (
  ),
);

}
";

#[test]
fn reads_level_paths_ignores_and_includes() {
    let d = project(
        NEON,
        &[
            ("vendor/larastan/larastan/extension.neon", ""),
            ("vendor/foo/bar/rules.neon", ""),
            ("vendor/phpstan/phpstan/conf/extra.neon", ""),
            ("vendor/phpstan/phpstan-mockery/extension.neon", ""),
            ("vendor/phpstan/phpstan-strict-rules/rules.neon", ""),
            (
                "build/custom.neon",
                "includes:\n    - ../vendor/foo/bar/rules.neon\nparameters:\n    checkFunctionNameCase: true\n",
            ),
            (
                "phpstan-baseline.neon",
                "parameters:\n    ignoreErrors:\n        - '#x#'\n",
            ),
        ],
    );
    let installer = d.join("vendor/phpstan/extension-installer/src");
    std::fs::create_dir_all(&installer).unwrap();
    let generated = GENERATED.replace("'/p/", &format!("'{}/", d.display()));
    std::fs::write(installer.join("GeneratedConfig.php"), generated).unwrap();
    let p = from_phpstan(&d, &d.join("phpstan.neon")).unwrap();
    assert_eq!(p.level, 5);
    assert_eq!(p.paths, ["app", "tests"]);
    assert_eq!(p.exclude, ["app/Legacy", "/tmp"]);
    assert_eq!(p.stubs, ["stubs/Foo.stub"]);
    assert_eq!(p.baselines, ["phpstan-baseline.neon"]);
    assert_eq!(
        p.extensions.iter().collect::<Vec<_>>(),
        ["larastan/larastan", "phpstan/phpstan-strict-rules"]
    );
    assert!(p.bleeding_edge);
    let names: Vec<&str> = p.unsupported.iter().map(|u| u.0.as_str()).collect();
    assert_eq!(
        names,
        [
            "build/custom.neon",
            "foo/bar",
            "phpstan/phpstan-mockery",
            "parameter checkMissingTypehints",
            "parameter phpVersion",
            "parameter universalObjectCratesClasses",
            "ignoreErrors no.suchIdentifier",
            "ignoreErrors whitespace.bom",
            "ignoreErrors phpstanApi.method",
        ]
    );
    let why = |n: &str| &p.unsupported.iter().find(|u| u.0 == n).unwrap().1;
    assert_eq!(
        why("ignoreErrors whitespace.bom"),
        "`whitespace.bom` is the format rule `format/whitespace/encoding` in Pun; ignore it there"
    );
    assert_eq!(
        p.changed,
        [
            (
                "additionalConstructors".to_string(),
                pun_phpstan::Value::Array(pun_phpstan::Array::list([pun_phpstan::Value::Str(
                    "App\\Bag::setUp".into()
                )]))
            ),
            (
                "checkBenevolentUnionTypes".to_string(),
                pun_phpstan::Value::Bool(true)
            ),
            (
                "checkFunctionNameCase".to_string(),
                pun_phpstan::Value::Bool(true)
            ),
            (
                "exceptions.implicitThrows".to_string(),
                pun_phpstan::Value::Bool(false)
            ),
        ]
    );
    assert_eq!(
        why("parameter checkMissingTypehints"),
        "the level sets it; Pun's analyse presets decide it"
    );
    assert_eq!(
        why("parameter universalObjectCratesClasses"),
        "no pun.toml setting for this parameter yet"
    );
    assert_eq!(
        why("ignoreErrors phpstanApi.method"),
        "`phpstanApi.method`: PHPStan extension-API checks; Pun has no PHP extension API"
    );
    assert_eq!(
        p.ignores,
        [
            "[[analyse.ignore]]\nmessage = \"#^Call to an undefined method#\"\n",
            "[[analyse.ignore]]\nrule = \"undefined/method\"\nidentifier = \"method.notFound\"\npaths = [\"app/Models/User.php\"]\ncount = 2\n",
            "[[analyse.ignore]]\nrule = \"undefined/property\"\nidentifier = \"property.notFound\"\nmessage = \"#Access#\"\npaths = [\"app/*\"]\n",
            "[[analyse.ignore]]\nrule = \"dead-code/number-comparison-always-constant\"\nidentifier = \"number.alwaysTrue\"\nmessage = \"#Access#\"\npaths = [\"app/*\"]\n",
        ]
    );
    let mut parts = Parts::default();
    let done = compose(&d, &mut parts, &p).unwrap();
    let text = parts.render();
    for line in [
        "\"undefined/function\" = { check-function-name-case = true }",
        "\"undefined/method\" = { check-benevolent-union-types = true, check-function-name-case = true }",
        "\"classes/uninitialized-property\" = { additional-constructors = ['App\\Bag::setUp'] }",
        "\"types/throws-not-covariant\" = \"off\"",
    ] {
        assert!(text.contains(&format!("\n{line}\n")), "{line}\n{text}");
    }
    assert_eq!(done.options, 3);
    let cfg = crate::from_toml(&d, &text).unwrap();
    assert!(cfg.analyse.bleeding_edge);
    let function = cfg
        .analyse
        .rules
        .iter()
        .find(|r| r.id == "analyse/undefined/function");
    assert_eq!(
        function.unwrap().options,
        serde_json::json!({"check-function-name-case": true})
    );
    assert!(
        parts.render().contains(
            "\"dead-code/no-unused-private-method\" = \"error\"\n"
        ) && parts.render().contains(
            "\n\n[analyse]\npreset = \"recommended\"\npaths = [\"app\", \"tests\"]\nexclude = [\"app/Legacy\", \"/tmp\"]\nstubs = [\"stubs/Foo.stub\"]\nbleeding-edge = true\n"
        ),
        "{}",
        parts.render()
    );
}

#[test]
fn migrates_maybe_undefined_variables() {
    use pun_phpstan::Value as V;
    for level in [0, 1] {
        for explicit in [None, Some(false), Some(true)] {
            let option = explicit.map_or_else(String::new, |value| {
                format!("    checkMaybeUndefinedVariables: {value}\n")
            });
            let neon = format!("parameters:\n    level: {level}\n{option}");
            let d = project(&neon, &[]);
            let p = from_phpstan(&d, &d.join("phpstan.neon")).unwrap();
            let expected = explicit.unwrap_or(level >= 1);
            let changed = explicit.is_some_and(|value| value != (level >= 1));
            let changes = if changed {
                vec![(
                    "checkMaybeUndefinedVariables".to_string(),
                    V::Bool(expected),
                )]
            } else {
                vec![]
            };
            assert_eq!(p.changed, changes, "{neon}");
            assert!(p.unsupported.is_empty(), "{neon}: {:?}", p.unsupported);
            let mut parts = Parts::default();
            let done = compose(&d, &mut parts, &p).unwrap();
            assert_eq!(done.options, usize::from(changed), "{neon}");
            let text = parts.render();
            let a = crate::from_toml(&d, &text).unwrap().analyse;
            assert_eq!(a.level(), level, "{text}");
            for consumer in [
                "analyse/undefined/variable",
                "analyse/undefined/compact-variable",
            ] {
                let rule = a.rules.iter().find(|rule| rule.id == consumer).unwrap();
                let options = if changed {
                    serde_json::json!({"maybe": expected})
                } else {
                    serde_json::Value::Null
                };
                assert_eq!(rule.options, options, "{neon}: {consumer}\n{text}");
            }
            let parameters = a.parameters(&d);
            assert_eq!(
                parameters.get_str("checkMaybeUndefinedVariables"),
                Some(&V::Bool(expected)),
                "{neon}\n{text}"
            );
            assert_eq!(
                parameters.get_str("cliArgumentsVariablesRegistered"),
                Some(&V::Bool(true)),
                "{neon}\n{text}"
            );
            std::fs::remove_dir_all(d).unwrap();
        }
    }
}

#[test]
fn argument_settings_preserve_defaults_and_report_unmapped_overrides() {
    use pun_phpstan::Value as V;

    for level in [0, 5] {
        for types in [false, true] {
            for references in [false, true] {
                for this_only in [false, true] {
                    let neon = format!(
                        "parameters:\n    level: {level}\n    checkFunctionArgumentTypes: {types}\n    checkArgumentsPassedByReference: {references}\n    checkThisOnly: {this_only}\n"
                    );
                    let d = project(&neon, &[]);
                    let p = from_phpstan(&d, &d.join("phpstan.neon")).unwrap();
                    let settings = [
                        ("checkFunctionArgumentTypes", types, level >= 5),
                        ("checkArgumentsPassedByReference", references, level >= 5),
                        ("checkThisOnly", this_only, level < 2),
                    ];
                    let mut unsupported: Vec<_> = settings.iter().filter(|(_, value, default)| value != default)
                        .map(|(name, _, _)| (format!("parameter {name}"), if *name == "checkThisOnly" {
                            "independent receiver eligibility has no pun.toml option yet".to_string()
                        } else {
                            "independent argument-check overrides have no pun.toml option yet".to_string()
                        })).collect();
                    unsupported.sort();
                    assert_eq!(p.unsupported, unsupported, "{neon}");
                    assert!(p.changed.is_empty(), "{neon}: {:?}", p.changed);
                    let mut parts = Parts::default();
                    compose(&d, &mut parts, &p).unwrap();
                    let text = parts.render();
                    let a = crate::from_toml(&d, &text).unwrap().analyse;
                    assert_eq!(a.level(), level, "{neon}\n{text}");
                    let parameters = a.parameters(&d);
                    for (name, _, default) in settings {
                        assert_eq!(
                            parameters.get_str(name),
                            Some(&V::Bool(default)),
                            "{neon}\n{text}"
                        );
                    }
                    std::fs::remove_dir_all(d).unwrap();
                }
            }
        }
    }
}

#[test]
fn reads_max_and_rejects_bad_levels() {
    let d = project("parameters:\n    level: max\n", &[]);
    assert_eq!(from_phpstan(&d, &d.join("phpstan.neon")).unwrap().level, 10);
    let d = project("parameters:\n    level: 11\n", &[]);
    let e = from_phpstan(&d, &d.join("phpstan.neon"))
        .unwrap_err()
        .to_string();
    assert!(
        e.starts_with("Level config file phar://")
            && e.ends_with("/conf/config.level11.neon was not found."),
        "{e}"
    );
}

#[test]
fn takes_phar_bleeding_edge() {
    let d = project(
        "includes:\n    - phar://phpstan.phar/conf/bleedingEdge.neon\nparameters:\n    level: 0\n",
        &[],
    );
    let p = from_phpstan(&d, &d.join("phpstan.neon")).unwrap();
    assert!(p.bleeding_edge);
    assert!(p.unsupported.is_empty(), "{:?}", p.unsupported);
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "one test walks every case of the behaviour it checks"
)]
fn composes_the_level_extensions_and_parameters() {
    let at_in =
        |level: u8, ext: &[&str], strict: &[(&str, bool)], files: &[(&str, &str)], format: &str| {
            let d = project(&format!("parameters:\n    level: {level}\n"), files);
            let mut p: Phpstan = from_phpstan(&d, &d.join("phpstan.neon")).unwrap();
            p.extensions = ext.iter().map(std::string::ToString::to_string).collect();
            let mut rules = pun_phpstan::Array::new();
            for (k, v) in strict {
                rules.set(pun_phpstan::Key::from(*k), pun_phpstan::Value::Bool(*v));
            }
            p.parameters.set(
                pun_phpstan::Key::from("strictRules"),
                pun_phpstan::Value::Array(rules),
            );
            p.ignores =
                vec!["[[analyse.ignore]]\nrule = \"undefined/method\"\nmessage = \"#x#\"\n".into()];
            // As `pun migrate` does: [extensions] from composer.lock first, then [analyse] from phpstan.neon.
            let mut parts = Parts {
                format: format.into(),
                extensions: crate::migrate::extensions(&crate::composer::detect(&d).unwrap()),
                ..Parts::default()
            };
            let done = compose(&d, &mut parts, &p).unwrap();
            (done, parts, d)
        };
    let at = |level: u8, ext: &[&str], strict: &[(&str, bool)]| {
        at_in(level, ext, strict, &[("composer.json", "{}")], "")
    };
    let lines = |parts: &Parts, v: &str| -> Vec<String> {
        (parts.analyse_rules.iter())
            .filter(|(_, x)| x == v)
            .map(|(k, _)| k.clone())
            .collect()
    };
    let (done, parts, d) = at(5, &[], &[]);
    assert_eq!(done.preset, AnalysePreset::Recommended);
    let off = lines(&parts, "\"off\"");
    assert!(off.contains(&"deprecated/class".to_string()), "{off:?}");
    assert!(
        off.contains(&"classes/uninitialized-property".to_string()),
        "{off:?}"
    );
    assert!(
        off.contains(&"types/printf-argument".to_string()),
        "{off:?}"
    );
    assert!(
        !off.contains(&"types/random-int-range".to_string()),
        "{off:?}"
    );
    assert_eq!((done.on, done.off), (0, off.len()));
    let cfg = crate::from_toml(&d, &parts.render()).unwrap();
    assert_eq!(cfg.analyse.ignore.len(), 1);
    assert_eq!(
        cfg.analyse.ignore[0].rule.as_deref(),
        Some("analyse/undefined/method")
    );
    let has = |id: &str| cfg.analyse.rules.iter().any(|r| r.id == id);
    assert!(!has("analyse/deprecated/class") && has("analyse/deprecated/to-string"));

    let (done, parts, _) = at(4, &[], &[]);
    assert_eq!(done.preset, AnalysePreset::Recommended);
    assert!(lines(&parts, "\"off\"").contains(&"types/random-int-range".to_string()));

    let strict = "phpstan/phpstan-strict-rules";
    let params = [
        ("booleansInConditions", true),
        ("disallowedEmpty", true),
        ("uselessCast", false),
    ];
    let (done, parts, _) = at(0, &[strict], &params);
    assert_eq!(done.preset, AnalysePreset::Basic);
    let on = lines(&parts, "\"on\"");
    assert!(
        on.contains(&"operators/non-boolean-and-operand".to_string()),
        "{on:?}"
    );
    assert!(!(parts.lint_rules.iter()).any(|(k, _)| k == "dead-code/no-useless-cast"));
    assert!(
        !on.contains(&"control-flow/non-boolean-while".to_string()),
        "needs bleedingEdge too"
    );
    assert_eq!(done.lint, 2);
    for rule in ["style/no-empty", "dead-code/no-duplicate-array-key"] {
        assert!(
            parts
                .lint_rules
                .contains(&(rule.into(), "\"error\"".into()))
        );
    }
    let (done, parts, _) = at_in(5, &[], &[], &[("composer.lock", super::LARAVEL_LOCK)], "");
    assert!((parts.analyse_rules.iter()).any(|(k, v)| k == "laravel" && v == "\"off\""));
    assert!(
        !parts
            .analyse_rules
            .iter()
            .any(|(k, _)| k.starts_with("laravel/"))
    );
    assert_eq!(done.off, lines(&parts, "\"off\"").len());

    let backtick = [("disallowedBacktick", true)];
    let format_rule = (
        "format/functions/backtick-to-shell-exec".to_string(),
        "a PHPStan check that is this format rule in Pun; turn it on in [format.rules]".to_string(),
    );
    let (done, ..) = at_in(0, &[strict], &backtick, &[], "preset = \"empty\"\n");
    let format = |id: &str| (id.to_string(), format_rule.1.clone());
    assert_eq!(
        done.unsupported,
        [
            format("format/casts/short-scalar"),
            format_rule.clone(),
            format("format/whitespace/encoding")
        ]
    );
}

#[test]
fn identifiers_and_groups() {
    use crate::rule_of_identifier;
    assert_eq!(
        rule_of_identifier("function.notFound"),
        Some("analyse/undefined/function")
    );
    assert_eq!(
        rule_of_identifier("greater.alwaysTrue"),
        Some("analyse/dead-code/number-comparison-always-constant")
    );
    assert_eq!(
        rule_of_identifier("larastan.configCollection"),
        Some("analyse/laravel/config-collection")
    );
    assert_eq!(
        rule_of_identifier("whitespace.bom"),
        Some("format/whitespace/encoding")
    );
    assert_eq!(rule_of_identifier("no.such.identifier"), None);
    assert_eq!(rule_of_identifier(".alwaysTrue"), None);
    assert!(crate::groups().iter().any(|g| g == "dead-code"));
    assert!(!crate::groups().iter().any(|g| g == "dead"));
}

#[test]
fn no_level_runs_no_rules() {
    let d = project(
        "parameters:\n    paths: [app]\n",
        &[("composer.json", "{}")],
    );
    let p = from_phpstan(&d, &d.join("phpstan.neon")).unwrap();
    assert!(p.no_rules);
    let mut parts = Parts::default();
    let done = compose(&d, &mut parts, &p).unwrap();
    assert!(done.off > 10 && done.on == 0, "{done:?}");
    std::fs::write(d.join("pun.toml"), parts.render()).unwrap();
    let cfg = load(&d, &Options::default()).unwrap();
    assert_eq!(cfg.analyse.rules, [] as [schema::AnalyseRule; 0]);
    let d = project("parameters:\n    level: 0\n", &[]);
    assert!(!from_phpstan(&d, &d.join("phpstan.neon")).unwrap().no_rules);
}
