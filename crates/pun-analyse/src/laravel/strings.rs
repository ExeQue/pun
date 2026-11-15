pub(super) fn class_basename(class: &str) -> &str {
    class.rsplit('\\').next().unwrap_or(class)
}

pub(crate) fn snake(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        if c.is_ascii_uppercase() && !out.is_empty() {
            out.push('_');
        }
        out.push(c.to_ascii_lowercase());
    }
    out
}

pub(super) fn studly(value: &str) -> String {
    value
        .split(['-', '_', ' '])
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| {
                f.to_ascii_uppercase().to_string() + c.as_str()
            })
        })
        .collect()
}

pub(crate) fn camel(value: &str) -> String {
    let s = studly(value);
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| {
        f.to_ascii_lowercase().to_string() + c.as_str()
    })
}

pub(super) fn plural_studly(value: &str) -> String {
    let split = value
        .char_indices()
        .filter(|&(i, c)| i > 0 && c.is_ascii_uppercase())
        .map(|(i, _)| i)
        .next_back()
        .unwrap_or(0);
    format!("{}{}", &value[..split], plural(&value[split..]))
}

fn plural(word: &str) -> String {
    let lower = word.to_ascii_lowercase();
    let bytes = lower.as_bytes();
    if ["x", "ch", "ss", "sh"].iter().any(|s| lower.ends_with(s)) {
        format!("{word}es")
    } else if lower.ends_with('s') {
        word.to_string()
    } else if bytes.len() > 1 && lower.ends_with('y') && !b"aeiou".contains(&bytes[bytes.len() - 2])
    {
        format!("{}ies", &word[..word.len() - 1])
    } else {
        format!("{word}s")
    }
}

pub(super) fn singular(word: &str) -> String {
    let lower = word.to_ascii_lowercase();
    if lower.ends_with("ies") && word.len() > 3 {
        format!("{}y", &word[..word.len() - 3])
    } else if ["xes", "ches", "sses", "shes"]
        .iter()
        .any(|s| lower.ends_with(s))
    {
        word[..word.len() - 2].to_string()
    } else if lower.ends_with('s') && !lower.ends_with("ss") {
        word[..word.len() - 1].to_string()
    } else {
        word.to_string()
    }
}
