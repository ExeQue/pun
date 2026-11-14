//! The edits `pun require` and `pun remove` make to a project's requirements: one entry of `require` or
//! `require-dev` set or removed, in pun.toml's text (every other line and comment kept as it is) or in composer.json
//! (written back with four-space indents: a file in that layout, Composer's own, changes only where Composer changes
//! it).

use crate::version::is_platform;
use anyhow::{Context, Result, bail};
use pcre2::bytes::Regex;
use pun_config::toml_text;
use pun_tokens::php_compat::strnatcmp;
use serde::Serialize;
use serde_json::{Map, Value};
use std::cmp::Ordering;
use std::sync::LazyLock;

/// Composer's `sort-packages` order: `php`, `hhvm`, extensions, libraries, other platform packages, then the rest,
/// each group compared naturally.
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

/// The line range of `[section]`'s entries in `lines`: from its header to the next header.
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

/// The lines of a section that hold one `key = value` each, with their key.
fn keys_in(lines: &[String], start: usize, end: usize) -> Vec<(usize, String)> {
    (start + 1..end)
        .filter_map(|i| {
            let table: toml::Table = toml::from_str(&lines[i]).ok()?;
            (table.len() == 1).then(|| (i, table.keys().next().cloned().unwrap_or_default()))
        })
        .collect()
}

/// That the edited pun.toml reads back with `name` in `section` at `expect`, or gone when `expect` is `None`.
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

/// pun.toml's text with `name` set to `constraint` in `[section]`: in place when it is there, else after the
/// section's last entry (at its sorted place with `sort`), in a new section at the end when there is none.
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
            out.push_str(&format!("[{section}]\n{entry}"));
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

/// pun.toml's text without `name` in `[section]`. A section left with no entries goes too, unless it holds a
/// comment of its own (a comment after a blank line belongs to what follows).
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

fn parse_json(text: &str) -> Result<Map<String, Value>> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(text).context("composer.json is not valid JSON")? {
        Value::Object(m) => Ok(m),
        _ => bail!("composer.json must hold an object"),
    }
}

/// `JsonFile::encode` with Composer's flags, then a newline: what Composer writes when it can't edit in place.
fn write_json(m: &Map<String, Value>) -> String {
    let mut out = vec![];
    let mut ser = serde_json::Serializer::with_formatter(
        &mut out,
        serde_json::ser::PrettyFormatter::with_indent(b"    "),
    );
    m.serialize(&mut ser).expect("a JSON object serializes");
    let mut text = String::from_utf8(out).expect("serde_json writes UTF-8");
    text.push('\n');
    text
}

/// The JSON grammar Composer's `JsonManipulator` matches values with.
const DEFINES: &str = r#"(?(DEFINE)
   (?<number>    -? (?= [1-9]|0(?!\d) ) \d++ (?:\.\d++)? (?:[eE] [+-]?+ \d++)? )
   (?<boolean>   true | false | null )
   (?<string>    " (?:[^"\\]*+ | \\ ["\\bfnrt\/] | \\ u [0-9A-Fa-f]{4} )* " )
   (?<array>     \[  (?:  (?&json) \s*+ (?: , (?&json) \s*+ )*+  )?+  \s*+ \] )
   (?<pair>      \s*+ (?&string) \s*+ : (?&json) \s*+ )
   (?<object>    \{  (?:  (?&pair)  (?: , (?&pair)  )*+  )?+  \s*+ \} )
   (?<json>      \s*+ (?: (?&number) | (?&boolean) | (?&string) | (?&array) | (?&object) ) )
)"#;

/// The members before the one named, as Composer's patterns skip them.
const BEFORE: &str = r"\s*\{\s*(?:(?&string)\s*:\s*(?&json)\s*,\s*)*?";

/// A pattern in `x` mode after [`DEFINES`]; `i` makes it caseless.
fn pattern(body: &str, dotall: bool, caseless: bool) -> Regex {
    pcre2::bytes::RegexBuilder::new()
        .extended(true)
        .dotall(dotall)
        .caseless(caseless)
        .build(&format!("{DEFINES}{body}"))
        .expect("valid pattern")
}

