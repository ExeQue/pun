use anyhow::{Result, bail};
use pun_config::Rule;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pun_tokens::Tokens;

mod fixer_annotation;
mod php_grammar;
mod php_parser;

pub trait Fixer: Send + Sync {
    fn name(&self) -> &'static str;
    fn priority(&self) -> i32;
    fn is_risky(&self) -> bool {
        false
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool;
    fn supports(&self, _file: &Path) -> bool {
        true
    }
    fn configure(&mut self, options: &Value) -> Result<()> {
        if options.is_null() {
            Ok(())
        } else {
            bail!("[{}] Is not configurable.", self.name())
        }
    }
    fn fix(&self, file: &Path, tokens: &mut Tokens);
    fn apply(&self, file: &Path, tokens: &mut Tokens) {
        if tokens.count() > 0 && self.supports(file) && self.is_candidate(tokens) {
            self.fix(file, tokens);
        }
    }
    fn set_whitespaces_config(&mut self, _config: &WhitespacesFixerConfig) {}
    fn set_preset(&mut self, _preset: &str) {}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhitespacesFixerConfig {
    indent: String,
    line_ending: String,
}

impl Default for WhitespacesFixerConfig {
    fn default() -> Self {
        WhitespacesFixerConfig {
            indent: "    ".into(),
            line_ending: "\n".into(),
        }
    }
}

impl WhitespacesFixerConfig {
    pub fn new(indent: &str, line_ending: &str) -> Result<Self> {
        if !["  ", "    ", "\t"].contains(&indent) {
            bail!("Invalid \"indent\" param, expected tab or two or four spaces.");
        }
        if !["\n", "\r\n"].contains(&line_ending) {
            bail!("Invalid \"lineEnding\" param, expected \"\\n\" or \"\\r\\n\".");
        }
        Ok(WhitespacesFixerConfig {
            indent: indent.into(),
            line_ending: line_ending.into(),
        })
    }

    pub fn get_indent(&self) -> &str {
        &self.indent
    }

    pub fn get_line_ending(&self) -> &str {
        &self.line_ending
    }
}

pub(crate) const DEPRECATED: &[(&str, &[&str])] = &[
    (
        "braces",
        &[
            "single_space_around_construct",
            "control_structure_braces",
            "control_structure_continuation_position",
            "declare_parentheses",
            "no_multiple_statements_per_line",
            "braces_position",
            "statement_indentation",
            "no_extra_blank_lines",
        ],
    ),
    ("class_keyword_remove", &[]),
    (
        "compact_nullable_typehint",
        &["compact_nullable_type_declaration"],
    ),
    ("curly_braces_position", &["braces_position"]),
    (
        "escape_implicit_backslashes",
        &["string_implicit_backslashes"],
    ),
    ("function_typehint_space", &["type_declaration_spaces"]),
    (
        "native_function_type_declaration_casing",
        &["native_type_declaration_casing"],
    ),
    ("new_with_braces", &["new_with_parentheses"]),
    (
        "no_blank_lines_before_namespace",
        &["blank_lines_before_namespace"],
    ),
    (
        "no_spaces_inside_parenthesis",
        &["spaces_inside_parentheses"],
    ),
    (
        "no_trailing_comma_in_list_call",
        &["no_trailing_comma_in_singleline"],
    ),
    (
        "no_trailing_comma_in_singleline_array",
        &["no_trailing_comma_in_singleline"],
    ),
    (
        "no_trailing_comma_in_singleline_function_call",
        &["no_trailing_comma_in_singleline"],
    ),
    ("no_unneeded_curly_braces", &["no_unneeded_braces"]),
    ("phpdoc_types_no_duplicates", &["phpdoc_no_duplicate_types"]),
    (
        "single_blank_line_before_namespace",
        &["blank_lines_before_namespace"],
    ),
    (
        "single_space_after_construct",
        &["single_space_around_construct"],
    ),
    ("visibility_required", &["modifier_keywords"]),
];

struct Proxy {
    name: &'static str,
    inner: Box<dyn Fixer>,
}

impl Fixer for Proxy {
    fn name(&self) -> &'static str {
        self.name
    }
    fn priority(&self) -> i32 {
        self.inner.priority()
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        self.inner.is_candidate(tokens)
    }
    fn supports(&self, file: &Path) -> bool {
        self.inner.supports(file)
    }
    fn fix(&self, file: &Path, tokens: &mut Tokens) {
        self.inner.fix(file, tokens);
    }
}

const BLADE_UNSAFE: &[&str] = &[
    "declare_strict_types",
    "fully_qualified_strict_types",
    "global_namespace_import",
    "no_unused_imports",
];

struct SkipBladeFiles(Box<dyn Fixer>);

impl Fixer for SkipBladeFiles {
    fn name(&self) -> &'static str {
        self.0.name()
    }
    fn priority(&self) -> i32 {
        self.0.priority()
    }
    fn is_risky(&self) -> bool {
        self.0.is_risky()
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        self.0.is_candidate(tokens)
    }
    fn supports(&self, file: &Path) -> bool {
        !file
            .file_name()
            .is_some_and(|n| n.as_encoded_bytes().ends_with(b".blade.php"))
            && self.0.supports(file)
    }
    fn fix(&self, file: &Path, tokens: &mut Tokens) {
        self.0.fix(file, tokens);
    }
}

