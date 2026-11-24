extern crate punphp_fixers as pun_fixers;
use pun_config::Rule;
use pun_format::Pipeline;

#[test]
fn pun_rules_run_at_priority_zero() {
    let mut pun: Vec<&str> = pun_fixers::all()
        .iter()
        .map(|f| f.name())
        .filter(|n| n.starts_with("Pun/"))
        .collect();
    assert!(pun.len() > 1, "{pun:?}");
    let rule = |name: &str| Rule {
        name: name.into(),
        options: serde_json::Value::Null,
    };
    let rules: Vec<Rule> = ["no_unused_imports"]
        .into_iter()
        .chain(pun.iter().rev().copied())
        .chain(["array_syntax"])
        .map(rule)
        .collect();
    let pipeline = Pipeline::new(&rules, &pun_fixers::all).unwrap();
    pun.sort_unstable();
    let want: Vec<&str> = ["array_syntax"]
        .into_iter()
        .chain(pun)
        .chain(["no_unused_imports"])
        .collect();
    assert_eq!(
        pipeline
            .fixers()
            .map(pun_format::Fixer::name)
            .collect::<Vec<_>>(),
        want
    );
}

#[test]
fn pun_risky_rules() {
    let mut risky: Vec<&str> = pun_fixers::all()
        .iter()
        .filter(|f| f.name().starts_with("Pun/") && f.is_risky())
        .map(|f| f.name())
        .collect();
    risky.sort_unstable();
    assert_eq!(
        risky,
        [
            "Pun/final_controller",
            "Pun/no_implicit_model_query",
            "Pun/prefer_array_spread",
            "Pun/prefer_first_class_callable",
            "Pun/use_dedicated_expectation"
        ]
    );
}

#[test]
fn indent_and_line_ending_options_reach_every_fixer() {
    let rule = |name: &str, options: serde_json::Value| Rule {
        name: name.into(),
        options,
    };
    let rules = [
        rule("indentation_type", serde_json::json!({"indent": "\t"})),
        rule("line_ending", serde_json::json!({"line_ending": "\r\n"})),
        rule("statement_indentation", serde_json::Value::Null),
    ];
    let pipeline = Pipeline::new(&rules, &pun_fixers::all).unwrap();
    let code = b"<?php\nif ($a) {\n$b = 1;\n}\n";
    let fixed = pipeline
        .run(std::path::Path::new("a.php"), code)
        .unwrap()
        .unwrap()
        .code;
    assert_eq!(
        String::from_utf8(fixed).unwrap(),
        "<?php\r\nif ($a) {\r\n\t$b = 1;\r\n}\r\n"
    );
    let bad = [rule(
        "indentation_type",
        serde_json::json!({"indent": "  x"}),
    )];
    assert!(Pipeline::new(&bad, &pun_fixers::all).is_err());
}
