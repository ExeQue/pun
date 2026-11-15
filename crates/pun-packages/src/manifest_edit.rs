use crate::error::{Context, Result, bail};
use crate::version::is_platform;
use pcre2::bytes::Regex;
use pun_config::toml_text;
use pun_tokens::php_compat::strnatcmp;
use std::cmp::Ordering;
use std::fmt::Write as _;
use std::sync::LazyLock;

pub fn sort_cmp(a: &str, b: &str) -> Ordering {
    let prefix = |name: &str| -> String {
        if !is_platform(name) {
            return format!("5-{name}");
        }
        for (start, group) in [("php", '0'), ("hhvm", '1'), ("ext", '2'), ("lib", '3')] {
            if name.starts_with(start) {
                return format!("{group}-{name}");
            }
        }
        format!("4-{name}")
    };
    strnatcmp(prefix(a).as_bytes(), prefix(b).as_bytes()).cmp(&0)
}

fn find_section(lines: &[String], section: &str) -> Option<(usize, usize)> {
    let header = format!("[{section}]");
    let start = lines.iter().position(|l| {
        let l = l.split('#').next().unwrap_or_default();
        l.trim() == header
    })?;
    let end = (start + 1..lines.len())
        .find(|&i| lines[i].trim_start().starts_with('['))
        .unwrap_or(lines.len());
    Some((start, end))
}

fn keys_in(lines: &[String], start: usize, end: usize) -> Vec<(usize, String)> {
    (start + 1..end)
        .filter_map(|i| {
            let table: toml::Table = toml::from_str(&lines[i]).ok()?;
            (table.len() == 1).then(|| (i, table.keys().next().cloned().unwrap_or_default()))
        })
        .collect()
}

fn check(text: &str, section: &str, name: &str, expect: Option<&str>) -> Result<()> {
    let table: toml::Table =
        toml::from_str(text).context("the edited pun.toml is not valid TOML")?;
    let found: Vec<&toml::Value> = (table
        .get(section)
        .and_then(toml::Value::as_table)
        .into_iter())
    .flatten()
    .filter(|(k, _)| k.eq_ignore_ascii_case(name))
    .map(|(_, v)| v)
    .collect();
    let ok = match expect {
        Some(c) => found.len() == 1 && found[0].as_str() == Some(c),
        None => found.is_empty(),
    };
    if !ok {
        bail!(
            "pun.toml writes [{section}] in a form Pun can't edit (a dotted or inline table); change {name} there by hand"
        );
    }
    Ok(())
}

pub fn toml_set(
    text: &str,
    section: &str,
    name: &str,
    constraint: &str,
    sort: bool,
) -> Result<String> {
    let entry = format!(
        "{} = {}\n",
        toml_text::key(name),
        toml_text::string(constraint)
    );
    let mut lines: Vec<String> = text.split_inclusive('\n').map(String::from).collect();
    let out = match find_section(&lines, section) {
        None => {
            let mut out = text.to_string();
            if !out.is_empty() {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push('\n');
            }
            let _ = write!(out, "[{section}]\n{entry}");
            out
        }
        Some((start, end)) => {
            let keys = keys_in(&lines, start, end);
            if let Some((i, _)) = keys.iter().find(|(_, k)| k.eq_ignore_ascii_case(name)) {
                lines[*i] = entry;
            } else {
                let sorted = (keys.iter())
                    .find(|(_, k)| sort && sort_cmp(k, name) == Ordering::Greater)
                    .map(|(i, _)| *i);
                let at = sorted.unwrap_or_else(|| keys.last().map_or(start + 1, |(i, _)| i + 1));
                if !lines[at - 1].ends_with('\n') {
                    lines[at - 1].push('\n');
                }
                lines.insert(at, entry);
            }
            lines.concat()
        }
    };
    check(&out, section, name, Some(constraint))?;
    Ok(out)
}

pub fn toml_remove(text: &str, section: &str, name: &str) -> Result<String> {
    let mut lines: Vec<String> = text.split_inclusive('\n').map(String::from).collect();
    if let Some((start, end)) = find_section(&lines, section) {
        let keys = keys_in(&lines, start, end);
        let gone: Vec<usize> = (keys.iter())
            .filter(|(_, k)| k.eq_ignore_ascii_case(name))
            .map(|(i, _)| *i)
            .collect();
        for i in gone.iter().rev() {
            lines.remove(*i);
        }
        let end = end - gone.len();
        let blank = |l: &String| l.trim().is_empty();
        let comment = |l: &String| l.trim_start().starts_with('#');
        let content = &lines[start + 1..end];
        let cut = (1..content.len())
            .find(|&i| comment(&content[i]) && blank(&content[i - 1]))
            .unwrap_or(content.len());
        if !gone.is_empty() && keys.len() == gone.len() && content[..cut].iter().all(blank) {
            lines.drain(start..start + 1 + cut);
            if start > 0 && blank(&lines[start - 1]) && lines.get(start).is_none_or(blank) {
                lines.remove(start - 1);
            }
        }
    }
    let out = lines.concat();
    check(&out, section, name, None)?;
    Ok(out)
}

