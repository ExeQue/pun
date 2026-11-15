//! Cases beyond Rector's fixtures. Each tests/oracle/*.php.inc is an input, `-----`, and what Rector 2.8.4
//! (`.oracle/rector-bin`, `rector process` with the rule and `withPhpVersion(80400)`) wrote for it.

use std::path::Path;

use pun_lint::{Linter, Rule, rules};

fn spike_rules() -> Vec<Box<dyn Rule>> {
    vec![Box::new(rules::PreferCompoundAssignment)]
}

fn check_oracle(name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/oracle")
        .join(name);
    let content = std::fs::read_to_string(&path).unwrap();
    let (input, expected) = content.split_once("-----\n").unwrap();
    let fixed = Linter::new(spike_rules(), pun_lint::PhpVersion::from_id(80400))
        .fix(&path, input)
        .unwrap();
    assert_eq!(fixed.source, expected, "{name}");
}

#[test]
fn compound_assignment() {
    check_oracle("prefer_compound_assignment.php.inc");
}

#[test]
fn compound_assignment_keeps_parentheses_around_low_precedence_operators() {
    let linter = Linter::new(spike_rules(), pun_lint::PhpVersion::from_id(80400));
    let fixed = linter
        .fix(
            Path::new("a.php"),
            "<?php\n$a = $a + ($b or $c);\n$a = $a . (($b xor $c));\n$a = $a - ($b and $c);\n",
        )
        .unwrap();
    assert_eq!(
        fixed.source,
        "<?php\n$a += ($b or $c);\n$a .= (($b xor $c));\n$a -= ($b and $c);\n"
    );
}

#[test]
fn unused_foreach_key_with_a_missing_array_element_in_compact() {
    let code = "<?php\nforeach ($a as $k => $v) { compact([, 'k']); }\n";
    assert_eq!(fix_with(rules::NoUnusedForeachKey, code), code);
}

fn check_rule_oracle(rule: impl Rule + 'static, name: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/oracle")
        .join(name);
    let content = std::fs::read_to_string(&path).unwrap();
    let (input, expected) = content.split_once("-----\n").unwrap();
    let fixed = Linter::new(vec![Box::new(rule)], pun_lint::PhpVersion::from_id(80400))
        .fix(&path, input)
        .unwrap();
    assert_eq!(fixed.source, expected, "{name}");
}

fn fix_with(rule: impl Rule + 'static, code: &str) -> String {
    Linter::new(vec![Box::new(rule)], pun_lint::PhpVersion::from_id(80400))
        .fix(Path::new("a.php"), code)
        .unwrap()
        .source
}

#[test]
fn and_true() {
    check_rule_oracle(rules::NoAndTrue, "no_and_true.php.inc");
}

#[test]
fn and_true_keeps_parentheses_that_hold_the_meaning() {
    assert_eq!(
        fix_with(rules::NoAndTrue, "<?php\n$a = $x || true && ($b ?? $c);\n"),
        "<?php\n$a = $x || ($b ?? $c);\n"
    );
}

#[test]
fn cast_in_concat() {
    check_rule_oracle(rules::NoCastInConcat, "no_cast_in_concat.php.inc");
}

#[test]
fn cast_in_concat_keeps_parentheses_around_a_ternary() {
    assert_eq!(
        fix_with(
            rules::NoCastInConcat,
            "<?php\n$a = 'a' . (string) ($b ? 'x' : 'y');\n"
        ),
        "<?php\n$a = 'a' . ($b ? 'x' : 'y');\n"
    );
}

#[test]
fn useless_continue() {
    check_rule_oracle(rules::NoUselessContinue, "no_useless_continue.php.inc");
}

#[test]
fn empty_block() {
    check_rule_oracle(rules::NoEmptyBlock, "no_empty_block.php.inc");
}

#[test]
fn empty_block_keeps_parentheses_around_an_inverted_ternary() {
    assert_eq!(
        fix_with(
            rules::NoEmptyBlock,
            "<?php\nif ($a ? $b : $c) {} else { x(); }\n"
        ),
        "<?php\nif (!($a ? $b : $c)) {\n    x();\n}\n"
    );
}

#[test]
fn empty_block_keeps_parentheses_around_an_assignment_in_a_comparison() {
    assert_eq!(
        fix_with(
            rules::NoEmptyBlock,
            "<?php\nif (($a = 1) === $b) {} else { x(); }\n"
        ),
        "<?php\nif (($a = 1) !== $b) {\n    x();\n}\n"
    );
}

#[test]
fn useless_catch() {
    check_rule_oracle(rules::NoUselessCatch, "no_useless_catch.php.inc");
}

