use std::{env, fs, path::Path};

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
        out += &format!("#[path = {path:?}] mod {n};\n");
    }
    out += "pub fn all() -> Vec<Box<dyn pun_format::Fixer>> { vec![";
    for n in &names {
        out += &format!("{n}::fixer(),");
    }
    out += "] }\n";
    fs::write(
        Path::new(&env::var("OUT_DIR").unwrap()).join("fixers.rs"),
        out,
    )
    .unwrap();
}
