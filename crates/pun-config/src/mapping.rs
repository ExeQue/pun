use crate::MAPPING;
use std::sync::{LazyLock, OnceLock};

pub(crate) struct Section {
    pub path: Vec<String>,
    body: &'static str,
    table: OnceLock<toml::Table>,
}

impl Section {
    pub(crate) fn table(&self) -> &toml::Table {
        self.table
            .get_or_init(|| toml::from_str(self.body).expect("rules/mapping.toml"))
    }
}

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

pub(crate) fn sections() -> &'static [Section] {
    static T: LazyLock<Vec<Section>> = LazyLock::new(|| {
        let mut out: Vec<Section> = vec![];
        let mut at = 0;
        for line in MAPPING.split_inclusive('\n') {
            if let Some(path) = header(line.trim_end()) {
                if let Some(last) = out.last_mut() {
                    last.body = &MAPPING[MAPPING.len() - last.body.len()..at];
                }
                out.push(Section {
                    path,
                    body: &MAPPING[at + line.len()..],
                    table: OnceLock::new(),
                });
            }
            at += line.len();
        }
        out
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
    sections().iter().filter_map(move |s| match &s.path[..] {
        [p, k] if p == parent => Some((k.as_str(), s)),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headers() {
        let path = |p: &[&str]| Some(p.iter().map(ToString::to_string).collect::<Vec<_>>());
        assert_eq!(header("[meta]"), path(&["meta"]));
        assert_eq!(
            header("[upstream.php-cs-fixer]"),
            path(&["upstream", "php-cs-fixer"])
        );
        assert_eq!(
            header(r#"[options."format/arrays/syntax"]"#),
            path(&["options", "format/arrays/syntax"])
        );
        assert_eq!(
            header(r#"["format/imports/group", "format/imports/single-per-statement"],"#),
            None
        );
        assert_eq!(header("  [meta]"), None);
        assert_eq!(header("x = 1"), None);
    }

    #[test]
    fn sections_split_at_headers() {
        let meta = section(&["meta"]);
        assert!(meta.table().contains_key("conflicts"));
        assert!(!meta.table().contains_key("arrays"));
        let groups = section(&["meta", "groups"]);
        assert!(groups.table().contains_key("arrays"));
        let tools: Vec<&str> = under("upstream").map(|(t, _)| t).take(3).collect();
        assert_eq!(tools, ["php-cs-fixer", "pint", "pun"]);
    }
}
