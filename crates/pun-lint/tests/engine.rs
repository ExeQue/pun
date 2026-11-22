use std::path::{Path, PathBuf};

use mago_span::HasSpan;
use mago_syntax::cst::Node;
use pun_lint::problem::{Edit, Fix, Level, Safety, Span};
use pun_lint::{Context, LintError, Linter, MAX_ROUNDS, Report, Rule, rules};

#[test]
fn check_reports_each_problem_once_with_rule_path_span_and_fix() {
    let linter = Linter::new(rules::all(), pun_lint::PhpVersion::from_id(80400));
    let code = "<?php\ndeclare(strict_types=1);\n$a = !true;\n";
    let problems = linter.check(Path::new("src/a.php"), code).unwrap();
    assert_eq!(problems.len(), 1);

    let negated = &problems[0];
    assert_eq!(negated.rule, "lint/complexity/no-negated-boolean-literal");
    assert_eq!(negated.path, PathBuf::from("src/a.php"));
    assert_eq!(negated.span, Span::new(36, 41)); // `!true`
    assert_eq!(negated.level, Level::Error);
    assert_eq!(
        negated.message,
        "Use `false` instead of negating a boolean literal."
    );
    assert_eq!(negated.help, None);
    let fix = negated.fix.as_ref().unwrap();
    assert_eq!(fix.safety, Safety::Safe);
    assert_eq!(fix.edits, vec![Edit::new(Span::new(36, 41), "false")]);
}

#[test]
fn rules_below_their_php_version_do_not_run() {
    let rules = || -> Vec<Box<dyn Rule>> { vec![Box::new(ReportOnly), Box::new(NeedsPhp84)] };
    let both = vec!["lint/test/report-only", "lint/test/needs-php-8.4"];
    assert_eq!(
        Linter::new(rules(), pun_lint::PhpVersion::from_id(80400)).rule_ids(),
        both
    );
    assert_eq!(
        Linter::new(rules(), pun_lint::PhpVersion::from_id(80399)).rule_ids(),
        vec!["lint/test/report-only"]
    );
}

#[test]
fn fix_counts_rounds_and_keeps_problems_without_a_fix() {
    let rules: Vec<Box<dyn Rule>> = vec![Box::new(ReportOnly), Box::new(Unwrap)];
    let linter = Linter::new(rules, pun_lint::PhpVersion::from_id(80400));
    let code = "<?php\n$a = ((1));\ntry { } finally { }\n";
    let fixed = linter.fix(Path::new("a.php"), code).unwrap();
    assert_eq!(fixed.source, "<?php\n$a = 1;\ntry { } finally { }\n");
    assert_eq!(fixed.rounds, 2);
    assert_eq!(fixed.applied.len(), 2);
    assert_eq!(fixed.applied[0].span, Span::new(11, 16));
    assert_eq!(fixed.applied[1].span, Span::new(11, 14));
    assert_eq!(fixed.remaining.len(), 1);
    assert_eq!(fixed.remaining[0].rule, "lint/test/report-only");
}

#[test]
fn nothing_to_fix_is_zero_rounds() {
    let linter = Linter::new(rules::all(), pun_lint::PhpVersion::from_id(80400));
    let code = "<?php\ndeclare(strict_types=1);\n$a += 1;\n";
    let fixed = linter.fix(Path::new("a.php"), code).unwrap();
    assert_eq!(fixed.source, code);
    assert_eq!(fixed.rounds, 0);
    assert!(fixed.applied.is_empty() && fixed.remaining.is_empty());
}

#[test]
fn a_file_that_does_not_parse_is_refused() {
    let linter = Linter::new(rules::all(), pun_lint::PhpVersion::from_id(80400));
    let err = linter
        .check(Path::new("a.php"), "<?php\n$a = ;\n")
        .unwrap_err();
    let LintError::Syntax { span, .. } = &err else {
        panic!("{err:?}");
    };
    assert_eq!(span.start, 11);
    assert!(err.to_string().starts_with("syntax error: "), "{err}");
    assert_eq!(
        linter
            .fix(Path::new("a.php"), "<?php\n$a = ;\n")
            .unwrap_err(),
        err
    );
}

#[test]
fn a_fix_that_breaks_the_syntax_is_an_error() {
    let linter = Linter::new(
        vec![Box::new(DropSemicolons)],
        pun_lint::PhpVersion::from_id(80400),
    );
    let err = linter
        .fix(Path::new("a.php"), "<?php\n$a = 1;\n$b = 2;\n")
        .unwrap_err();
    let LintError::FixBrokeSyntax { rules, .. } = &err else {
        panic!("{err:?}");
    };
    assert_eq!(rules, &vec!["lint/test/drop-semicolons".to_string()]);
    assert!(
        err.to_string()
            .starts_with("the fixes of lint/test/drop-semicolons produced a syntax error: "),
        "{err}"
    );
}