pub struct Pipeline {
    fixers: Vec<Box<dyn Fixer>>,
    pub not_ported: Vec<String>,
    profile: Option<Mutex<BTreeMap<&'static str, (Duration, u64)>>>,
    ignore_mismatched_rules: bool,
}

#[derive(Debug)]
pub struct Fixed {
    pub code: Vec<u8>,
    pub applied: Vec<&'static str>,
}

impl Pipeline {
    pub fn new(rules: &[Rule], factory: &dyn Fn() -> Vec<Box<dyn Fixer>>) -> Result<Self> {
        let mut fixers: Vec<Box<dyn Fixer>> = Vec::new();
        let mut not_ported = Vec::new();
        let mut pool = factory();
        for rule in rules {
            let mut target = rule.name.as_str();
            if let Some((_, successors)) = DEPRECATED.iter().find(|(n, _)| *n == target) {
                if target != "visibility_required" {
                    let hint = match successors {
                        [] => "Remove it; it has no replacement.".to_string(),
                        s => format!(
                            "Use {} instead.",
                            s.iter()
                                .map(|s| format!("\"{s}\""))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    };
                    bail!(
                        "Rule \"{target}\" is deprecated in PHP-CS-Fixer and not supported by Pun. {hint}"
                    );
                }
                target = successors[0];
            }
            let found = match pool.iter().position(|f| f.name() == target) {
                Some(i) => Some(pool.swap_remove(i)),
                None => factory().into_iter().find(|f| f.name() == target),
            };
            match found {
                Some(mut f) => {
                    f.set_whitespaces_config(&WhitespacesFixerConfig::default());
                    f.configure(&rule.options)?;
                    if target != rule.name {
                        f = Box::new(Proxy {
                            name: "visibility_required",
                            inner: f,
                        });
                    }
                    if BLADE_UNSAFE.contains(&target) {
                        f = Box::new(SkipBladeFiles(f));
                    }
                    fixers.push(f);
                }
                None => not_ported.push(rule.name.clone()),
            }
        }
        fixers.sort_by(|a, b| {
            b.priority()
                .cmp(&a.priority())
                .then_with(|| a.name().as_bytes().cmp(b.name().as_bytes()))
        });
        Ok(Pipeline {
            fixers,
            not_ported,
            profile: None,
            ignore_mismatched_rules: false,
        })
    }

    pub fn set_preset(&mut self, preset: &str) {
        for f in &mut self.fixers {
            f.set_preset(preset);
        }
    }

    pub fn ignore_mismatched_rules(&mut self, ignore: bool) {
        self.ignore_mismatched_rules = ignore;
    }

    pub fn enable_profile(&mut self) {
        self.profile = Some(Mutex::default());
    }

    pub fn profile_report(&self) -> String {
        let Some(p) = &self.profile else {
            return String::new();
        };
        let mut rows: Vec<_> = p.lock().unwrap().iter().map(|(k, v)| (*k, *v)).collect();
        rows.sort_by(|a, b| b.1.0.cmp(&a.1.0).then(a.0.cmp(b.0)));
        rows.iter()
            .map(|(k, (d, n))| format!("{:9.3}s {n:7} {k}\n", d.as_secs_f64()))
            .collect()
    }

    fn timed<T>(&self, stage: &'static str, f: impl FnOnce() -> T) -> T {
        let Some(p) = &self.profile else { return f() };
        let t = Instant::now();
        let out = f();
        let e = t.elapsed();
        let mut m = p.lock().unwrap();
        let slot = m.entry(stage).or_default();
        slot.0 += e;
        slot.1 += 1;
        out
    }

    pub fn fixers(&self) -> impl Iterator<Item = &dyn Fixer> {
        self.fixers.iter().map(std::convert::AsRef::as_ref)
    }

    pub fn run(&self, file: &Path, code: &[u8]) -> Result<Option<Fixed>, LintError> {
        self.timed("(lint)", || lint(code))?;
        let mut tokens = self.timed("(tokenize)", || Tokens::from_code(code));
        let ignored = has_annotation(code)
            .then(|| fixer_annotation::find(&tokens).map(|a| fixer_annotation::skipped(&a)));
        let ignored = ignored.and_then(Result::ok).unwrap_or_default();
        let mut applied = Vec::new();
        for f in &self.fixers {
            if ignored.iter().any(|n| n == f.name().as_bytes()) {
                continue;
            }
            self.timed(f.name(), || f.apply(file, &mut tokens));
            if tokens.is_changed() {
                self.timed("(clear_empty_tokens)", || tokens.clear_empty_tokens());
                tokens.clear_changed();
                applied.push(f.name());
            }
        }
        if applied.is_empty() {
            return Ok(None);
        }
        let new = self.timed("(generate_code)", || tokens.generate_code());
        if new == code {
            return Ok(None);
        }
        self.timed("(lint after fix)", || lint(&new))
            .map_err(|e| LintError {
                after_fix: true,
                ..e
            })?;
        Ok(Some(Fixed { code: new, applied }))
    }
}

impl Pipeline {
    pub fn check_annotations(&self, file: &Path, code: &[u8]) -> Result<()> {
        if !has_annotation(code) {
            return Ok(());
        }
        let path = file.display();
        let annotations = match fixer_annotation::find(&Tokens::from_code(code)) {
            Ok(a) => a,
            Err(fixer_annotation::FindError::Duplicated(message)) => {
                let path = path.to_string();
                return Err(AnnotationError::Analysing { path, message }.into());
            }
            Err(fixer_annotation::FindError::UndefinedArrayKey) => {
                return Err(AnnotationError::UndefinedArrayKey.into());
            }
        };
        if self.ignore_mismatched_rules {
            return Ok(());
        }
        let used = |n: &[u8]| {
            self.fixers.iter().any(|f| f.name().as_bytes() == n)
                || self.not_ported.iter().any(|r| r.as_bytes() == n)
        };
        let missing: Vec<String> = fixer_annotation::ignored(&annotations)
            .iter()
            .filter(|n| !used(n))
            .map(|n| match n.first() {
                None => "- (no name provided)".to_string(),
                Some(b'@') => format!(
                    "- {} (can exclude only rules, not sets)",
                    String::from_utf8_lossy(n)
                ),
                Some(_) => format!("- {} (unknown rule)", String::from_utf8_lossy(n)),
            })
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        Err(AnnotationError::NotEnabled(format!(
            "@php-cs-fixer-ignore annotation(s) used for rules that are not in the current set of enabled rules:\n{}\n\n\
             Please check your annotation(s) usage in {path} to ensure that these rules are included, or update your \
             annotation(s) usage if they have been replaced by other rules in the version of PHP CS Fixer you are using.",
            missing.join("\n")
        ))
        .into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnnotationError {
    NotEnabled(String),
    Analysing { path: String, message: String },
    UndefinedArrayKey,
}

impl std::fmt::Display for AnnotationError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            AnnotationError::NotEnabled(message) => f.write_str(message),
            AnnotationError::Analysing { path, message } => {
                write!(f, "Error while analysing file \"{path}\": {message}")
            }
            AnnotationError::UndefinedArrayKey => f.write_str("Undefined array key 1"),
        }
    }
}

impl std::error::Error for AnnotationError {}

fn has_annotation(code: &[u8]) -> bool {
    code.windows(13)
        .any(|w| w == b"php-cs-fixer-" || w == b"pun-ignore-fi")
}

#[derive(Debug, Clone)]
pub struct LintError {
    pub message: Vec<u8>,
    pub line: u32,
    pub after_fix: bool,
}

pub fn lint(code: &[u8]) -> Result<(), LintError> {
    php_parser::parse(code).map_err(|e| LintError {
        message: [
            if e.fatal {
                &b"Fatal error: "[..]
            } else {
                b"Parse error: "
            },
            &e.message,
            format!(" on line {}.", e.line).as_bytes(),
        ]
        .concat(),
        line: e.line,
        after_fix: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pun_tokens::{Kind, Token};

    struct F {
        name: &'static str,
        priority: i32,
        unsupported: &'static str,
        edit: fn(&mut Tokens),
        ws: Option<WhitespacesFixerConfig>,
    }
    impl Fixer for F {
        fn name(&self) -> &'static str {
            self.name
        }
        fn priority(&self) -> i32 {
            self.priority
        }
        fn is_candidate(&self, tokens: &Tokens) -> bool {
            tokens.is_token_kind_found(Kind::T_STRING)
        }
        fn supports(&self, file: &Path) -> bool {
            !file.ends_with(self.unsupported)
        }
        fn fix(&self, _: &Path, tokens: &mut Tokens) {
            assert_eq!(
                self.ws,
                Some(WhitespacesFixerConfig::default()),
                "the pipeline sets the whitespace config"
            );
            (self.edit)(tokens);
        }
        fn set_whitespaces_config(&mut self, config: &WhitespacesFixerConfig) {
            self.ws = Some(config.clone());
        }
    }
    fn f(name: &'static str, priority: i32, edit: fn(&mut Tokens)) -> Box<dyn Fixer> {
        Box::new(F {
            name,
            priority,
            unsupported: "skip.php",
            edit,
            ws: None,
        })
    }
    struct Plain;
    impl Fixer for Plain {
        fn name(&self) -> &'static str {
            "plain"
        }
        fn priority(&self) -> i32 {
            0
        }
        fn is_candidate(&self, _: &Tokens) -> bool {
            true
        }
        fn fix(&self, _: &Path, _: &mut Tokens) {}
    }
    struct ShowPreset(String);
    impl Fixer for ShowPreset {
        fn name(&self) -> &'static str {
            "show_preset"
        }
        fn priority(&self) -> i32 {
            0
        }
        fn is_candidate(&self, _: &Tokens) -> bool {
            true
        }
        fn fix(&self, _: &Path, t: &mut Tokens) {
            let i = (0..t.count())
                .find(|&i| t[i].is_kind(Kind::T_STRING))
                .unwrap();
            t.set(i, Token::new(Kind::T_STRING, self.0.as_str()));
        }
        fn set_preset(&mut self, preset: &str) {
            self.0 = preset.into();
        }
    }

