use std::sync::{LazyLock, OnceLock};

pub(crate) struct Section {
    pub path: &'static [&'static str],
    json: &'static str,
    table: OnceLock<toml::Table>,
}

impl Section {
    pub(crate) fn table(&self) -> &toml::Table {
        self.table
            .get_or_init(|| serde_json::from_str(self.json).expect("rules/mapping.toml"))
    }
}

/// Every section of rules/mapping.toml in file order: its header path and its table, as JSON (see build.rs).
static SECTIONS: &[(&[&str], &str)] = include!(concat!(env!("OUT_DIR"), "/mapping.rs"));

pub(crate) fn sections() -> &'static [Section] {
    static T: LazyLock<Vec<Section>> = LazyLock::new(|| {
        (SECTIONS.iter())
            .map(|&(path, json)| Section {
                path,
                json,
                table: OnceLock::new(),
            })
            .collect()
    });
    &T
}

pub(crate) fn section(path: &[&str]) -> &'static Section {
    sections()
        .iter()
        .find(|s| s.path == path)
        .expect("rules/mapping.toml section")
}

pub(crate) fn under(parent: &str) -> impl Iterator<Item = (&'static str, &'static Section)> {
    sections().iter().filter_map(move |s| match s.path {
        [p, k] if *p == parent => Some((*k, s)),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_split_at_headers() {
        let meta = section(&["meta"]);
        assert!(meta.table().contains_key("conflicts"));
        assert!(!meta.table().contains_key("arrays"));
        let groups = section(&["meta", "groups"]);
        assert!(groups.table().contains_key("arrays"));
        let syntax = section(&["options", "format/arrays/syntax"]);
        assert!(syntax.table().contains_key("syntax"));
        let tools: Vec<&str> = under("upstream").map(|(t, _)| t).take(3).collect();
        assert_eq!(tools, ["php-cs-fixer", "pint", "pun"]);
    }
}
