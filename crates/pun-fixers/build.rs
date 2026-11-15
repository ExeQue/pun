use std::fmt::Write as _;
use std::{env, fs, path::Path};

/// The fixers the Laravel extension registers. PHP-CS-Fixer's `php_unit_*` and Doctrine fixers ship in its core,
/// so they stay in Pun's.
const LARAVEL: &[&str] = &["pint_laravel_blade"];

fn main() {
    println!("cargo:rerun-if-changed=src/fixers");
    let mut names: Vec<String> = fs::read_dir("src/fixers")
        .unwrap()
        .filter_map(|e| {
            e.ok()?
                .file_name()
                .into_string()
                .ok()?
                .strip_suffix(".rs")
                .map(String::from)
        })
        .collect();
    names.sort();
    let mut out = String::new();
    let dir = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("src/fixers");
    for n in &names {
        let path = dir.join(format!("{n}.rs")).display().to_string();
        let _ = writeln!(out, "#[path = {path:?}] mod {n};");
    }
    for (name, doc, laravel) in [
        ("core", "The fixers outside every extension.", false),
        ("laravel", "The Laravel extension's fixers.", true),
    ] {
        let _ = write!(
            out,
            "#[doc = {doc:?}] pub fn {name}() -> Vec<Box<dyn pun_format::Fixer>> {{ vec!["
        );
        for n in names
            .iter()
            .filter(|n| LARAVEL.contains(&n.as_str()) == laravel)
        {
            let _ = write!(out, "{n}::fixer(),");
        }
        out += "] }\n";
    }
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("fixers.rs"),
        out,
    )
    .unwrap();
}
