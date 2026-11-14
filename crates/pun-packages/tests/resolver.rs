//! Composer's installer fixtures that run `update`, with the lock Composer 2.11.2 wrote for each
//! (`scripts/export-resolver-fixtures.py`): the resolver must lock the same packages, or fail where Composer failed,
//! and the lock it writes must be Composer's text. Composer's pool builder fixtures
//! (`scripts/export-poolbuilder-fixtures.py`): the pool must hold the packages Composer's holds.

use pun_packages::lock::Lock;
use pun_packages::resolver::filters::Filters;
use pun_packages::resolver::package::{Origin, Package, load, stability_value};
use pun_packages::resolver::pool::{
    ArrayRepository, PoolBuilder, Repository, Request, RootAliases, Stabilities, Transitive,
    filtered,
};
use pun_packages::resolver::repos::path_packages;
use pun_packages::resolver::{Options, locker, resolve};
use pun_packages::version::normalize;
use serde_json::{Map, Value};
use std::collections::HashMap;

fn options(args: &[Value]) -> Options {
    let mut o = Options::default();
    for a in args.iter().filter_map(Value::as_str) {
        match a {
            "--with-dependencies" => o.transitive = Transitive::Dependencies,
            "--with-all-dependencies" => o.transitive = Transitive::All,
            "--minimal-changes" => o.minimal_changes = true,
            "--prefer-lowest" => o.prefer_lowest = true,
            "--prefer-stable" => o.prefer_stable = true,
            "--ignore-platform-reqs" => o.ignore_all_platform = true,
            _ if a.starts_with("--ignore-platform-req=") => {
                o.ignore_platform
                    .push(a["--ignore-platform-req=".len()..].to_string());
            }
            _ if a.starts_with('-') => {}
            name => o.allow_list.push(name.to_string()),
        }
    }
    o
}

fn as_map(v: &Value) -> Map<String, Value> {
    v.as_object().cloned().unwrap_or_default()
}

