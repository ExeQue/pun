use crate::{mapping, renamed, rules};
use anyhow::{Result, bail};
use std::fmt::Write;

pub const RULE_DOCS_URL: &str = "https://punphp.com/docs/rules/";

pub fn explain(name: &str) -> Result<String> {
    let exact = rules()
        .iter()
        .find(|(id, _)| id == name || id.split_once('/').unwrap().1 == name);
    if let Some((id, _)) = exact {
        return Ok(text(id));
    }
    if let Some(id) = crate::merged_into(name) {
        return Ok(format!("`{name}` was merged into `{id}`.\n\n{}", text(id)));
    }
    let id = match entries().into_iter().find(|(_, k, _)| k == name) {
        Some((tool, _, e)) => {
            let str = |k| e.get(k).and_then(toml::Value::as_str).map(str::to_string);
            match str("pun") {
                Some(pun) if crate::rule(&pun).is_some() => Some(pun),
                Some(pun) => bail!("`{name}` ({tool}) is `{pun}` in pun.toml, not a rule"),
                None => bail!(
                    "`{name}` ({tool}) has no Pun rule: {}",
                    str("drop").or(str("unsupported")).unwrap()
                ),
            }
        }
        None => renamed(name).map(str::to_string),
    };
    match id {
        Some(id) => Ok(format!("`{name}` is `{id}` in Pun.\n\n{}", text(&id))),
        None => bail!("`{name}` is not a Pun rule or an upstream rule name"),
    }
}

fn entries() -> Vec<(String, String, toml::Value)> {
    let mut out = vec![];
    for (tool, s) in mapping::under("upstream") {
        for (k, e) in s.table() {
            out.push((tool.to_string(), k.clone(), e.clone()));
        }
    }
    out
}

fn text(id: &str) -> String {
    let (_, e) = rules().iter().find(|(r, _)| r == id).unwrap();
    let str = |k| e.get(k).and_then(toml::Value::as_str);
    let list = |k| -> Vec<&str> {
        (e.get(k).and_then(toml::Value::as_array))
            .map_or_default(|a| a.iter().filter_map(toml::Value::as_str).collect())
    };
    let (command, rest) = id.split_once('/').unwrap();
    let group = rest.split_once('/').unwrap().0;
    let mut out = format!("{id}\n");
    if let Some(s) = str("summary") {
        let _ = writeln!(out, "{s}");
    }
    out += "\n";
    let mut row = |label: &str, value: &str| writeln!(out, "{label:<10}{value}").unwrap();
    row("group", &format!("{group}: {}", group_description(group)));
    let opt_in = |v: Vec<&str>| {
        if v.is_empty() {
            "none (opt-in by name or group)".to_string()
        } else {
            v.join(", ")
        }
    };
    match command {
        "format" => row("presets", &opt_in(list("presets"))),
        "lint" => {
            row("preset", &opt_in(str("preset").into_iter().collect()));
            let sets: Vec<&str> = (crate::rector_sets().iter())
                .filter(|(_, rules)| rules.contains_key(id))
                .map(|(s, _)| s.as_str())
                .collect();
            if !sets.is_empty() {
                row("sets", &sets.join(", "));
            }
        }
        _ => row("preset", &opt_in(str("preset").into_iter().collect())),
    }
    if let Some(p) = phpstan_preset(id) {
        row("phpstan", &p);
    }
    if let Some(i) = str("intent") {
        row("intent", i);
    }
    row("fix", str("fix").unwrap_or("none"));
    let fixed_by = list("fixed_by");
    if !fixed_by.is_empty() {
        row("fixed by", &fixed_by.join(", "));
    }
    let names: Vec<(String, String)> = (entries().into_iter())
        .filter(|(_, _, e)| e.get("pun").and_then(toml::Value::as_str) == Some(id))
        .map(|(tool, k, _)| (tool, k))
        .chain(
            (entries().into_iter())
                .filter(|(_, _, e)| {
                    (e.get("also").and_then(toml::Value::as_array))
                        .is_some_and(|a| a.iter().any(|r| r.as_str() == Some(id)))
                })
                .map(|(tool, k, _)| (format!("{tool}, also"), k)),
        )
        .collect();
    if names.is_empty() {
        row("upstream", "none (Pun's own rule)");
    }
    for (i, (tool, k)) in names.iter().enumerate() {
        row(
            if i == 0 { "upstream" } else { "" },
            &format!("{k} ({tool})"),
        );
    }
    row("docs", &format!("{RULE_DOCS_URL}{id}"));
    out
}

/// The lowest PHPStan level preset that runs rule `id` (`phpstan/level_4`), marked when only bleeding edge does.
fn phpstan_preset(id: &str) -> Option<String> {
    let lowest = |bleeding_edge| {
        (0..=crate::schema::PHPSTAN_MAX_LEVEL)
            .find(|l| crate::phpstan_level_rules(*l, bleeding_edge).contains(id))
    };
    match (lowest(false), lowest(true)) {
        (Some(l), _) => Some(format!("phpstan/level_{l} and up")),
        (None, Some(l)) => Some(format!(
            "phpstan/level_{l} and up, with bleeding-edge = true"
        )),
        (None, None) => None,
    }
}

fn group_description(group: &str) -> String {
    let groups = mapping::section(&["meta", "groups"]).table();
    groups[group].as_str().unwrap().to_string()
}