    #[test]
    fn set_preset_reaches_every_fixer() {
        let all = || -> Vec<Box<dyn Fixer>> {
            vec![Box::new(ShowPreset("laravel".into())), Box::new(Plain)]
        };
        let mut p = Pipeline::new(&[rule("show_preset"), rule("plain")], &all).unwrap();
        p.set_preset("psr12");
        let fixed = p.run(Path::new("a.php"), b"<?php foo;").unwrap().unwrap();
        assert_eq!(fixed.code, b"<?php psr12;");
    }

    fn rule(n: &str) -> Rule {
        Rule {
            name: n.into(),
            options: Value::Null,
        }
    }
    fn upper(t: &mut Tokens) {
        for i in 0..t.count() {
            if t[i].is_kind(Kind::T_STRING) {
                let c = t[i].get_content().to_ascii_uppercase();
                t.set(i, Token::new(Kind::T_STRING, c));
            }
        }
    }
    fn drop_space(t: &mut Tokens) {
        if let Some(i) =
            (0..t.count()).find(|&i| t[i].is_kind(Kind::T_WHITESPACE) && t[i].get_content() == b" ")
        {
            t.clear_at(i);
        }
    }
    fn round_trip(t: &mut Tokens) {
        let i = (0..t.count())
            .find(|&i| t[i].is_kind(Kind::T_STRING))
            .unwrap();
        let old = t[i].clone();
        t.set(i, Token::new(Kind::T_STRING, "tmp"));
        t.set(i, old);
    }
    fn none(_: &mut Tokens) {}
    fn factory() -> Vec<Box<dyn Fixer>> {
        vec![
            f("z", 1, none),
            f("upper", 5, upper),
            f("drop_space", 5, drop_space),
            f("round_trip", 9, round_trip),
            f("unused", 9, none),
            Box::new(Plain),
        ]
    }