const EXTRA_UPDATES: &str = r#"[
{"name": "three alternatives and a conflict", "composer": {"require": {"a/a": "*"}, "repositories": [{"type": "package", "package": [{"name": "a/a", "version": "2.0", "require": {"b/b": "<1.1"}, "conflict": {"b/b": "<1.0"}}, {"name": "b/b", "version": "1.0"}, {"name": "b/b", "version": "1.1"}, {"name": "b/b", "version": "0.9"}]}]}, "args": [], "expect": {"packages": [["a/a", "2.0"], ["b/b", "1.0"]], "packages-dev": []}},
{"name": "pick older if newer conflicts", "composer": {"require": {"x/x": "*"}, "repositories": [{"type": "package", "package": [{"name": "x/x", "version": "1.0", "require": {"a/a": ">=2.0.0.0", "b/b": ">=2.0.0.0"}}, {"name": "a/a", "version": "2.0.0", "require": {"b/b": ">=2.0.0.0"}}, {"name": "a/a", "version": "2.1.0", "require": {"b/b": ">=2.2.0.0"}}, {"name": "b/b", "version": "2.1.0"}, {"name": "s/s", "version": "2.0.0", "replace": {"a/a": ">=2.0.0.0", "b/b": ">=2.0.0.0"}}]}]}, "args": [], "expect": {"packages": [["a/a", "2.0.0"], ["b/b", "2.1.0"], ["x/x", "1.0"]], "packages-dev": []}},
{"name": "use replacer if necessary", "composer": {"require": {"a/a": "*", "d/d": "*"}, "repositories": [{"type": "package", "package": [{"name": "a/a", "version": "1.0", "require": {"b/b": ">=1.0", "c/c": ">=1.0"}}, {"name": "b/b", "version": "1.0"}, {"name": "d/d", "version": "1.0", "replace": {"b/b": ">=1.0", "c/c": ">=1.0"}}, {"name": "d/d", "version": "1.1", "replace": {"b/b": ">=1.0", "c/c": ">=1.0"}}]}]}, "args": [], "expect": {"packages": [["a/a", "1.0"], ["d/d", "1.1"]], "packages-dev": []}},
{"name": "issue 265", "composer": {"minimum-stability": "dev", "require": {"c/c": "2.0.0.0-dev"}, "repositories": [{"type": "package", "package": [{"name": "a/a", "version": "2.0.999999-dev"}, {"name": "a/a", "version": "2.1-dev"}, {"name": "a/a", "version": "2.2-dev"}, {"name": "b/b", "version": "2.0.10", "require": {"a/a": "2.1.0.0-dev"}}, {"name": "b/b", "version": "2.0.9", "require": {"a/a": "2.1.0.0-dev"}, "replace": {"d/d": "2.0.9.0"}}, {"name": "c/c", "version": "2.0-dev", "require": {"a/a": ">=2.0", "d/d": ">=2.0"}}, {"name": "d/d", "version": "2.0.9", "require": {"a/a": ">=2.1", "b/b": ">=2.0-dev"}}]}]}, "args": [], "expect": {"packages": [["a/a", "2.1-dev"], ["b/b", "2.0.10"], ["c/c", "2.0-dev"], ["d/d", "2.0.9"]], "packages-dev": []}},
{"name": "learn literals with sorted rule literals", "composer": {"require": {"symfony/twig-bridge": "*", "twig/twig": "*"}, "repositories": [{"type": "package", "package": [{"name": "twig/twig", "version": "2.0"}, {"name": "twig/twig", "version": "1.6"}, {"name": "twig/twig", "version": "1.5"}, {"name": "symfony/symfony", "version": "2.0", "replace": {"symfony/twig-bridge": "2.0"}}, {"name": "symfony/twig-bridge", "version": "2.0", "require": {"twig/twig": "<2.0"}}]}]}, "args": [], "expect": {"packages": [["symfony/twig-bridge", "2.0"], ["twig/twig", "1.6"]], "packages-dev": []}},
{"name": "learn positive literal", "composer": {"require": {"a/a": "*"}, "repositories": [{"type": "package", "package": [{"name": "a/a", "version": "1.0", "require": {"b/b": "1.0", "c/c": ">=1.0", "d/d": "1.0"}}, {"name": "b/b", "version": "1.0", "require": {"e/e": "1.0"}}, {"name": "c/c", "version": "1.0", "require": {"f/f": "1.0"}}, {"name": "c/c", "version": "2.0", "require": {"f/f": "1.0", "g/g": ">=1.0"}}, {"name": "d/d", "version": "1.0", "require": {"f/f": ">=1.0"}}, {"name": "e/e", "version": "1.0", "require": {"g/g": "<=2.0"}}, {"name": "f/f", "version": "1.0"}, {"name": "f/f", "version": "2.0"}, {"name": "g/g", "version": "1.0"}, {"name": "g/g", "version": "2.0"}, {"name": "g/g", "version": "3.0"}]}]}, "args": [], "expect": {"packages": [["a/a", "1.0"], ["b/b", "1.0"], ["c/c", "2.0"], ["d/d", "1.0"], ["e/e", "1.0"], ["f/f", "1.0"], ["g/g", "2.0"]], "packages-dev": []}},
{"name": "a package requires what it replaces", "composer": {"require": {"a/a": "*"}, "repositories": [{"type": "package", "package": [{"name": "a/a", "version": "1.0", "replace": {"b/b": "1.0"}, "require": {"b/b": "^1.0"}}, {"name": "b/b", "version": "1.0"}]}]}, "args": [], "expect": {"packages": [["a/a", "1.0"]], "packages-dev": []}},
{"name": "self.version requires of a branch alias", "composer": {"minimum-stability": "dev", "require": {"a/a": "^1.0@dev"}, "repositories": [{"type": "package", "package": [{"name": "a/a", "version": "dev-main", "require": {"b/b": "self.version"}, "extra": {"branch-alias": {"dev-main": "1.0.x-dev"}}}, {"name": "b/b", "version": "dev-main", "extra": {"branch-alias": {"dev-main": "1.0.x-dev"}}}]}]}, "args": [], "expect": {"packages": [["a/a", "dev-main"], ["b/b", "dev-main"]], "packages-dev": []}},
{"name": "a conflict with what an alias replaces", "composer": {"minimum-stability": "dev", "require": {"a/a": "*", "b/b": "1.0.x-dev"}, "repositories": [{"type": "package", "package": [{"name": "a/a", "version": "1.0", "conflict": {"c/c": "1.0.x-dev"}}, {"name": "b/b", "version": "dev-main", "replace": {"c/c": "self.version"}, "extra": {"branch-alias": {"dev-main": "1.0.x-dev"}}}, {"name": "c/c", "version": "1.0"}]}]}, "args": [], "expect": {"packages": [["a/a", "1.0"], ["b/b", "dev-main"]], "packages-dev": []}},
{"name": "unsatisfiable requires", "composer": {"require": {"a/a": "*", "b/b": "*"}, "repositories": [{"type": "package", "package": [{"name": "a/a", "version": "1.0", "require": {"c/c": "^1.0"}}, {"name": "b/b", "version": "1.0", "require": {"c/c": "^2.0"}}, {"name": "c/c", "version": "1.0"}, {"name": "c/c", "version": "2.0"}]}]}, "args": [], "expect": {"exit": 2}},
{"name": "two root requirements conflict", "composer": {"require": {"a/a": "1.0", "b/b": "*"}, "repositories": [{"type": "package", "package": [{"name": "a/a", "version": "1.0"}, {"name": "b/b", "version": "1.0", "conflict": {"a/a": "1.0"}}]}]}, "args": [], "expect": {"exit": 2}},
{"name": "backtrack across alternatives", "composer": {"require": {"a/a": "*", "b/b": "*", "c/c": "*"}, "repositories": [{"type": "package", "package": [{"name": "a/a", "version": "1.0", "require": {"d/d": "1.0"}}, {"name": "a/a", "version": "2.0", "require": {"d/d": "2.0"}}, {"name": "b/b", "version": "1.0", "require": {"e/e": "1.0"}}, {"name": "b/b", "version": "2.0", "require": {"e/e": "2.0"}}, {"name": "c/c", "version": "1.0", "require": {"d/d": "1.0", "e/e": "1.0"}}, {"name": "c/c", "version": "2.0", "require": {"d/d": "1.0", "e/e": "2.0"}}, {"name": "d/d", "version": "1.0"}, {"name": "d/d", "version": "2.0"}, {"name": "e/e", "version": "1.0"}, {"name": "e/e", "version": "2.0"}]}]}, "args": [], "expect": {"packages": [["a/a", "1.0"], ["b/b", "2.0"], ["c/c", "2.0"], ["d/d", "1.0"], ["e/e", "2.0"]], "packages-dev": []}},
{"name": "random graph: a learned rule offers a choice", "composer": {"require": {"e/e": ">=1.1", "j/j": "<2.0"}, "repositories": [{"type": "package", "package": [{"name": "i/i", "version": "1.2.0", "require": {"g/g": "^1.0", "b/b": "<2.0", "c/c": "*", "h/h": "^1.1"}, "conflict": {"e/e": ">=1.1"}}, {"name": "i/i", "version": "1.3.0", "require": {"b/b": ">=1.1"}, "provide": {"v/virt": "1.0.0"}}, {"name": "i/i", "version": "2.0.0"}, {"name": "i/i", "version": "2.1.0"}, {"name": "e/e", "version": "1.1.0", "require": {"b/b": "^1.1"}, "conflict": {"h/h": "^1.0 || ^2.0"}}, {"name": "e/e", "version": "1.2.0"}, {"name": "e/e", "version": "1.3.0"}, {"name": "e/e", "version": "2.0.0", "require": {"j/j": "^3.0", "g/g": ">=1.1", "h/h": ">=1.1", "c/c": "^1.1"}}, {"name": "e/e", "version": "3.0.0", "replace": {"i/i": "2.0.0"}}, {"name": "e/e", "version": "3.1.0"}, {"name": "h/h", "version": "1.1.0", "require": {"c/c": "^1.0 || ^2.0", "e/e": "^1.0 || ^2.0", "j/j": "^1.0 || ^2.0"}, "conflict": {"a/a": "^1.0"}}, {"name": "h/h", "version": "1.2.0", "require": {"e/e": "^3.0"}}, {"name": "h/h", "version": "1.3.0", "require": {"e/e": "<2.0"}}, {"name": "h/h", "version": "2.1.0", "require": {"b/b": "~1.2"}, "conflict": {"i/i": "^2.0"}}, {"name": "h/h", "version": "3.1.0", "require": {"c/c": "*", "i/i": "<2.0", "a/a": "^1.0 || ^2.0", "j/j": "*"}}, {"name": "j/j", "version": "1.0.0", "require": {"g/g": ">=2.0", "h/h": "^2.0", "a/a": "*", "c/c": ">=1.1"}}, {"name": "j/j", "version": "1.2.0", "require": {"a/a": ">=1.1", "b/b": "^1.0 || ^2.0", "g/g": "*", "h/h": ">=2.0"}}, {"name": "j/j", "version": "2.0.0", "require": {"i/i": "^3.0"}}, {"name": "j/j", "version": "3.1.0", "require": {"e/e": "*", "g/g": ">=1.1", "i/i": "~1.2", "a/a": "^1.1"}}, {"name": "c/c", "version": "1.0.0", "require": {"g/g": "~1.2", "a/a": "^1.0", "e/e": ">=1.1"}}, {"name": "c/c", "version": "1.1.0"}, {"name": "c/c", "version": "1.3.0", "require": {"a/a": "^1.0 || ^2.0", "i/i": ">=1.1", "h/h": "*"}, "conflict": {"g/g": "^1.0"}}, {"name": "c/c", "version": "2.1.0", "require": {"b/b": "^1.1", "a/a": "^3.0"}}, {"name": "c/c", "version": "3.0.0", "conflict": {"e/e": "~1.2"}}, {"name": "c/c", "version": "3.1.0", "require": {"a/a": "^1.0 || ^2.0", "j/j": "*"}}, {"name": "b/b", "version": "1.2.0", "conflict": {"c/c": ">=1.1"}, "require": {"v/virt": "*"}}, {"name": "b/b", "version": "1.3.0", "require": {"j/j": "<2.0", "a/a": "^3.0", "c/c": "^1.0 || ^2.0", "h/h": "^1.1"}}, {"name": "b/b", "version": "2.0.0", "require": {"h/h": "*", "a/a": "^2.0"}}, {"name": "g/g", "version": "1.1.0", "require": {"c/c": "~1.2", "a/a": "^1.0", "h/h": "^1.1"}, "conflict": {"e/e": "1.0.0"}}, {"name": "g/g", "version": "2.0.0", "require": {"h/h": "<2.0", "c/c": ">=1.1", "a/a": "*"}, "provide": {"v/virt": "1.0.0"}}, {"name": "g/g", "version": "2.1.0", "require": {"i/i": "<2.0", "h/h": "^3.0", "j/j": "^1.0", "e/e": "^1.0"}, "conflict": {"a/a": "*"}, "provide": {"v/virt": "1.0.0"}}, {"name": "g/g", "version": "3.0.0", "require": {"j/j": "^3.0"}}, {"name": "a/a", "version": "1.2.0", "require": {"i/i": "^1.1", "e/e": "~1.2", "c/c": "^1.0 || ^2.0", "h/h": "~1.2"}}, {"name": "a/a", "version": "1.3.0"}, {"name": "a/a", "version": "2.0.0", "require": {"c/c": ">=1.1", "j/j": ">=2.0", "b/b": ">=1.1", "e/e": ">=2.0", "v/virt": "*"}}, {"name": "a/a", "version": "3.0.0", "require": {"g/g": "~1.2", "b/b": "*", "j/j": "1.0.0", "c/c": ">=2.0"}}, {"name": "a/a", "version": "3.1.0", "require": {"j/j": "<2.0", "c/c": "*", "i/i": "^1.0 || ^2.0"}, "conflict": {"h/h": "^1.0 || ^2.0"}, "provide": {"v/virt": "1.0.0"}}]}]}, "args": [], "expect": {"exit": 2}},
{"name": "random graph: --prefer-lowest tries a branch's alternative", "composer": {"require": {"d/d": "^1.0"}, "repositories": [{"type": "package", "package": [{"name": "d/d", "version": "1.1.0", "require": {"b/b": "^3.0", "a/a": "^2.0"}, "provide": {"v/virt": "1.0.0"}}, {"name": "d/d", "version": "1.2.0", "require": {"v/virt": "*"}}, {"name": "d/d", "version": "2.1.0", "provide": {"v/virt": "1.0.0"}}, {"name": "a/a", "version": "1.1.0", "require": {"b/b": ">=1.1", "v/virt": "*"}, "provide": {"v/virt": "1.0.0"}}, {"name": "a/a", "version": "2.0.0", "require": {"b/b": "^1.1"}}, {"name": "a/a", "version": "3.1.0"}, {"name": "b/b", "version": "1.0.0", "conflict": {"d/d": ">=2.0"}, "provide": {"v/virt": "1.0.0"}}, {"name": "b/b", "version": "1.3.0", "provide": {"v/virt": "1.0.0"}, "require": {"v/virt": "*"}}, {"name": "b/b", "version": "2.1.0", "require": {"a/a": "1.0.0", "d/d": "^1.1"}}, {"name": "b/b", "version": "3.0.0", "require": {"d/d": "~1.2", "a/a": "<2.0", "v/virt": "*"}}]}]}, "args": ["--prefer-lowest"], "expect": {"packages": [["b/b", "1.3.0"], ["d/d", "1.2.0"]], "packages-dev": []}},
{"name": "random graph: a multi-conflict rule in a conflict", "composer": {"require": {"b/b": "<2.0", "f/f": ">=1.1"}, "repositories": [{"type": "package", "package": [{"name": "f/f", "version": "1.0.0", "require": {"g/g": "^3.0", "v/virt": "*"}}, {"name": "f/f", "version": "1.1.0"}, {"name": "f/f", "version": "1.2.0", "require": {"b/b": "1.0.0"}}, {"name": "f/f", "version": "2.0.0", "require": {"b/b": "^1.0 || ^2.0", "g/g": "^1.1"}}, {"name": "f/f", "version": "3.1.0", "conflict": {"b/b": ">=2.0"}, "replace": {"g/g": "2.0.0"}, "provide": {"v/virt": "1.0.0"}}, {"name": "b/b", "version": "1.0.0", "require": {"f/f": "^2.0", "v/virt": "*"}}, {"name": "b/b", "version": "1.1.0", "conflict": {"g/g": "^3.0"}}, {"name": "b/b", "version": "1.2.0", "require": {"f/f": "^1.0", "g/g": "^1.1"}}, {"name": "b/b", "version": "2.0.0", "require": {"f/f": "^1.1", "g/g": "<2.0"}}, {"name": "b/b", "version": "3.0.0", "require": {"f/f": "^3.0", "g/g": "^1.0 || ^2.0"}}, {"name": "b/b", "version": "3.1.0", "require": {"g/g": ">=2.0", "f/f": ">=1.1"}}, {"name": "g/g", "version": "1.3.0", "require": {"b/b": "^1.0", "v/virt": "*"}}, {"name": "g/g", "version": "2.1.0"}]}]}, "args": ["--prefer-lowest"], "expect": {"packages": [["b/b", "1.1.0"], ["f/f", "1.1.0"]], "packages-dev": []}},
{"name": "random graph: a level-1 multi-conflict while learning", "composer": {"require": {"d/d": "~1.2", "f/f": ">=1.1", "e/e": "1.0.0"}, "repositories": [{"type": "package", "package": [{"name": "f/f", "version": "1.1.0"}, {"name": "f/f", "version": "1.2.0", "require": {"d/d": "^1.0"}, "provide": {"v/virt": "1.0.0"}}, {"name": "f/f", "version": "1.3.0", "require": {"e/e": "1.0.0"}, "replace": {"d/d": "2.0.0"}}, {"name": "f/f", "version": "2.1.0", "require": {"d/d": "^1.1", "e/e": "1.0.0"}}, {"name": "f/f", "version": "3.0.0", "conflict": {"d/d": "^2.0"}, "replace": {"e/e": "1.0.0"}, "require": {"v/virt": "*"}}, {"name": "f/f", "version": "3.1.0", "replace": {"d/d": "1.0.0"}, "provide": {"v/virt": "1.0.0"}, "require": {"v/virt": "*"}}, {"name": "e/e", "version": "1.0.0", "conflict": {"f/f": "^2.0"}, "require": {"v/virt": "*"}}, {"name": "e/e", "version": "1.1.0"}, {"name": "e/e", "version": "2.0.0"}, {"name": "e/e", "version": "3.1.0", "require": {"f/f": ">=1.1"}, "conflict": {"d/d": "*"}}, {"name": "d/d", "version": "1.1.0", "require": {"e/e": "^1.0"}, "conflict": {"f/f": "^1.0"}}, {"name": "d/d", "version": "1.2.0"}, {"name": "d/d", "version": "2.1.0", "require": {"f/f": "^1.0", "e/e": "^3.0"}}]}]}, "args": [], "expect": {"packages": [["d/d", "1.2.0"], ["e/e", "1.0.0"], ["f/f", "1.2.0"]], "packages-dev": []}},
{"name": "random graph: learning reverts the request decisions", "composer": {"require": {"g/g": "~1.2"}, "repositories": [{"type": "package", "package": [{"name": "e/e", "version": "1.1.0", "require": {"d/d": "~1.2"}, "replace": {"g/g": "1.0.0"}, "provide": {"v/virt": "1.0.0"}}, {"name": "e/e", "version": "1.3.0", "require": {"g/g": "~1.2", "d/d": "<2.0"}, "provide": {"v/virt": "1.0.0"}}, {"name": "e/e", "version": "3.1.0", "require": {"d/d": "^1.1"}, "replace": {"g/g": "2.0.0"}}, {"name": "d/d", "version": "1.0.0", "conflict": {"e/e": "^2.0"}, "require": {"v/virt": "*"}}, {"name": "d/d", "version": "1.2.0", "require": {"e/e": "1.0.0"}}, {"name": "d/d", "version": "2.0.0"}, {"name": "d/d", "version": "2.1.0", "require": {"g/g": "~1.2", "e/e": "^3.0"}}, {"name": "d/d", "version": "3.0.0", "require": {"g/g": "^1.0 || ^2.0", "e/e": "^1.0 || ^2.0"}, "provide": {"v/virt": "1.0.0"}}, {"name": "g/g", "version": "1.0.0", "require": {"d/d": "*", "e/e": "^1.1", "v/virt": "*"}}, {"name": "g/g", "version": "1.1.0", "require": {"e/e": "^1.0 || ^2.0", "d/d": "^2.0"}, "provide": {"v/virt": "1.0.0"}}, {"name": "g/g", "version": "1.2.0", "require": {"d/d": ">=2.0", "v/virt": "*"}}, {"name": "g/g", "version": "1.3.0", "require": {"e/e": ">=1.1"}}, {"name": "g/g", "version": "2.0.0", "require": {"d/d": "1.0.0", "e/e": "~1.2"}}, {"name": "g/g", "version": "2.1.0"}]}]}, "args": ["--prefer-lowest"], "expect": {"packages": [["d/d", "1.0.0"], ["e/e", "1.3.0"], ["g/g", "1.3.0"]], "packages-dev": []}}
]"#;