#[test]
fn double_self_assignment() {
    check_rule_oracle(
        rules::NoDoubleSelfAssignment,
        "no_double_self_assignment.php.inc",
    );
}

#[test]
fn duplicate_array_key() {
    check_rule_oracle(rules::NoDuplicateArrayKey, "no_duplicate_array_key.php.inc");
}

#[test]
fn null_property_init() {
    check_rule_oracle(rules::NoNullPropertyInit, "no_null_property_init.php.inc");
}

#[test]
fn null_property_init_in_parentheses() {
    assert_eq!(
        fix_with(
            rules::NoNullPropertyInit,
            "<?php\nclass A { public $h = (null); }\n"
        ),
        "<?php\nclass A { public $h; }\n"
    );
}

#[test]
fn redundant_final_const() {
    check_rule_oracle(
        rules::NoRedundantFinalConst,
        "no_redundant_final_const.php.inc",
    );
}

#[test]
fn unused_closure_variable() {
    check_rule_oracle(
        rules::NoUnusedClosureVariable,
        "no_unused_closure_variable.php.inc",
    );
}

#[test]
fn unused_foreach_key() {
    check_rule_oracle(rules::NoUnusedForeachKey, "no_unused_foreach_key.php.inc");
}

#[test]
fn return_in_constructor() {
    check_rule_oracle(
        rules::NoReturnInConstructor,
        "no_return_in_constructor.php.inc",
    );
}

#[test]
fn return_in_constructor_in_parentheses() {
    assert_eq!(
        fix_with(
            rules::NoReturnInConstructor,
            "<?php\nclass A {\n    public function __construct() {\n        return (1);\n    }\n}\n"
        ),
        "<?php\nclass A {\n    public function __construct() {\n        return;\n    }\n}\n"
    );
}

#[test]
fn empty_array_identical_on_a_parameter_written_later() {
    check_rule_oracle(
        rules::PreferEmptyArrayIdentical,
        "prefer_empty_array_identical.php.inc",
    );
}

#[test]
fn arrow_function_return_type_of_a_member_of_an_unknown_class() {
    check_rule_oracle(
        rules::AddArrowFunctionReturn,
        "add_arrow_function_return_type_unknown_class.php.inc",
    );
}

#[test]
fn statement_padding_skips_a_file_with_inline_html() {
    check_rule_oracle(
        rules::RequireStatementPadding,
        "require_statement_padding_html.php.inc",
    );
}

#[test]
fn class_keyword() {
    // PHP-CS-Fixer's `class_keyword` (.oracle/php-cs-fixer, `--allow-risky=yes`) on PHP's own classes: a quoted string
    // that is a class name spelt as declared, with or without a leading `\`. Classes only the fixer's own process
    // knows (`PhpCsFixer\...`) are not known to Pun.
    check_rule_oracle(rules::PreferClassConstant, "prefer_class_constant.php.inc");
}

#[test]
fn implicit_array_creation() {
    // phpstan-strict-rules 's `disallowedImplicitArrayCreation` (.oracle/phpstan, level 0) on the same file reports
    // exactly these lines and messages; `$t` after a `try`, `static`, `global`, `null` and a compound write stay quiet.
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/no_implicit_array_creation.php");
    let source = std::fs::read_to_string(&path).unwrap();
    let linter = Linter::new(
        vec![Box::new(rules::NoImplicitArrayCreation)],
        pun_lint::PhpVersion::from_id(80400),
    );
    let found: Vec<(usize, String)> = linter
        .check(&path, &source)
        .unwrap()
        .iter()
        .map(|p| {
            (
                source[..p.span.start].matches('\n').count() + 1,
                p.message.clone(),
            )
        })
        .collect();
    let expected = [
        (2, "variable $a does not exist."),
        (3, "variable $b might not exist."),
        (5, "variable $n does not exist."),
        (7, "variable $acc might not exist."),
        (9, "variable $w might not exist."),
        (11, "variable $top might not exist."),
    ];
    let expected: Vec<(usize, String)> = expected
        .iter()
        .map(|(l, m)| (*l, format!("Implicit array creation is not allowed - {m}")))
        .collect();
    assert_eq!(found, expected);
}

#[test]
fn file_with_only_an_opening_tag_gets_no_strict_types_declaration() {
    for rule in [
        Box::new(rules::AddSafeStrictTypes) as Box<dyn Rule>,
        Box::new(rules::RequireStrictTypesInTests),
    ] {
        let linter = Linter::new(vec![rule], pun_lint::PhpVersion::from_id(80400));
        assert_eq!(
            linter.check(Path::new("a.php"), "<?php\n").unwrap(),
            [] as [pun_problem::Problem; 0]
        );
    }
}