#[test]
fn fixes_that_never_settle_stop_after_max_rounds() {
    let linter = Linter::new(
        vec![Box::new(AlwaysAppend)],
        pun_lint::PhpVersion::from_id(80400),
    );
    let err = linter.fix(Path::new("a.php"), "<?php\n").unwrap_err();
    assert_eq!(
        err,
        LintError::NoConvergence {
            rules: vec!["lint/test/always-append".to_string()]
        }
    );
    assert_eq!(
        err.to_string(),
        format!("fixes did not settle after {MAX_ROUNDS} rounds (lint/test/always-append)")
    );
}

#[test]
fn a_panicking_rule_fails_its_file_only() {
    let linter = Linter::new(vec![Box::new(Panics)], pun_lint::PhpVersion::from_id(80400));
    for (code, message) in [
        ("<?php\ntry { } finally { }\n", "no finally"),
        ("<?php\nbreak;\n", "line 2"),
    ] {
        let err = linter.check(Path::new("a.php"), code).unwrap_err();
        assert_eq!(
            err,
            LintError::Panicked {
                message: message.to_string()
            }
        );
        assert_eq!(err.to_string(), format!("a lint rule panicked: {message}"));
    }
    let err = linter.check(Path::new("a.php"), "<?php\ncontinue;\n");
    assert_eq!(err.unwrap_err().to_string(), "a lint rule panicked: ");
    assert_eq!(
        linter.check(Path::new("b.php"), "<?php\n$a = 1;\n"),
        Ok(vec![])
    );
}

struct Panics;

impl Rule for Panics {
    fn id(&self) -> &'static str {
        "lint/test/panics"
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _ancestors: &[Node<'ast, 'arena>],
        _ctx: &Context<'_>,
        _out: &mut Vec<Report>,
    ) {
        match node {
            Node::Try(_) => panic!("no finally"),
            Node::Break(_) => panic!("line {}", 2),
            Node::Continue(_) => std::panic::panic_any(7),
            _ => {}
        }
    }
}

struct ReportOnly;

impl Rule for ReportOnly {
    fn id(&self) -> &'static str {
        "lint/test/report-only"
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        ancestors: &[Node<'ast, 'arena>],
        _: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if let (Node::Try(t), [Node::Program(_), Node::Statement(_)]) = (node, ancestors) {
            out.push(Report {
                span: pun_lint::syntax::node_span(t),
                message: "try".into(),
                help: None,
                fix: None,
            });
        }
    }
}

struct NeedsPhp84;

impl Rule for NeedsPhp84 {
    fn id(&self) -> &'static str {
        "lint/test/needs-php-8.4"
    }

    fn min_php_version(&self) -> pun_lint::PhpVersion {
        pun_lint::PhpVersion::from_id(80400)
    }

    fn visit<'ast, 'arena>(
        &self,
        _: Node<'ast, 'arena>,
        _: &[Node<'ast, 'arena>],
        _: &Context<'_>,
        _: &mut Vec<Report>,
    ) {
    }
}

struct Unwrap;

impl Rule for Unwrap {
    fn id(&self) -> &'static str {
        "lint/test/unwrap"
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if let Node::Parenthesized(p) = node {
            let span = pun_lint::syntax::node_span(p);
            let inner = pun_lint::syntax::node_span(p.expression);
            out.push(Report {
                span,
                message: "parentheses".into(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits: vec![Edit::new(span, &ctx.source[inner.start..inner.end])],
                }),
            });
        }
    }
}

struct DropSemicolons;

impl Rule for DropSemicolons {
    fn id(&self) -> &'static str {
        "lint/test/drop-semicolons"
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _: &[Node<'ast, 'arena>],
        _: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if let Node::Terminator(t) = node {
            let span = pun_lint::syntax::span(t.span());
            out.push(Report {
                span,
                message: "semicolon".into(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits: vec![Edit::new(span, "")],
                }),
            });
        }
    }
}

struct AlwaysAppend;

impl Rule for AlwaysAppend {
    fn id(&self) -> &'static str {
        "lint/test/always-append"
    }

    fn visit<'ast, 'arena>(
        &self,
        node: Node<'ast, 'arena>,
        _: &[Node<'ast, 'arena>],
        ctx: &Context<'_>,
        out: &mut Vec<Report>,
    ) {
        if let Node::Program(_) = node {
            let end = ctx.source.len();
            out.push(Report {
                span: Span::new(end, end),
                message: "append".into(),
                help: None,
                fix: Some(Fix {
                    safety: Safety::Safe,
                    edits: vec![Edit::new(Span::new(end, end), "\n")],
                }),
            });
        }
    }
}

#[test]
fn a_rule_without_options_rejects_any() {
    let mut rule = ReportOnly;
    assert_eq!(rule.configure(&serde_json::Value::Null), Ok(()));
    assert_eq!(
        rule.configure(&serde_json::json!({"a": 1}))
            .map_err(|e| e.to_string()),
        Err("takes no options".to_string())
    );
}