#[test]
fn resolves_as_composer() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/resolver.json")).expect("valid fixtures");
    let platform: Vec<Map<String, Value>> = fixtures["platform"]
        .as_array()
        .unwrap()
        .iter()
        .map(as_map)
        .collect();
    let mut failures = vec![];
    // Composer's SolverTest cases, other solver paths, and random graphs that reach the solver's learning and
    // backtracking, as composer.json files with what Composer 2.11.2's `composer update --no-install` locked.
    let extra: Value = serde_json::from_str(EXTRA_UPDATES).expect("valid cases");
    let cases: Vec<&Value> = (fixtures["cases"].as_array().unwrap().iter())
        .chain(extra.as_array().unwrap())
        .collect();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let manifest = as_map(&case["composer"]);
        let repositories: Vec<Box<dyn Repository>> =
            (manifest.get("repositories").and_then(Value::as_array))
                .into_iter()
                .flatten()
                .map(|r| {
                    filtered(
                        Box::new(ArrayRepository::from_config(&as_map(r)).unwrap()),
                        &as_map(r),
                    )
                    .unwrap()
                })
                .collect();
        let lock = case.get("lock").map(|l| {
            let mut l = l.clone();
            l["content-hash"] = "".into();
            Lock::from_composer_lock(&l.to_string()).unwrap()
        });
        let options = options(case["args"].as_array().unwrap());
        let mut text = None;
        let got = match resolve(
            &manifest,
            platform.clone(),
            repositories,
            lock.as_ref(),
            &options,
        ) {
            Err(_) | Ok(Err(_)) => "fails".to_string(),
            Ok(Ok(r)) => {
                let list = |ids: &[usize]| -> Vec<Value> {
                    ids.iter()
                        .map(|&p| {
                            serde_json::json!([r.arena[p].pretty_name, r.arena[p].pretty_version])
                        })
                        .collect()
                };
                text = Some(locker::lock(&r, &manifest, &options).to_composer_lock());
                serde_json::json!({"packages": list(&r.packages), "packages-dev": list(&r.packages_dev)}).to_string()
            }
        };
        let expect = &case["expect"];
        let want = if expect.get("exit").is_some() {
            "fails".to_string()
        } else {
            serde_json::json!({"packages": expect["packages"], "packages-dev": expect["packages-dev"]}).to_string()
        };
        if got != want {
            failures.push(format!("{name}\n  got:  {got}\n  want: {want}"));
        } else if let (Some(text), Some(Value::String(written))) = (text, expect.get("text")) {
            // The content-hash covers the composer.json the export wrote, with Packagist turned off.
            let hash = |t: &str| {
                t.lines()
                    .filter(|l| !l.contains("\"content-hash\""))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            if hash(&text) != hash(written) {
                failures.push(format!(
                    "{name}: the lock text differs\n--- got\n{text}\n--- want\n{written}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// One pool entry as Composer's `PoolBuilderTest::getPackageResultSet` writes it: the fixture's `id` for a package
/// that has one, else `name-version`, `#reference`, ` (locked)` and ` (alias of ...)`.
fn pool_entry(arena: &[Package], ids: &HashMap<usize, String>, p: usize) -> String {
    let package = &arena[p];
    if let Some(id) = ids.get(&p).filter(|_| package.alias_of.is_none()) {
        return id.clone();
    }
    let mut out = format!("{}-{}", package.name, package.version);
    // An alias's source is the aliased package's, as `AliasPackage` delegates it.
    let data = arena[package.alias_of.unwrap_or(p)].data();
    if let Some(r) = (data.get("source").and_then(|s| s.get("reference")))
        .and_then(Value::as_str)
        .filter(|r| !r.is_empty())
    {
        out.push_str(&format!("#{r}"));
    }
    if package.origin == Origin::Locked {
        out.push_str(" (locked)");
    }
    if let Some(of) = package.alias_of {
        let base = ids
            .get(&of)
            .cloned()
            .unwrap_or_else(|| arena[of].version.clone());
        out.push_str(&format!(" (alias of {base})"));
    }
    out
}

const EXTRA_POOLS: &str = r#"[
 {
  "name": "a disjunctive dependency constraint loads every alternative",
  "request": {
   "require": {
    "b/b": "*",
    "c/c": "*"
   }
  },
  "repos": [
   [
    {
     "name": "a/a",
     "version": "1.0.0"
    },
    {
     "name": "a/a",
     "version": "2.0.0"
    },
    {
     "name": "a/a",
     "version": "3.0.0"
    },
    {
     "name": "b/b",
     "version": "1.0.0",
     "require": {
      "a/a": "^2.0"
     }
    },
    {
     "name": "c/c",
     "version": "1.0.0",
     "require": {
      "a/a": "^1.0 || ^2.0"
     }
    },
    {
     "name": "d/d",
     "version": "1.0.0",
     "require": {
      "a/a": "^1.0 || ^2.0"
     }
    }
   ]
  ],
  "root": {},
  "fixed": [],
  "expect": [
   "a/a-1.0.0.0",
   "a/a-2.0.0.0",
   "b/b-1.0.0.0",
   "c/c-1.0.0.0"
  ]
 },
 {
  "name": "unlocking a locked package of unacceptable stability",
  "request": {
   "require": {
    "b/b": "*"
   },
   "locked": [
    {
     "name": "a/a",
     "version": "1.0.0-beta1"
    },
    {
     "name": "b/b",
     "version": "1.0.0",
     "require": {
      "a/a": "^1.0@beta"
     }
    }
   ],
   "allowList": [
    "b/b"
   ],
   "allowTransitiveDeps": true
  },
  "repos": [
   [
    {
     "name": "a/a",
     "version": "1.0.0-beta1"
    },
    {
     "name": "a/a",
     "version": "1.0.0"
    },
    {
     "name": "b/b",
     "version": "1.0.0",
     "require": {
      "a/a": "^1.0@beta"
     }
    },
    {
     "name": "b/b",
     "version": "1.1.0",
     "require": {
      "a/a": "^1.0"
     }
    }
   ]
  ],
  "root": {},
  "fixed": [],
  "expect": [
   "a/a-1.0.0.0",
   "b/b-1.0.0.0",
   "b/b-1.1.0.0"
  ]
 },
 {
  "name": "a locked package requires a path-unlocked one",
  "request": {
   "require": {
    "root/a": "*",
    "root/b": "*"
   },
   "locked": [
    {
     "name": "root/a",
     "version": "1.0.0",
     "require": {
      "symlinked/path-pkg": "*"
     }
    },
    {
     "name": "root/b",
     "version": "1.0.0"
    },
    {
     "name": "symlinked/path-pkg",
     "version": "1.0.0",
     "dist": {
      "type": "path",
      "url": "./symlinked-path-repo",
      "reference": "abcd"
     },
     "transport-options": {}
    }
   ],
   "allowList": [
    "root/b"
   ]
  },
  "repos": [
   {
    "type": "path",
    "url": "./symlinked-path-repo"
   },
   [
    {
     "name": "root/a",
     "version": "1.0.0",
     "require": {
      "symlinked/path-pkg": "*"
     }
    },
    {
     "name": "root/b",
     "version": "1.0.0"
    },
    {
     "name": "root/b",
     "version": "1.1.0"
    },
    {
     "name": "symlinked/transitive",
     "version": "2.0.0"
    },
    {
     "name": "symlinked/transitive2",
     "version": "2.0.0"
    }
   ]
  ],
  "root": {},
  "fixed": [],
  "expect": [
   "root/a-1.0.0.0 (locked)",
   "root/b-1.0.0.0",
   "root/b-1.1.0.0",
   "symlinked/path-pkg-2.0.0.0",
   "symlinked/transitive-2.0.0.0",
   "symlinked/transitive2-2.0.0.0"
  ]
 },
 {
  "name": "a replaced root requirement stays locked without all dependencies",
  "request": {
   "require": {
    "b/b": "*",
    "c/c": "*"
   },
   "locked": [
    {
     "name": "b/b",
     "version": "1.0.0"
    },
    {
     "name": "c/c",
     "version": "1.0.0"
    }
   ],
   "allowList": [
    "b/b"
   ],
   "allowTransitiveDepsNoRootRequire": true
  },
  "repos": [
   [
    {
     "name": "b/b",
     "version": "1.0.0"
    },
    {
     "name": "b/b",
     "version": "2.0.0",
     "replace": {
      "c/c": "1.0.0"
     }
    },
    {
     "name": "c/c",
     "version": "1.0.0"
    },
    {
     "name": "c/c",
     "version": "1.1.0"
    }
   ]
  ],
  "root": {},
  "fixed": [],
  "expect": [
   "b/b-1.0.0.0",
   "b/b-2.0.0.0",
   "c/c-1.0.0.0 (locked)"
  ]
 }
]"#;

#[test]
fn pools_as_composer() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/poolbuilder.json")).expect("valid fixtures");
    let dir = std::env::temp_dir().join(format!("pun-poolbuilder-{}", std::process::id()));
    for (path, text) in fixtures["files"].as_object().unwrap() {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text.as_str().unwrap()).unwrap();
    }
    let mut failures = vec![];
    // Cases Composer's fixtures lack, with the pool Composer 2.11.2's PoolBuilder built for each.
    let extra: Value = serde_json::from_str(EXTRA_POOLS).expect("valid cases");
    let cases: Vec<&Value> = (fixtures["cases"].as_array().unwrap().iter())
        .chain(extra.as_array().unwrap())
        .collect();
    for case in &cases {
        let name = case["name"].as_str().unwrap();
        let (root, request_data) = (&case["root"], &case["request"]);
        let mut arena: Vec<Package> = vec![];
        let mut ids: HashMap<usize, String> = HashMap::new();
        let mut add = |data: &Value, origin: Origin, arena: &mut Vec<Package>| -> Vec<usize> {
            let mut data = as_map(data);
            let id = data.shift_remove("id");
            let loaded = load(data, origin, arena).unwrap();
            if let Some(id) = id {
                ids.insert(*loaded.last().unwrap(), id.to_string());
            }
            loaded
        };
        // The root's repository comes first in Pun's list; these fixtures have none.
        let mut repos: Vec<Box<dyn Repository>> = vec![Box::new(ArrayRepository::default())];
        for (i, repo) in case["repos"].as_array().unwrap().iter().enumerate() {
            let index = i + 1;
            if repo.get("type").is_some() {
                let packages = path_packages(&dir, &as_map(repo), None).unwrap();
                repos.push(Box::new(ArrayRepository::of(packages)));
                continue;
            }
            let list = repo.get("packages").unwrap_or(repo).as_array().unwrap();
            let mut packages = vec![];
            for data in list {
                packages.extend(add(data, Origin::Repository(index), &mut arena));
            }
            let array = Box::new(ArrayRepository {
                packages,
                ..ArrayRepository::default()
            });
            repos.push(filtered(array, &as_map(repo)).unwrap());
        }
        let mut request = Request::default();
        for data in case["fixed"].as_array().unwrap() {
            request.fixed.extend(add(data, Origin::Root, &mut arena));
        }
        if let Some(locked) = request_data.get("locked").and_then(Value::as_array) {
            let lock = locked
                .iter()
                .flat_map(|d| add(d, Origin::Locked, &mut arena));
            request.lock = Some(lock.collect());
        }
        for (n, c) in request_data["require"].as_object().unwrap() {
            let c = pun_packages::constraint::parse(c.as_str().unwrap()).unwrap();
            request.requires.insert(n.to_ascii_lowercase(), c);
        }
        if let Some(list) = request_data.get("allowList").and_then(Value::as_array) {
            request.allow_list = list
                .iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect();
            let flag = |k: &str| request_data.get(k) == Some(&Value::Bool(true));
            request.transitive = if flag("allowTransitiveDeps") {
                Transitive::All
            } else if flag("allowTransitiveDepsNoRootRequire") {
                Transitive::Dependencies
            } else {
                Transitive::OnlyListed
            };
        }
        let text = |v: &Value, k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let stabilities = Stabilities {
            minimum: stability_value(
                root.get("minimum-stability")
                    .and_then(Value::as_str)
                    .unwrap_or("stable"),
            ),
            flags: (root
                .get("stability-flags")
                .and_then(Value::as_object)
                .into_iter()
                .flatten())
            .map(|(k, v)| (k.clone(), stability_value(v.as_str().unwrap())))
            .collect(),
        };
        let mut aliases: RootAliases = HashMap::new();
        for a in root
            .get("aliases")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let alias = text(a, "alias");
            let entry = (alias.clone(), normalize(&alias).unwrap());
            (aliases.entry(text(a, "package")).or_default())
                .insert(normalize(&text(a, "version")).unwrap(), entry);
        }
        let references: HashMap<String, String> = (root
            .get("references")
            .and_then(Value::as_object)
            .into_iter()
            .flatten())
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_string()))
        .collect();
        let filters = Filters::default();
        let builder = PoolBuilder::new(&stabilities, &aliases, &references, &[], &filters, 0);
        let pool = builder.build(arena, &mut repos, &mut request).unwrap();
        let mut got: Vec<String> = (pool.ids.iter())
            .map(|&p| pool_entry(&pool.arena, &ids, p))
            .collect();
        let mut want: Vec<String> = (case["expect"].as_array().unwrap().iter())
            .map(|v| v.as_str().map_or_else(|| v.to_string(), String::from))
            .collect();
        got.sort();
        want.sort();
        if got != want {
            failures.push(format!("{name}\n  got:  {got:?}\n  want: {want:?}"));
        }
    }
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(
        failures.is_empty(),
        "{} of {} pools differ:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
