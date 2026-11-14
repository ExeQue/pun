use std::path::{Path, PathBuf};
use std::sync::Mutex;

use pun_lint::Linter;
use pun_lint::rules::facts::{project_codebase, set_project_codebase};
use pun_lint::rules::{PreferChainedExpect, PreferToBeTrueFalse, PreferYieldDataProvider};

fn sources(dir: &Path, out: &mut Vec<(String, String)>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path: PathBuf = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "php") {
            out.push((
                path.to_string_lossy().into_owned(),
                std::fs::read_to_string(&path).unwrap(),
            ));
        }
    }
}

static PROJECT: Mutex<()> = Mutex::new(());

#[test]
fn a_later_round_leaves_the_changed_class_alone() {
    let _turn = PROJECT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let fixture = std::fs::read_to_string(
        root.join("rector/YieldDataProviderRector/Oracle/used_after_other.php.inc"),
    )
    .unwrap();
    let (input, expected) = fixture.split_once("-----\n").unwrap();
    let mut files = Vec::new();
    sources(&root.join("phpunit-stubs"), &mut files);
    files.push(("/project/tests/BodyTest.php".into(), input.into()));
    set_project_codebase(Some(project_codebase(&files, 80400)));
    let linter = Linter::new(vec![Box::new(PreferYieldDataProvider)], 80400);
    let fixed = linter
        .fix(Path::new("/project/tests/BodyTest.php"), input)
        .unwrap();
    set_project_codebase(None);
    assert_eq!(fixed.source, expected);
    assert_eq!(fixed.rounds, 1);
}

/// The first round chains the `expect()` calls of both tests, which moves the second test's closure up a line, with
/// the two arrow functions on one line, the `__LINE__` and the anonymous class in it; the second round still finds them
/// in the project's codebase, so it types `names()` and its `toBe([])` becomes `toBeEmpty()`, as Rector does in its one
/// pass with both rules (ChainExpectCallsRector with `merge_different_variables` and SimplifyToLiteralBooleanRector,
/// `.oracle/rector-ecosystem`).
#[test]
fn a_later_round_types_a_moved_closure() {
    let _turn = PROJECT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let input = "<?php

it('chains', function (): void {
    expect(1)->toBe(1);

    expect(2)->toBe(2);
});

it('chains and empties', function (): void {
    $empty = fn (): Closure => fn (): array => [];
    $where = [__LINE__, new class {}];
    expect(1)->toBe(0);
    expect(names())->toBe([]);
});

function names(): array
{
    return [];
}
";
    let expected = "<?php

it('chains', function (): void {
    expect(1)->toBe(1)
        ->and(2)->toBe(2);
});

it('chains and empties', function (): void {
    $empty = fn (): Closure => fn (): array => [];
    $where = [__LINE__, new class {}];
    expect(1)->toBe(0)
        ->and(names())->toBeEmpty();
});

function names(): array
{
    return [];
}
";
    let path = "/project/tests/ChainTest.php";
    set_project_codebase(Some(project_codebase(
        &[(path.into(), input.into())],
        80400,
    )));
    let linter = Linter::new(
        vec![
            Box::new(PreferChainedExpect::default()),
            Box::new(PreferToBeTrueFalse),
        ],
        80400,
    );
    let fixed = linter.fix(Path::new(path), input).unwrap();
    set_project_codebase(None);
    assert_eq!(fixed.source, expected);
    assert_eq!(fixed.rounds, 2);
}