/// `preg_quote`, plus spaces, which `x` mode would drop.
fn quote(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if r".\+*?[^]$(){}=!<>|:-#".contains(c) || c == ' ' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A package name in a pattern, with `/` also matching JSON's `\/`.
fn quote_name(s: &str) -> String {
    quote(s).replace('/', r"\\?/")
}

fn encode(s: &str) -> String {
    crate::php::json_pretty(&Value::String(s.to_string()))
}

fn group<'t>(c: &pcre2::bytes::Captures<'t>, name: &str) -> &'t str {
    c.name(name).map_or("", |m| {
        std::str::from_utf8(m.as_bytes()).unwrap_or_default()
    })
}

fn is_match(re: &str, s: &str) -> bool {
    Regex::new(re).is_ok_and(|r| r.is_match(s.as_bytes()).unwrap_or(false))
}

/// Composer's `JsonManipulator`: edits composer.json's text in place, keeping its layout.
struct Manipulator {
    contents: String,
    newline: &'static str,
    indent: String,
}

impl Manipulator {
    fn new(text: &str) -> Option<Manipulator> {
        let contents = text.trim();
        let contents = if contents.is_empty() { "{}" } else { contents };
        if !(contents.starts_with('{') && contents.ends_with('}')) {
            return None;
        }
        let newline = if contents.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let indent = Regex::new(r#"(?m)^([ \t]+)""#)
            .ok()?
            .captures(contents.as_bytes())
            .ok()
            .flatten()
            .map_or("    ".to_string(), |c| {
                String::from_utf8_lossy(&c[1]).into_owned()
            });
        let contents = if contents == "{}" {
            format!("{{{newline}}}")
        } else {
            contents.to_string()
        };
        Some(Manipulator {
            contents,
            newline,
            indent,
        })
    }

    fn decoded(&self) -> Map<String, Value> {
        parse_json(&self.contents).unwrap_or_default()
    }

    fn text(&self) -> String {
        format!("{}{}", self.contents, self.newline)
    }

    /// `format`: an array or object laid out at `depth`, the way Composer writes a node it adds.
    fn format(&self, v: &Value, depth: usize) -> String {
        let list = |m: &Map<String, Value>| m.keys().enumerate().all(|(i, k)| *k == i.to_string());
        match v {
            Value::Array(items) if items.is_empty() => "[]".into(),
            Value::Object(m) if m.is_empty() => "[]".into(),
            Value::Array(items) => {
                let items: Vec<String> = items.iter().map(|i| self.format(i, depth + 1)).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Object(m) if list(m) => {
                let items: Vec<String> = m.values().map(|i| self.format(i, depth + 1)).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Object(m) => {
                let pad = self.indent.repeat(depth + 2);
                let elems: Vec<String> = (m.iter())
                    .map(|(k, v)| format!("{pad}{}: {}", encode(k), self.format(v, depth + 1)))
                    .collect();
                let sep = format!(",{}", self.newline);
                format!(
                    "{{{}{}{}{}}}",
                    self.newline,
                    elems.join(&sep),
                    self.newline,
                    self.indent.repeat(depth + 1)
                )
            }
            scalar => crate::php::json_pretty(scalar),
        }
    }

    /// `addLink`.
    fn add_link(&mut self, kind: &str, package: &str, constraint: &str, sort: bool) -> bool {
        if !self.decoded().contains_key(kind) {
            let mut content = Map::new();
            content.insert(package.into(), constraint.into());
            return self.add_main_key(kind, &Value::Object(content));
        }
        let re = pattern(
            &format!(
                r"^(?P<start>{BEFORE})(?P<property>{}\s*:\s*)(?P<value>(?&json))(?P<end>.*)",
                quote(&encode(kind))
            ),
            true,
            false,
        );
        let Ok(Some(m)) = re.captures(self.contents.as_bytes()) else {
            return false;
        };
        let (start, property, end) = (group(&m, "start"), group(&m, "property"), group(&m, "end"));
        let mut links = group(&m, "value").to_string();
        let existing = pattern(
            &format!(r#""(?P<package>{})"(\s*:\s*)(?&json)"#, quote_name(package)),
            false,
            true,
        );
        if let Ok(Some(found)) = existing.captures(links.as_bytes()) {
            let name = group(&found, "package").to_string();
            let each = pattern(
                &format!(r#""{}"(?P<separator>\s*:\s*)(?&json)"#, quote_name(&name)),
                false,
                true,
            );
            let mut out = String::new();
            let mut last = 0;
            for c in each.captures_iter(links.as_bytes()).flatten() {
                let whole = c.get(0).expect("the match");
                out.push_str(&links[last..whole.start()]);
                out.push_str(&format!(
                    "{}{}\"{constraint}\"",
                    encode(&name.replace(r"\/", "/")),
                    group(&c, "separator")
                ));
                last = whole.end();
            }
            out.push_str(&links[last..]);
            links = out;
        } else {
            let tail = Regex::new(r"(?s)^\s*\{\s*\S+.*?(\s*\}\s*)$")
                .expect("valid pattern")
                .captures(links.as_bytes())
                .ok()
                .flatten()
                .map(|c| String::from_utf8_lossy(&c[1]).into_owned());
            let entry = format!("{}: {}", encode(package), encode(constraint));
            let (nl, pad) = (self.newline, self.indent.repeat(2));
            links = match tail {
                Some(tail) => format!(
                    "{},{nl}{pad}{entry}{tail}",
                    &links[..links.len() - tail.len()]
                ),
                None => format!("{{{nl}{pad}{entry}{nl}{}}}", self.indent),
            };
        }
        if sort {
            let Ok(Value::Object(m)) = serde_json::from_str::<Value>(&links) else {
                return false;
            };
            let mut entries: Vec<(String, Value)> = m.into_iter().collect();
            entries.sort_by(|a, b| sort_cmp(&a.0, &b.0));
            links = self.format(&Value::Object(entries.into_iter().collect()), 0);
        }
        self.contents = format!("{start}{property}{links}{end}");
        true
    }

    /// `removeLink`: the entry, then the section when it is left empty.
    fn remove_link(&mut self, kind: &str, package: &str) -> bool {
        let before = self.contents.clone();
        self.remove_sub_node(kind, package)
            && (before == self.contents || self.remove_main_key_if_empty(kind))
    }

    /// `removeSubNode` for a link section.
    fn remove_sub_node(&mut self, main: &str, name: &str) -> bool {
        let decoded = self.decoded();
        let node = decoded.get(main);
        let empty = match node {
            None | Some(Value::Null | Value::Bool(false)) => true,
            Some(Value::Object(o)) => o.is_empty(),
            Some(Value::Array(a)) => a.is_empty(),
            Some(Value::String(s)) => s.is_empty() || s == "0",
            Some(_) => false,
        };
        if empty {
            return true;
        }
        let re = pattern(
            &format!(
                r"^(?P<start> \s* \{{ \s* (?: (?&string) \s* : (?&json) \s* , \s* )*?{}\s*:\s*)(?P<content>(?&object))(?P<end>.*)",
                quote(&encode(main))
            ),
            true,
            false,
        );
        let Ok(Some(m)) = re.captures(self.contents.as_bytes()) else {
            return false;
        };
        let (start, children, end) = (group(&m, "start"), group(&m, "content"), group(&m, "end"));
        if !node
            .and_then(Value::as_object)
            .is_some_and(|o| o.contains_key(name))
        {
            return true;
        }
        let key = quote_name(&encode(name));
        let child = pattern(
            &format!(
                r"^(?P<start>(?P<prefix>\{{ \s* (?: (?!{key}\s*:) (?&string) \s* : (?&json) \s* , \s* )*+) {key}\s*:\s*)(?P<content>(?&json))(?P<end>,?)"
            ),
            false,
            false,
        );
        let Ok(Some(c)) = child.captures(children.as_bytes()) else {
            return false;
        };
        let (prefix, tail) = (group(&c, "prefix"), group(&c, "end"));
        let rest = &children[c.get(0).expect("the match").end() - tail.len()..];
        let clean = if is_match(r",\s*$", prefix) {
            let p = Regex::new(r",\s*$").expect("valid pattern");
            let cut = p
                .find(prefix.as_bytes())
                .ok()
                .flatten()
                .map_or(prefix.len(), |m| m.start());
            format!("{}{rest}", &prefix[..cut])
        } else {
            let lead = Regex::new(r"^\s*,?\s*").expect("valid pattern");
            let cut = lead
                .find(rest.as_bytes())
                .ok()
                .flatten()
                .map_or(0, |m| m.end());
            format!("{prefix}{}", &rest[cut..])
        };
        let nothing_left = Regex::new(r"(?s)^\{\s*?(?P<content>\S+.*?)?(?P<trailingspace>\s*)\}$")
            .expect("valid pattern")
            .captures(clean.as_bytes())
            .ok()
            .flatten()
            .is_some_and(|c| c.name("content").is_none());
        self.contents = if nothing_left {
            format!("{start}{{{}{}}}{end}", self.newline, self.indent)
        } else {
            format!("{start}{clean}{end}")
        };
        true
    }

    /// `addMainKey`.
    fn add_main_key(&mut self, key: &str, content: &Value) -> bool {
        let content = self.format(content, 0);
        let re = pattern(
            &format!(
                r"^(?P<start>{BEFORE})(?P<key>{}\s*:\s*(?&json))(?P<end>.*)",
                quote(&encode(key))
            ),
            true,
            false,
        );
        if self.decoded().contains_key(key)
            && let Ok(Some(m)) = re.captures(self.contents.as_bytes())
        {
            self.contents = format!(
                "{}{}: {content}{}",
                group(&m, "start"),
                encode(key),
                group(&m, "end")
            );
            return true;
        }
        let (nl, indent) = (self.newline, &self.indent);
        let entry = format!("{indent}{}: {content}{nl}}}", encode(key));
        let tail = Regex::new(r"[^{\s](\s*)\}$")
            .expect("valid pattern")
            .captures(self.contents.as_bytes())
            .ok()
            .flatten()
            .map(|c| c[1].len());
        self.contents = match tail {
            Some(n) => format!(
                "{},{nl}{entry}",
                &self.contents[..self.contents.len() - n - 1]
            ),
            None => format!("{}{entry}", &self.contents[..self.contents.len() - 1]),
        };
        true
    }

    /// `removeMainKeyIfEmpty`.
    fn remove_main_key_if_empty(&mut self, key: &str) -> bool {
        let empty = match self.decoded().get(key) {
            None => return true,
            Some(Value::Object(o)) => o.is_empty(),
            Some(Value::Array(a)) => a.is_empty(),
            Some(_) => false,
        };
        !empty || self.remove_main_key(key)
    }

    /// `removeMainKey`.
    fn remove_main_key(&mut self, key: &str) -> bool {
        let re = pattern(
            &format!(
                r"^(?P<start>{BEFORE})(?P<removal>{}\s*:\s*(?&json))\s*,?\s*(?P<end>.*)",
                quote(&encode(key))
            ),
            true,
            false,
        );
        let Ok(Some(m)) = re.captures(self.contents.as_bytes()) else {
            return false;
        };
        let (mut start, end) = (group(&m, "start").to_string(), group(&m, "end"));
        if is_match(r",\s*$", &start) && end == "}" {
            let comma = start.rfind(',').expect("a comma");
            start = format!("{}{}", &start[..comma], &start[comma + 1..]);
            let chars: Vec<char> = self.indent.chars().collect();
            start = start.trim_end_matches(chars.as_slice()).to_string();
        }
        self.contents = format!("{start}{end}");
        if is_match(r"^\{\s*\}\s*$", &self.contents) {
            self.contents = "{\n}".into();
        }
        true
    }
}

/// composer.json's text with `name` set to `constraint` in `section` and taken out of `other`, as `composer
/// require` edits it: in place, keeping the file's layout, or rewritten when the file is laid out in a way
/// Composer's patterns don't follow.
pub fn json_set(
    text: &str,
    section: &str,
    other: &str,
    name: &str,
    constraint: &str,
    sort: bool,
) -> Result<String> {
    let mut m = parse_json(text)?;
    if let Some(mut edit) = Manipulator::new(text)
        && edit.add_link(section, name, constraint, sort)
        && edit.remove_link(other, name)
    {
        return Ok(edit.text());
    }
    let links = m
        .entry(section)
        .or_insert_with(|| Value::Object(Map::new()));
    let Value::Object(links) = links else {
        bail!("composer.json: {section} must be an object");
    };
    links.insert(name.to_string(), constraint.into());
    if let Some(Value::Object(o)) = m.get_mut(other) {
        o.shift_remove(name);
        if o.is_empty() {
            m.shift_remove(other);
        }
    }
    Ok(write_json(&m))
}

/// composer.json's text without `name` in `section` (and without `section` when that empties it), as
/// `composer remove` edits it.
pub fn json_remove(text: &str, section: &str, name: &str) -> Result<String> {
    let mut m = parse_json(text)?;
    if let Some(mut edit) = Manipulator::new(text)
        && edit.remove_sub_node(section, name)
        && edit.remove_main_key_if_empty(section)
    {
        return Ok(edit.text());
    }
    if let Some(Value::Object(o)) = m.get_mut(section) {
        o.shift_remove(name);
        if o.is_empty() {
            m.shift_remove(section);
        }
    }
    Ok(write_json(&m))
}

/// `VersionParser::parseNameVersionPairs`: `name:version`, `name=version` and `name version` in one argument, or a
/// name and the version as the next argument (one that is not a package name, a pattern or a platform package).
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

    /// The order Composer 2.11.2's `JsonManipulator::sortPackages` gives the same names.
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

    /// Composer 2.11.2's `parseNameVersionPairs` on the same arguments.
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
        // An inline table can't be edited line by line.
        let inline = "require = { php = \"^8.4\" }\n";
        assert!(toml_set(inline, "require", "a/b", "1.0", false).is_err());
        assert!(toml_remove(inline, "require", "php").is_err());
        assert!(toml_set("[require\n", "require", "a/b", "1.0", false).is_err());
    }

    /// The text Composer 2.11.2's `require -n --no-update` and `remove -n --no-update` write for the same
    /// composer.json, in Composer's four-space layout.
    #[test]
    fn edits_composer_json() {
        let text = "{\n    \"name\": \"acme/app\",\n    \"require\": {\n        \"php\": \"^8.4\",\n        \"acme/b\": \"1.0\"\n    },\n    \"require-dev\": {\n        \"acme/c\": \"^2.0\"\n    },\n    \"extra\": {}\n}\n";
        let moved = json_set(text, "require", "require-dev", "acme/c", "^2.1", true).unwrap();
        assert_eq!(
            json_set(&moved, "require", "require-dev", "ext-intl", "*", true).unwrap(),
            "{\n    \"name\": \"acme/app\",\n    \"require\": {\n        \"php\": \"^8.4\",\n        \"ext-intl\": \"*\",\n        \"acme/b\": \"1.0\",\n        \"acme/c\": \"^2.1\"\n    },\n    \"extra\": {}\n}\n"
        );
        assert_eq!(
            json_set(text, "require", "require-dev", "acme/b", "2.0", false).unwrap(),
            text.replace("\"acme/b\": \"1.0\"", "\"acme/b\": \"2.0\"")
        );
        assert_eq!(
            json_remove(text, "require", "acme/b").unwrap(),
            "{\n    \"name\": \"acme/app\",\n    \"require\": {\n        \"php\": \"^8.4\"\n    },\n    \"require-dev\": {\n        \"acme/c\": \"^2.0\"\n    },\n    \"extra\": {}\n}\n"
        );
        assert_eq!(
            json_set("", "require", "require-dev", "a/b", "*", false).unwrap(),
            "{\n    \"require\": {\n        \"a/b\": \"*\"\n    }\n}\n"
        );
        // composer remove --dev on the only require-dev entry.
        assert_eq!(
            json_remove(text, "require-dev", "acme/c").unwrap(),
            text.replace(
                "    \"require-dev\": {\n        \"acme/c\": \"^2.0\"\n    },\n",
                ""
            )
        );
        assert!(json_set("[]", "require", "require-dev", "a/b", "*", false).is_err());
        assert!(json_remove("{", "require", "a/b").is_err());
    }
}
