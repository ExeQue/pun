//! Splits `rules/mapping.toml` at its table headers and stores each section's table as JSON in
//! `$OUT_DIR/mapping.rs`, which `mapping::sections` includes: `pun` reads a few hundred KB of the mapping at every
//! start, and JSON parses many times faster than TOML.

use std::fmt::Write as _;

fn header(line: &str) -> Option<Vec<String>> {
    if !line.starts_with('[') {
        return None;
    }
    let mut t: toml::Table = toml::from_str(line).ok()?;
    let mut path = vec![];
    while let Some((k, toml::Value::Table(next))) = t.into_iter().next() {
        path.push(k);
        t = next;
    }
    Some(path)
}

fn main() {
    println!("cargo::rerun-if-changed=rules/mapping.toml");
    let text = std::fs::read_to_string("rules/mapping.toml").expect("rules/mapping.toml");
    let mut sections: Vec<(Vec<String>, usize)> = vec![];
    let mut ends = vec![];
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        if let Some(path) = header(line.trim_end()) {
            ends.push(at);
            sections.push((path, at + line.len()));
        }
        at += line.len();
    }
    ends.push(text.len());
    let mut out = String::from("&[\n");
    for ((path, start), end) in sections.into_iter().zip(&ends[1..]) {
        let table: toml::Table = toml::from_str(&text[start..*end])
            .unwrap_or_else(|e| panic!("rules/mapping.toml [{}]: {e}", path.join(".")));
        let json = serde_json::to_string(&table).expect("a TOML table is JSON");
        writeln!(out, "(&{path:?}, {json:?}),").expect("writing to a String");
    }
    out.push(']');
    let dir = std::env::var_os("OUT_DIR").expect("OUT_DIR");
    std::fs::write(std::path::Path::new(&dir).join("mapping.rs"), out)
        .expect("$OUT_DIR/mapping.rs");
}
