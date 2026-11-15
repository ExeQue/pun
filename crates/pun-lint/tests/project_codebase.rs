//! A project's codebase set with `set_project_codebase`: a file sees the symbols of the project's other files. The
//! expected code is Rector's (.oracle/rector-bin, `ReturnTypeFromStrictTypedPropertyRector` alone, PHP 8.4) for the
//! two classes in one file, tests/rector/ReturnTypeFromStrictTypedPropertyRector/Oracle/property_fetches.php.inc.

use std::path::Path;

use pun_lint::Linter;
use pun_lint::rules::AddReturnTypeFromProperty;
use pun_lint::rules::facts::{project_codebase, set_project_codebase};

const ITEM: &str = "<?php\n\nclass Item\n{\n    public string $name = '';\n}\n";
const HOLDER: &str = "<?php\n\nfinal class Holder\n{\n    private Item $item;\n\n    public function __construct()\n    {\n        $this->item = new Item();\n    }\n\n    public function name()\n    {\n        return $this->item->name;\n    }\n}\n";

fn fix() -> String {
    let linter = Linter::new(
        vec![Box::new(AddReturnTypeFromProperty)],
        pun_lint::PhpVersion::from_id(80400),
    );
    linter.fix(Path::new("holder.php"), HOLDER).unwrap().source
}

#[test]
fn a_file_sees_the_project_codebase() {
    let _lock = crate::PROJECT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let files = [
        ("item.php".to_string(), ITEM.to_string()),
        ("holder.php".to_string(), HOLDER.to_string()),
    ];
    set_project_codebase(Some(project_codebase(
        &files,
        pun_lint::PhpVersion::from_id(80400),
    )));
    let typed = fix();
    set_project_codebase(None);
    assert_eq!(
        typed,
        HOLDER.replace("public function name()", "public function name(): string")
    );
}

#[test]
fn a_deferred_scan_runs_when_a_rule_reads_facts() {
    use pun_lint::rules::NoGoto;
    use pun_lint::rules::facts::set_project_scan;
    use std::sync::atomic::{AtomicBool, Ordering};
    static SCANNED: AtomicBool = AtomicBool::new(false);
    let _lock = crate::PROJECT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_project_scan(|| {
        SCANNED.store(true, Ordering::SeqCst);
        let files = [
            ("item.php".to_string(), ITEM.to_string()),
            ("holder.php".to_string(), HOLDER.to_string()),
        ];
        project_codebase(&files, pun_lint::PhpVersion::from_id(80400))
    });
    let goto = Linter::new(vec![Box::new(NoGoto)], pun_lint::PhpVersion::from_id(80400));
    assert_eq!(
        goto.check(Path::new("holder.php"), HOLDER).unwrap(),
        [] as [pun_problem::Problem; 0]
    );
    let scanned_before = SCANNED.load(Ordering::SeqCst);
    let typed = fix();
    set_project_codebase(None);
    assert!(!scanned_before);
    assert!(SCANNED.load(Ordering::SeqCst));
    assert_eq!(
        typed,
        HOLDER.replace("public function name()", "public function name(): string")
    );
}

#[test]
fn a_changed_file_keeps_its_methods_called_on_this() {
    use pun_lint::rules::NoUnusedPrivateMethod;
    let _lock = crate::PROJECT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let changed = "<?php\n\nfinal class Views\n{\n    public function handle(): int\n    {\n        return $this->recent();\n    }\n\n    private function recent(): int\n    {\n        return 1;\n    }\n}\n";
    let scanned = changed.replace(
        "return $this->recent();",
        "$n = $this->recent();\n\n        return $n;",
    );
    let files = [("/p/views.php".to_string(), scanned)];
    set_project_codebase(Some(project_codebase(
        &files,
        pun_lint::PhpVersion::from_id(80400),
    )));
    let linter = Linter::new(
        vec![Box::new(NoUnusedPrivateMethod)],
        pun_lint::PhpVersion::from_id(80400),
    );
    let fixed = linter
        .fix(Path::new("/p/views.php"), changed)
        .unwrap()
        .source;
    set_project_codebase(None);
    assert_eq!(fixed, changed);
}