pub fn name_version_pairs(args: &[String]) -> Vec<(String, Option<String>)> {
    static SEPARATOR: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^([^=: ]+)[=: ](.*)$").expect("valid pattern"));
    static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)(?<=[a-z0-9_/-])\*|\*(?=[a-z0-9_/-])").expect("valid pattern")
    });
    let mut out = vec![];
    let mut i = 0;
    while i < args.len() {
        let arg = args[i].trim();
        let mut pair = match SEPARATOR.captures(arg.as_bytes()).ok().flatten() {
            Some(c) => format!(
                "{} {}",
                String::from_utf8_lossy(&c[1]),
                String::from_utf8_lossy(&c[2])
            ),
            None => arg.to_string(),
        };
        if !pair.contains(' ')
            && let Some(next) = args.get(i + 1)
            && !next.contains('/')
            && !PATTERN.is_match(next.as_bytes()).unwrap_or(false)
            && !is_platform(next)
        {
            pair = format!("{pair} {next}");
            i += 1;
        }
        match pair.split_once(' ') {
            Some((name, version)) if !name.is_empty() => {
                out.push((name.to_string(), Some(version.to_string())));
            }
            _ => out.push((pair, None)),
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorts_as_composer() {
        let mut names = vec![
            "laravel/framework",
            "ext-mbstring",
            "lib-icu",
            "composer-plugin-api",
            "php",
            "acme/pkg10",
            "acme/pkg9",
            "ext-intl",
            "hhvm",
        ];
        names.sort_by(|a, b| sort_cmp(a, b));
        assert_eq!(
            names,
            [
                "php",
                "hhvm",
                "ext-intl",
                "ext-mbstring",
                "lib-icu",
                "composer-plugin-api",
                "acme/pkg9",
                "acme/pkg10",
                "laravel/framework"
            ]
        );
    }

    #[test]
    fn pairs_names_and_versions_as_composer() {
        let args: Vec<String> = [
            "a/b:^1.0", "c/d", "^2", "e/f", "g/h", "ext-intl", "i/j=1.0", "k/l 2.0", "m/n", "o/*",
            "p/q", "*", "r/s",
        ]
        .map(String::from)
        .to_vec();
        let v = |s: &str| Some(s.to_string());
        assert_eq!(
            name_version_pairs(&args),
            [
                ("a/b".into(), v("^1.0")),
                ("c/d".into(), v("^2")),
                ("e/f".into(), None),
                ("g/h".into(), None),
                ("ext-intl".into(), None),
                ("i/j".into(), v("1.0")),
                ("k/l".into(), v("2.0")),
                ("m/n".into(), None),
                ("o/*".into(), None),
                ("p/q".into(), v("*")),
                ("r/s".into(), None),
            ]
        );
    }

    const PUN_TOML: &str = "# the app\n[format]\npreset = \"laravel\"\n\n[require]\nphp = \"^8.4\" # floor\n\"laravel/framework\" = \"^13.0\"\n\n# lint below\n[lint]\nsets = []\n";

    #[test]
    fn edits_pun_toml_in_place() {
        let replaced = toml_set(PUN_TOML, "require", "Laravel/Framework", "^13.1", false).unwrap();
        assert_eq!(
            replaced,
            PUN_TOML.replace(
                "\"laravel/framework\" = \"^13.0\"",
                "\"Laravel/Framework\" = \"^13.1\""
            )
        );
        let added = toml_set(PUN_TOML, "require", "acme/a", "^1.0", false).unwrap();
        assert_eq!(
            added,
            PUN_TOML.replace("^13.0\"\n", "^13.0\"\n\"acme/a\" = \"^1.0\"\n")
        );
        let sorted = toml_set(PUN_TOML, "require", "ext-intl", "*", true).unwrap();
        assert_eq!(
            sorted,
            PUN_TOML.replace("# floor\n", "# floor\next-intl = \"*\"\n")
        );
        let new = toml_set(PUN_TOML, "require-dev", "pestphp/pest", "^4.0", false).unwrap();
        assert_eq!(
            new,
            format!("{PUN_TOML}\n[require-dev]\n\"pestphp/pest\" = \"^4.0\"\n")
        );
        assert_eq!(
            toml_set("", "require", "a/b", "1.0", false).unwrap(),
            "[require]\n\"a/b\" = \"1.0\"\n"
        );
        assert_eq!(
            toml_set("x = 1", "require", "a/b", "1.0", false).unwrap(),
            "x = 1\n\n[require]\n\"a/b\" = \"1.0\"\n"
        );
        assert_eq!(
            toml_set("[require]", "require", "a/b", "1.0", false).unwrap(),
            "[require]\n\"a/b\" = \"1.0\"\n"
        );
        let removed = toml_remove(PUN_TOML, "require", "laravel/framework").unwrap();
        assert_eq!(
            removed,
            PUN_TOML.replace("\"laravel/framework\" = \"^13.0\"\n", "")
        );
        assert_eq!(
            toml_remove(PUN_TOML, "require-dev", "a/b").unwrap(),
            PUN_TOML
        );
        let emptied = toml_remove(PUN_TOML, "require", "php").unwrap();
        assert_eq!(
            toml_remove(&emptied, "require", "laravel/framework").unwrap(),
            "# the app\n[format]\npreset = \"laravel\"\n\n# lint below\n[lint]\nsets = []\n"
        );
        let last = "[format]\npreset = \"laravel\"\n\n[require-dev]\n\"a/b\" = \"1\"\n";
        assert_eq!(
            toml_remove(last, "require-dev", "a/b").unwrap(),
            "[format]\npreset = \"laravel\"\n"
        );
        let own = "[require]\n# pinned for the 8.4 runtime\n\"a/b\" = \"1\"\n";
        assert_eq!(
            toml_remove(own, "require", "a/b").unwrap(),
            "[require]\n# pinned for the 8.4 runtime\n"
        );
        let inline = "require = { php = \"^8.4\" }\n";
        assert!(toml_set(inline, "require", "a/b", "1.0", false).is_err());
        assert!(toml_remove(inline, "require", "php").is_err());
        assert!(toml_set("[require\n", "require", "a/b", "1.0", false).is_err());
    }
}
