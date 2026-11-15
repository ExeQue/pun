use crate::mapping;

const SECTIONS: [&str; 4] = [
    "phpstan",
    "larastan",
    "phpstan-strict-rules",
    "phpstan-deprecation-rules",
];

pub(crate) fn entry(identifier: &str) -> Option<&'static toml::Value> {
    let sections = SECTIONS.map(|s| mapping::section(&["upstream", s]).table());
    if let Some(e) = sections.iter().find_map(|t| t.get(identifier)) {
        return Some(e);
    }
    sections.iter().find_map(|t| {
        t.iter().find_map(|(k, e)| {
            let parts: Vec<&str> = k.split("%s").collect();
            (parts.len() > 1 && glob(&parts, identifier)).then_some(e)
        })
    })
}

pub fn rule_of_identifier(identifier: &str) -> Option<&'static str> {
    entry(identifier)?.get("pun")?.as_str()
}

fn glob(parts: &[&str], s: &str) -> bool {
    let Some(rest) = s.strip_prefix(parts[0]) else {
        return false;
    };
    if parts.len() == 1 {
        return rest.is_empty();
    }
    let word = |c: &u8| c.is_ascii_alphanumeric() || *c == b'_';
    let run = rest.bytes().take_while(word).count();
    (1..=run).any(|n| glob(&parts[1..], &rest[n..]))
}