    #[test]
    fn import_fixers_skip_blade_files() {
        let all = || {
            vec![
                f("declare_strict_types", 2, drop_space),
                f("upper", 1, upper),
            ]
        };
        let p = Pipeline::new(&[rule("declare_strict_types"), rule("upper")], &all).unwrap();
        let skip = p.fixers().next().unwrap();
        assert_eq!(
            (skip.name(), skip.priority(), skip.is_risky()),
            ("declare_strict_types", 2, false)
        );
        let run = |file: &str| {
            p.run(Path::new(file), b"<?php foo; bar;")
                .unwrap()
                .map(|f| f.code)
        };
        assert_eq!(run("dir/v.blade.php").unwrap(), b"<?php FOO; BAR;");
        assert_eq!(run("dir/v.php").unwrap(), b"<?php FOO;BAR;");
        assert!(run("dir/skip.php").is_none());
    }

    #[test]
    fn sorts_like_php_cs_fixer() {
        let p = Pipeline::new(
            &[
                rule("upper"),
                rule("drop_space"),
                rule("z"),
                rule("missing"),
                rule("round_trip"),
                rule("plain"),
            ],
            &factory,
        )
        .unwrap();
        assert_eq!(
            p.fixers().map(super::Fixer::name).collect::<Vec<_>>(),
            ["round_trip", "drop_space", "upper", "z", "plain"]
        );
        assert_eq!(p.not_ported, ["missing"]);
        assert!(!Plain.is_risky() && Plain.supports(Path::new("x.php")));
        assert_eq!(
            Plain
                .configure(&serde_json::json!({"a": 1}))
                .unwrap_err()
                .to_string(),
            "[plain] Is not configurable."
        );
        let plain = Pipeline::new(&[rule("plain")], &|| vec![Box::new(Plain)]).unwrap();
        assert!(
            plain
                .run(Path::new("x.php"), b"<?php foo();")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn runs_like_runner_fix_file() {
        let p = Pipeline::new(
            &[
                rule("upper"),
                rule("drop_space"),
                rule("round_trip"),
                rule("z"),
            ],
            &factory,
        )
        .unwrap();
        let fixed = p
            .run(Path::new("a.php"), b"<?php foo (bar);\nbaz();\n")
            .unwrap()
            .unwrap();
        assert_eq!(
            String::from_utf8(fixed.code).unwrap(),
            "<?php FOO(BAR);\nBAZ();\n"
        );
        assert_eq!(fixed.applied, ["round_trip", "drop_space", "upper"]);
        let only_round_trip = Pipeline::new(&[rule("round_trip")], &factory).unwrap();
        assert!(
            only_round_trip
                .run(Path::new("a.php"), b"<?php foo();")
                .unwrap()
                .is_none()
        );
        assert!(
            p.run(Path::new("a.php"), b"<?php\n$a = 1;\n")
                .unwrap()
                .is_none()
        );
        let only_upper = Pipeline::new(&[rule("upper")], &factory).unwrap();
        assert!(
            only_upper
                .run(Path::new("dir/skip.php"), b"<?php foo();")
                .unwrap()
                .is_none()
        );
        let e = p
            .run(Path::new("a.php"), b"<?php\nfunction (\n")
            .unwrap_err();
        assert!(!e.after_fix);
        assert_eq!(e.message, b"Parse error: Unclosed '(' on line 2 on line 3.");
        let only_drop = Pipeline::new(&[rule("drop_space")], &factory).unwrap();
        let e = only_drop
            .run(Path::new("a.php"), b"<?php\nfunction a() {}")
            .unwrap_err();
        assert!(e.after_fix);
        assert_eq!(
            e.message,
            b"Parse error: syntax error, unexpected token \"{\" on line 2."
        );
    }

    #[test]
    fn profile_counts_stages() {
        let mut p = Pipeline::new(&[rule("upper"), rule("drop_space")], &factory).unwrap();
        assert_eq!(p.profile_report(), "");
        p.enable_profile();
        p.run(Path::new("a.php"), b"<?php foo (bar);").unwrap();
        p.run(Path::new("b.php"), b"<?php $a;").unwrap();
        let calls: BTreeMap<String, u64> = p
            .profile_report()
            .lines()
            .map(|l| {
                let (_secs, rest) = l.split_once("s ").unwrap();
                let (n, stage) = rest.trim_start().split_once(' ').unwrap();
                (stage.to_string(), n.parse().unwrap())
            })
            .collect();
        let expected = [
            ("(clear_empty_tokens)", 2),
            ("(generate_code)", 1),
            ("(lint after fix)", 1),
            ("(lint)", 2),
            ("(tokenize)", 2),
            ("drop_space", 2),
            ("upper", 2),
        ];
        assert_eq!(
            calls,
            expected.iter().map(|(k, v)| (k.to_string(), *v)).collect()
        );
    }

    #[test]
    fn fixer_annotations() {
        use fixer_annotation::{FindError, find};
        let found = |code: &[u8]| find(&Tokens::from_code(code));
        let strs = |v: &[&str]| v.iter().map(|s| s.as_bytes().to_vec()).collect::<Vec<_>>();
        let a = found(b"<?php\ndeclare(strict_types=1);\n/* @php-cs-fixer-ignore b,a */\n; // @php-cs-fixer-foo x\nfoo();\n// @php-cs-fixer-ignore c\n/**\n * @php-cs-fixer-ignore d,@Set\n */\n?>\n").unwrap();
        assert_eq!(
            a,
            vec![
                (
                    b"php-cs-fixer-ignore".to_vec(),
                    strs(&["@Set", "a", "b", "c", "d"])
                ),
                (b"php-cs-fixer-foo".to_vec(), strs(&["x"]))
            ]
        );
        assert_eq!(
            fixer_annotation::ignored(
                &found("<?php\n// @php-cs-fixer-ignore ét\n$a;\n".as_bytes()).unwrap()
            ),
            strs(&["ét"])
        );
        assert_eq!(
            found(
                b"<?= 1 // \xff @php-cs-fixer-ignore x\n;\n// @php-cs-fixer-ignore \xc3\xa9t\xe9\n"
            )
            .unwrap(),
            vec![]
        );
        assert_eq!(
            found(b"<?php\n// @php-cs-fixer-ignore\tx\n").unwrap_err(),
            FindError::UndefinedArrayKey
        );
        let dup = |m: &str| {
            FindError::Duplicated(format!(
                "Duplicated values found for annotation \"@php-cs-fixer-ignore\": {m}."
            ))
        };
        assert_eq!(
            found(b"<?php\n// @php-cs-fixer-ignore a,b,b,a\n").unwrap_err(),
            dup("\"a\", \"b\"")
        );
        assert_eq!(
            found(b"<?php\n// @php-cs-fixer-ignore a\n// @php-cs-fixer-ignore b,a\n").unwrap_err(),
            dup("\"a\"")
        );

        let a = found(b"<?php\n// @pun-ignore-file format/strings/single-quote,style/no-alias-function,nope,format.single-quote\n// @php-cs-fixer-ignore x\n").unwrap();
        assert_eq!(
            fixer_annotation::skipped(&a),
            strs(&["x", "single_quote", "no_alias_functions"])
        );
        assert_eq!(
            found(b"<?php\n// @pun-ignore-file\tformat/x/y\n").unwrap(),
            vec![]
        );

        let p =
            Pipeline::new(&[rule("upper"), rule("drop_space"), rule("gone")], &factory).unwrap();
        let code = b"<?php\n// @php-cs-fixer-ignore upper,gone\nfoo ();\n";
        assert_eq!(
            p.run(Path::new("a.php"), code).unwrap().unwrap().code,
            b"<?php\n// @php-cs-fixer-ignore upper,gone\nfoo();\n"
        );
        p.check_annotations(Path::new("a.php"), code).unwrap();
        p.check_annotations(Path::new("a.php"), b"<?php foo();")
            .unwrap();
        p.check_annotations(Path::new("a.php"), b"<?php\n// @pun-ignore-file nope\n")
            .unwrap();
        let err = |code: &[u8]| {
            p.check_annotations(Path::new("/p/b.php"), code)
                .unwrap_err()
                .to_string()
        };
        assert_eq!(
            err(b"<?php\n\n// @php-cs-fixer-ignore nope,@Foo,\n$a = \"x\";\n"),
            "@php-cs-fixer-ignore annotation(s) used for rules that are not in the current set of enabled rules:\n- (no name provided)\n\
             - @Foo (can exclude only rules, not sets)\n- nope (unknown rule)\n\nPlease check your annotation(s) usage in /p/b.php to \
             ensure that these rules are included, or update your annotation(s) usage if they have been replaced by other rules \
             in the version of PHP CS Fixer you are using."
        );
        assert_eq!(
            err(b"<?php\n// @php-cs-fixer-ignore a,b,b,a\n"),
            "Error while analysing file \"/p/b.php\": Duplicated values found for annotation \"@php-cs-fixer-ignore\": \"a\", \"b\"."
        );
        assert_eq!(
            err(b"<?php\n// @php-cs-fixer-ignore\tx\n"),
            "Undefined array key 1"
        );
        let kind = |code: &[u8]| {
            (p.check_annotations(Path::new("/p/b.php"), code)
                .unwrap_err())
            .downcast::<AnnotationError>()
            .unwrap()
        };
        assert_eq!(
            kind(b"<?php\n// @php-cs-fixer-ignore a,b,b,a\n"),
            AnnotationError::Analysing {
                path: "/p/b.php".into(),
                message:
                    "Duplicated values found for annotation \"@php-cs-fixer-ignore\": \"a\", \"b\"."
                        .into()
            }
        );
        assert_eq!(
            kind(b"<?php\n// @php-cs-fixer-ignore\tx\n"),
            AnnotationError::UndefinedArrayKey
        );
        assert!(matches!(
            kind(b"<?php\n// @php-cs-fixer-ignore nope\n"),
            AnnotationError::NotEnabled(_)
        ));
        let mut q = Pipeline::new(&[rule("upper")], &factory).unwrap();
        q.ignore_mismatched_rules(true);
        q.check_annotations(Path::new("a.php"), b"<?php\n// @php-cs-fixer-ignore nope\n")
            .unwrap();
        assert_eq!(
            q.check_annotations(Path::new("a.php"), b"<?php\n// @php-cs-fixer-ignore\tx\n")
                .unwrap_err()
                .to_string(),
            "Undefined array key 1"
        );
    }

    #[test]
    fn whitespaces_config() {
        assert_eq!(
            WhitespacesFixerConfig::new("   ", "\n")
                .unwrap_err()
                .to_string(),
            "Invalid \"indent\" param, expected tab or two or four spaces."
        );
        assert_eq!(
            WhitespacesFixerConfig::new("\t", "\r")
                .unwrap_err()
                .to_string(),
            r#"Invalid "lineEnding" param, expected "\n" or "\r\n"."#
        );
        let c = WhitespacesFixerConfig::new("\t", "\r\n").unwrap();
        assert_eq!((c.get_indent(), c.get_line_ending()), ("\t", "\r\n"));
        let d = WhitespacesFixerConfig::default();
        assert_eq!((d.get_indent(), d.get_line_ending()), ("    ", "\n"));
    }

    #[test]
    fn deprecated_rules() {
        let all = || -> Vec<Box<dyn Fixer>> {
            vec![
                f("modifier_keywords", 56, upper),
                f("new_with_parentheses", 37, none),
            ]
        };
        let err = Pipeline::new(&[rule("new_with_braces")], &all)
            .err()
            .unwrap()
            .to_string();
        assert_eq!(
            err,
            "Rule \"new_with_braces\" is deprecated in PHP-CS-Fixer and not supported by Pun. Use \"new_with_parentheses\" instead."
        );
        let err = Pipeline::new(&[rule("braces")], &all)
            .err()
            .unwrap()
            .to_string();
        assert!(err.ends_with("Use \"single_space_around_construct\", \"control_structure_braces\", \"control_structure_continuation_position\", \"declare_parentheses\", \"no_multiple_statements_per_line\", \"braces_position\", \"statement_indentation\", \"no_extra_blank_lines\" instead."), "{err}");
        assert!(
            Pipeline::new(&[rule("class_keyword_remove")], &all)
                .err()
                .unwrap()
                .to_string()
                .ends_with("Remove it; it has no replacement.")
        );
        let p = Pipeline::new(&[rule("visibility_required")], &all).unwrap();
        assert_eq!(
            p.fixers()
                .map(|f| (f.name(), f.priority()))
                .collect::<Vec<_>>(),
            [("visibility_required", 56)]
        );
        let fixed = p.run(Path::new("a.php"), b"<?php foo();").unwrap().unwrap();
        assert_eq!(
            (fixed.code, fixed.applied),
            (b"<?php FOO();".to_vec(), vec!["visibility_required"])
        );
        assert!(
            p.run(Path::new("skip.php"), b"<?php foo();")
                .unwrap()
                .is_none()
        );
        assert!(p.run(Path::new("a.php"), b"<?php $a;").unwrap().is_none());
        let p = Pipeline::new(
            &[rule("visibility_required"), rule("modifier_keywords")],
            &all,
        )
        .unwrap();
        assert_eq!(
            p.fixers().map(super::Fixer::name).collect::<Vec<_>>(),
            ["modifier_keywords", "visibility_required"]
        );
    }
}
