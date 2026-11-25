use std::fmt::Write as _;
use std::path::Path;

use pun_analyse::{Project, run};
use serde_json::Value;

fn errors(project: &Project) -> Vec<(i64, String, String, Option<String>)> {
    run(project)
        .unwrap()
        .into_iter()
        .map(|e| (e.line, e.identifier, e.message, e.tip))
        .collect()
}

// Literals above ConstantArrayTypeBuilder::ARRAY_COUNT_LIMIT are generalized
// oversized arrays, in their descriptions and in what they are accepted by.
#[test]
fn oversized_literals_match_phpstan() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/oversized-arrays");
    let expected: Value =
        serde_json::from_slice(&std::fs::read(root.join("expected-phpstan.json")).unwrap())
            .unwrap();
    let want: Vec<_> = expected["files"]["oversized.php"]["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["line"].as_i64().unwrap(),
                e["identifier"].as_str().unwrap().to_string(),
                e["message"].as_str().unwrap().to_string(),
                e["tip"].as_str().map(str::to_string),
            )
        })
        .collect();
    let mut project = Project::new(vec![root.join("oversized.php")], Vec::new());
    project.level = 9;
    assert_eq!(errors(&project), want);
}

// Issue #61: the check of a large literal against its PHPDoc type grew with the
// cube of the row count (a 4,000-row literal took a minute).
#[test]
fn large_literal_checked_against_phpdoc_stays_cheap() {
    let mut rows = String::new();
    for i in 0..4000 {
        writeln!(rows, "['id-{i:06}', 'a', {i}, null],").unwrap();
    }
    let source = format!(
        "<?php\n\nfinal class Data\n{{\n    /** @var array<string, list<array<int, int|string|null>>> */\n    public const array TABLES = ['rows' => [\n{rows}]];\n\n    /** @return array<string, list<array<int, int|string|null>>> */\n    public static function tables(): array\n    {{\n        return [...self::TABLES];\n    }}\n}}\n"
    );
    let file = Path::new(env!("CARGO_TARGET_TMPDIR")).join("oversized-large-literal.php");
    std::fs::write(&file, source).unwrap();
    let mut project = Project::new(vec![file], Vec::new());
    project.level = 9;
    let started = std::time::Instant::now();
    assert_eq!(errors(&project), Vec::new());
    assert!(started.elapsed().as_secs() < 60, "{:?}", started.elapsed());
}