#[test]
fn autoloaded_vendor_parents_guard_overrides() {
    use pun_lint::rules::AddReturnTypeFromConstant;
    use pun_lint::rules::facts::project_codebase_autoloaded;
    let _lock = crate::PROJECT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let base = |ns: &str| {
        format!(
            "<?php\n\nnamespace {ns};\n\nclass {}\n{{\n    public function run()\n    {{\n        return 0;\n    }}\n}}\n",
            if ns == "Acme" { "Base" } else { "LocalBase" }
        )
    };
    let commands = "<?php\n\nnamespace App;\n\nfinal class FromVendor extends \\Acme\\Base\n{\n    public const OK = 1;\n\n    public function run()\n    {\n        return self::OK;\n    }\n}\n\nfinal class FromProject extends LocalBase\n{\n    public const OK = 1;\n\n    public function run()\n    {\n        return self::OK;\n    }\n}\n";
    let files = [(
        "/project/app/Commands.php".to_string(),
        commands.to_string(),
    )];
    let autoload = |class: &[u8]| match class {
        b"acme\\base" => Some((
            "/project/vendor/acme/pkg/src/Base.php".to_string(),
            base("Acme"),
        )),
        b"app\\localbase" => Some(("/project/app/LocalBase.php".to_string(), base("App"))),
        _ => None,
    };
    set_project_codebase(Some(project_codebase_autoloaded(
        &files,
        pun_lint::PhpVersion::from_id(80400),
        &autoload,
    )));
    let linter = Linter::new(
        vec![Box::new(AddReturnTypeFromConstant)],
        pun_lint::PhpVersion::from_id(80400),
    );
    let fixed = linter
        .fix(Path::new("/project/app/Commands.php"), commands)
        .unwrap()
        .source;
    set_project_codebase(None);
    let at = commands.rfind("public function run()").unwrap();
    let expected = format!(
        "{}public function run(): int{}",
        &commands[..at],
        &commands[at + 21..]
    );
    assert_eq!(fixed, expected);
}

#[test]
fn a_file_autoloaded_for_two_names_is_scanned_for_each() {
    use pun_lint::rules::AddReturnTypeFromConstant;
    use pun_lint::rules::facts::project_codebase_autoloaded;
    let _lock = crate::PROJECT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let commands = "<?php\n\nnamespace App;\n\nfinal class FromVendor extends \\Acme\\Base\n{\n    public const OK = 1;\n\n    public function run()\n    {\n        return self::OK;\n    }\n\n    public function take(\\Acme\\Missing $a, \\Acme\\Gone $b): void\n    {\n    }\n}\n";
    let base = "<?php\n\nnamespace Acme;\n\nclass Base\n{\n    public function run()\n    {\n        return 0;\n    }\n}\n";
    let files = [(
        "/project/app/Commands.php".to_string(),
        commands.to_string(),
    )];
    let autoload = |class: &[u8]| {
        matches!(class, b"acme\\base" | b"acme\\missing" | b"acme\\gone").then(|| {
            (
                "/project/vendor/acme/pkg/src/Base.php".to_string(),
                base.to_string(),
            )
        })
    };
    set_project_codebase(Some(project_codebase_autoloaded(
        &files,
        pun_lint::PhpVersion::from_id(80400),
        &autoload,
    )));
    let linter = Linter::new(
        vec![Box::new(AddReturnTypeFromConstant)],
        pun_lint::PhpVersion::from_id(80400),
    );
    let fixed = linter
        .fix(Path::new("/project/app/Commands.php"), commands)
        .unwrap()
        .source;
    set_project_codebase(None);
    assert_eq!(fixed, commands);
}
