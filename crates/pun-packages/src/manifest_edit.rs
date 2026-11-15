use crate::error::{Context, Result, bail};
use crate::version::is_platform;
use pcre2::bytes::Regex;
use pun_tokens::php_compat::strnatcmp;
use serde_json::{Map, Value};
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

fn parse_json(text: &str) -> Result<Map<String, Value>> {
    if text.trim().is_empty() {
        return Ok(Map::new());
    }
    match serde_json::from_str(text).context("composer.json is not valid JSON")? {
        Value::Object(m) => Ok(m),
        _ => bail!("composer.json must hold an object"),
    }
}

/// Whether PHP decodes the object into a list, which `json_encode` writes as an array.
fn is_list(m: &Map<String, Value>) -> bool {
    m.keys().enumerate().all(|(i, k)| *k == i.to_string())
}

/// `JsonConfigSource::restoreEmptyObjects`: the empty values Composer writes as `{}`, not `[]`.
fn restored(path: &[String]) -> bool {
    const ROOT: [&str; 13] = [
        "require",
        "require-dev",
        "conflict",
        "provide",
        "replace",
        "suggest",
        "config",
        "autoload",
        "autoload-dev",
        "scripts",
        "scripts-descriptions",
        "scripts-aliases",
        "support",
    ];
    const CONFIG: [&str; 10] = [
        "policy",
        "platform",
        "http-basic",
        "bearer",
        "gitlab-token",
        "gitlab-oauth",
        "github-oauth",
        "custom-headers",
        "forgejo-token",
        "preferred-install",
    ];
    let path: Vec<&str> = path.iter().map(String::as_str).collect();
    match path[..] {
        [key] => ROOT.contains(&key),
        ["config", key] => CONFIG.contains(&key),
        ["config", "policy", _] | ["autoload" | "autoload-dev", "psr-0" | "psr-4"] => true,
        _ => false,
    }
}

/// What Composer writes when it can't edit in place: the file as `JsonFile::read` decodes it (objects become PHP
/// arrays), after `restoreEmptyObjects`, through `JsonFile::write` with the file's indent, then a newline.
fn write_json(m: Map<String, Value>, indent: &str) -> String {
    fn put(out: &mut String, v: &Value, path: &mut Vec<String>, indent: &str) {
        let items: Vec<(Option<&String>, &Value)> = match v {
            Value::Array(items) => items.iter().map(|v| (None, v)).collect(),
            Value::Object(m) if is_list(m) => m.values().map(|v| (None, v)).collect(),
            Value::Object(m) => m.iter().map(|(k, v)| (Some(k), v)).collect(),
            scalar => return out.push_str(&crate::php::json_pretty(scalar)),
        };
        let object = matches!(v, Value::Object(m) if !is_list(m));
        if items.is_empty() {
            out.push_str(if restored(path) { "{}" } else { "[]" });
            return;
        }
        out.push(if object { '{' } else { '[' });
        for (i, (key, item)) in items.into_iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('\n');
            out.push_str(&indent.repeat(path.len() + 1));
            if let Some(key) = key {
                let _ = write!(out, "{}: ", encode(key));
            }
            path.push(key.map_or_else(|| i.to_string(), String::clone));
            put(out, item, path, indent);
            path.pop();
        }
        out.push('\n');
        out.push_str(&indent.repeat(path.len()));
        out.push(if object { '}' } else { ']' });
    }
    let mut out = String::new();
    put(&mut out, &Value::Object(m), &mut vec![], indent);
    out.push('\n');
    out
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
    /// For text that parses as an object, so it is `{...}` once trimmed, as Composer's constructor requires.
    fn new(text: &str) -> Manipulator {
        let contents = text.trim();
        let contents = if contents.is_empty() { "{}" } else { contents };
        let newline = if contents.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let indent = Regex::new(r#"(?m)^([ \t]+)""#)
            .expect("valid pattern")
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
        Manipulator {
            contents,
            newline,
            indent,
        }
    }

    fn decoded(&self) -> Map<String, Value> {
        parse_json(&self.contents).unwrap_or_default()
    }

    fn text(&self) -> String {
        format!("{}{}", self.contents, self.newline)
    }

    /// `format`: an array or object laid out at `depth`, the way Composer writes a node it adds.
    fn format(&self, v: &Value, depth: usize) -> String {
        match v {
            Value::Array(items) => {
                let items: Vec<String> = items.iter().map(|i| self.format(i, depth + 1)).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Object(m) if is_list(m) => {
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
        if self.decoded().get(kind).is_none_or(Value::is_null) {
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
                let _ = write!(
                    out,
                    "{}{}\"{constraint}\"",
                    encode(&name.replace(r"\/", "/")),
                    group(&c, "separator")
                );
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
            // Composer fails with a TypeError where the links no longer decode, as when the constraint holds a `"`.
            let requirements = match serde_json::from_str::<Value>(&links) {
                Ok(Value::Object(m)) => {
                    let mut entries: Vec<(String, Value)> = m.into_iter().collect();
                    entries.sort_by(|a, b| sort_cmp(&a.0, &b.0));
                    Value::Object(entries.into_iter().collect())
                }
                Ok(list) => list,
                Err(_) => return false,
            };
            links = self.format(&requirements, 0);
        }
        self.contents = format!("{start}{property}{links}{end}");
        true
    }

    /// `removeLink`: the entry, then the section when it is left empty.
    fn remove_link(&mut self, kind: &str, package: &str) -> bool {
        let before = self.contents.clone();
        if !self.remove_sub_node(kind, package) {
            return false;
        }
        if before != self.contents {
            self.remove_main_key_if_empty(kind);
        }
        true
    }

    /// `removeSubNode` for a link section.
    fn remove_sub_node(&mut self, main: &str, name: &str) -> bool {
        let decoded = self.decoded();
        let node = decoded.get(main);
        // PHP's `empty()`.
        let empty = match node {
            None | Some(Value::Null) => true,
            Some(Value::Bool(b)) => !b,
            Some(Value::Number(n)) => n.as_f64() == Some(0.0),
            Some(Value::String(s)) => s.is_empty() || s == "0",
            Some(Value::Array(a)) => a.is_empty(),
            Some(Value::Object(o)) => o.is_empty(),
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

    /// `removeMainKeyIfEmpty`, called only after `removeSubNode` edited the key, so its value is an object.
    fn remove_main_key_if_empty(&mut self, key: &str) {
        if matches!(self.decoded().get(key), Some(Value::Object(o)) if o.is_empty()) {
            self.remove_main_key(key);
        }
    }

    /// `removeMainKey` for a key `removeSubNode` just matched with a stricter pattern, so this one matches too.
    fn remove_main_key(&mut self, key: &str) {
        let re = pattern(
            &format!(
                r"^(?P<start>{BEFORE})(?P<removal>{}\s*:\s*(?&json))\s*,?\s*(?P<end>.*)",
                quote(&encode(key))
            ),
            true,
            false,
        );
        let m = (re.captures(self.contents.as_bytes()).ok().flatten())
            .expect("removeSubNode matched the key");
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
    let mut edit = Manipulator::new(text);
    if edit.add_link(section, name, constraint, sort) && edit.remove_link(other, name) {
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
    Ok(write_json(m, &edit.indent))
}

/// composer.json's text without `name` in `section` (and without `section` when that empties it), as
/// `composer remove` edits it.
pub fn json_remove(text: &str, section: &str, name: &str) -> Result<String> {
    let mut m = parse_json(text)?;
    let mut edit = Manipulator::new(text);
    if edit.remove_link(section, name) {
        return Ok(edit.text());
    }
    if let Some(Value::Object(o)) = m.get_mut(section) {
        o.shift_remove(name);
        if o.is_empty() {
            m.shift_remove(section);
        }
    }
    Ok(write_json(m, &edit.indent))
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

    /// `JsonManipulator` cases: `linkProvider`, `providerAddLinkAndSortPackages` and `provideRemoveLink` from
    /// Composer 2.11.2's `JsonManipulatorTest`, then what Composer 2.11.2's manipulator returns for the rest.
    #[test]
    fn manipulates_as_composer() {
        type Edit = fn(&mut Manipulator) -> bool;
        let unsorted = "{\n    \"require\": {\n        \"vendor/baz\": \"qux\"\n    }\n}";
        let cases: [(&str, Edit, bool, &str); 11] = [
            (
                "{\n    \"require\": {\n    }\n}",
                |m| m.add_link("require", "vendor/baz", "qux", false),
                true,
                "{\n    \"require\": {\n        \"vendor/baz\": \"qux\"\n    }\n}\n",
            ),
            (
                unsorted,
                |m| m.add_link("require", "foo", "bar", true),
                true,
                "{\n    \"require\": {\n        \"foo\": \"bar\",\n        \"vendor/baz\": \"qux\"\n    }\n}\n",
            ),
            (
                unsorted,
                |m| m.add_link("require", "foo", "bar", false),
                true,
                "{\n    \"require\": {\n        \"vendor/baz\": \"qux\",\n        \"foo\": \"bar\"\n    }\n}\n",
            ),
            (
                "{\n    \"require\": {\n        \"foo\": \"baz\",\n        \"ext-10gd\": \"*\",\n        \"ext-2mcrypt\": \"*\",\n        \"lib-foo\": \"*\",\n        \"hhvm\": \"*\",\n        \"php\": \">=5.5\"\n    }\n}",
                |m| m.add_link("require", "igorw/retry", "*", true),
                true,
                "{\n    \"require\": {\n        \"php\": \">=5.5\",\n        \"hhvm\": \"*\",\n        \"ext-2mcrypt\": \"*\",\n        \"ext-10gd\": \"*\",\n        \"lib-foo\": \"*\",\n        \"foo\": \"baz\",\n        \"igorw/retry\": \"*\"\n    }\n}\n",
            ),
            (
                "{\n    \"require\": {\n        \"foo/bar\": \"^1\"\n    },\n    \"require-dev\": {\n        \"baz/qux\": \"^1\"\n    }\n}",
                |m| m.remove_link("require-dev", "baz/qux"),
                true,
                "{\n    \"require\": {\n        \"foo/bar\": \"^1\"\n    }\n}\n",
            ),
            (
                "{\n    \"require-dev\": {\n        \"baz/qux\": \"^1\",\n        \"foo/bar\": \"^1\"\n    }\n}",
                |m| m.remove_link("require-dev", "baz/qux"),
                true,
                "{\n    \"require-dev\": {\n        \"foo/bar\": \"^1\"\n    }\n}\n",
            ),
            (
                "{\n    \"require\": {\n        \"foo/bar\": \"^1\"\n    },\n    \"require-dev\": {\n    }\n}",
                |m| m.remove_link("require-dev", "baz/qux"),
                true,
                "{\n    \"require\": {\n        \"foo/bar\": \"^1\"\n    },\n    \"require-dev\": {\n    }\n}\n",
            ),
            // A null section is `isset`-absent, so Composer sets it as a new key, in its own layout.
            (
                "{\n    \"name\": \"foo/bar\",\n    \"require\" :  null\n}",
                |m| m.add_link("require", "foo/baz", "^1", true),
                true,
                "{\n    \"name\": \"foo/bar\",\n    \"require\": {\n        \"foo/baz\": \"^1\"\n    }\n}\n",
            ),
            // Sorting re-lays the links out with `format`, which writes every decoded array inline.
            (
                "{\n    \"require\": {\n        \"b/b\": [],\n        \"a/a\": [1, {\"0\": 2}, {\"k\": [3]}],\n        \"c/c\": {\"x\": {}}\n    }\n}",
                |m| m.add_link("require", "php", "^8.4", true),
                true,
                "{\n    \"require\": {\n        \"php\": \"^8.4\",\n        \"a/a\": [1, [2], {\n                \"k\": [3]\n            }],\n        \"b/b\": [],\n        \"c/c\": {\n            \"x\": []\n        }\n    }\n}\n",
            ),
            (
                "{\n    \"require\": [{\"a/b\": \"1\"}]\n}",
                |m| m.add_link("require", "a/b", "2", true),
                true,
                "{\n    \"require\": [{\n            \"a/b\": \"2\"\n        }]\n}\n",
            ),
            // The constraint goes in unescaped, so the links no longer decode: Composer throws a TypeError.
            (
                "{\n    \"require\": {\n        \"a/b\": \"1\"\n    }\n}",
                |m| m.add_link("require", "a/b", "2\"", true),
                false,
                "",
            ),
        ];
        for (text, edit, ok, expected) in cases {
            let mut m = Manipulator::new(text);
            assert_eq!(edit(&mut m), ok, "{text}");
            if ok {
                assert_eq!(m.text(), expected, "{text}");
            }
        }
        // `removeSubNode` on a section PHP's `empty()` holds empty leaves the file as it is and succeeds;
        // any other value that isn't an object can't be matched.
        for (value, ok) in [
            ("[]", true),
            ("\"0\"", true),
            ("\"\"", true),
            ("0", true),
            ("0.0", true),
            ("false", true),
            ("null", true),
            ("[1]", false),
            ("\"x\"", false),
            ("true", false),
            ("1", false),
        ] {
            let text = format!("{{\n    \"require-dev\": {value}\n}}");
            let mut m = Manipulator::new(&text);
            assert_eq!(m.remove_sub_node("require-dev", "a/b"), ok, "{value}");
            assert_eq!(m.text(), format!("{text}\n"));
        }
    }

    /// The text Composer 2.11.2's `require -n --no-update` and `remove -n --no-update` write for the same
    /// composer.json.
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
        // Composer rejects both values against its schema; the rewrite can't put a link into a string.
        let strings = "{\"require\": \"x\", \"require-dev\": \"y\"}";
        assert!(json_set(strings, "require", "require-dev", "a/b", "*", false).is_err());
    }

    /// `composer require [--dev] name:constraint` and `composer remove [--dev] name`, run with Composer 2.11.2.
    #[test]
    fn edits_other_layouts_as_composer() {
        // From the fourth case on, most can't be edited in place: a key written with an escape, or a section
        // given twice, which Composer decodes to the last one. Composer rewrites those files with their indent,
        // PHP's empty arrays, lists and floats, and `{}` only where `restoreEmptyObjects` puts it back.
        let dups = "{\n\t\"name\": \"acme/app\",\n\t\"require\": {\n\t\t\"acme/b\": \"1.0\"\n\t},\n\t\"require-dev\": {\n\t\t\"acme/c\": \"^2.0\"\n\t},\n\t\"require-dev\": {\n\t\t\"acme/d\": \"^3.0\"\n\t},\n\t\"extra\": {\"a\": {}, \"b\": {\"0\": \"x\"}, \"c\": 1.0, \"d\": \"\\u00e9\\/\"},\n\t\"autoload\": {\"psr-4\": {}},\n\t\"config\": {\"platform\": {}, \"policy\": {\"advisories\": {}}},\n\t\"support\": {}\n}\n";
        let rewritten = "{\n\t\"name\": \"acme/app\",\n\t\"require\": {\n\t\t\"acme/b\": \"1.0\",\n\t\t\"acme/d\": \"^3.1\"\n\t},\n\t\"extra\": {\n\t\t\"a\": [],\n\t\t\"b\": [\n\t\t\t\"x\"\n\t\t],\n\t\t\"c\": 1,\n\t\t\"d\": \"é/\"\n\t},\n\t\"autoload\": {\n\t\t\"psr-4\": {}\n\t},\n\t\"config\": {\n\t\t\"platform\": {},\n\t\t\"policy\": {\n\t\t\t\"advisories\": {}\n\t\t}\n\t},\n\t\"support\": {}\n}\n";
        let removed = rewritten.replace(",\n\t\t\"acme/d\": \"^3.1\"", "");
        let cases: [(&str, &str, &str, Option<&str>, &str); 10] = [
            (
                "{\r\n    \"name\": \"acme/app\",\r\n    \"require\": {\r\n        \"acme/b\": \"1.0\"\r\n    }\r\n}\r\n",
                "require",
                "acme/c",
                Some("^2.0"),
                "{\r\n    \"name\": \"acme/app\",\r\n    \"require\": {\r\n        \"acme/b\": \"1.0\",\r\n        \"acme/c\": \"^2.0\"\r\n    }\r\n}\r\n",
            ),
            (
                "{\n    \"name\": \"acme/app\"\n}\n",
                "require",
                "acme/b",
                Some("^1.0"),
                "{\n    \"name\": \"acme/app\",\n    \"require\": {\n        \"acme/b\": \"^1.0\"\n    }\n}\n",
            ),
            (
                "{\n    \"require\": {\n        \"acme/b\": \"^1.0\"\n    }\n}\n",
                "require",
                "acme/b",
                None,
                "{\n}\n",
            ),
            (
                "{\n  \"name\": \"acme/app\",\n  \"requir\\u0065\": {\n    \"acme/b\": \"1.0\"\n  },\n  \"extra\": {\"a\": {}, \"b\": {\"0\": \"x\"}, \"c\": 1.0},\n  \"autoload\": {\"psr-4\": {}}\n}\n",
                "require",
                "acme/c",
                Some("^2.1"),
                "{\n  \"name\": \"acme/app\",\n  \"require\": {\n    \"acme/b\": \"1.0\",\n    \"acme/c\": \"^2.1\"\n  },\n  \"extra\": {\n    \"a\": [],\n    \"b\": [\n      \"x\"\n    ],\n    \"c\": 1\n  },\n  \"autoload\": {\n    \"psr-4\": {}\n  }\n}\n",
            ),
            (
                "{\n    \"name\": \"acme/app\",\n    \"require-d\\u0065v\": {\n        \"acme/c\": \"^2.0\",\n        \"acme/d\": \"^3.0\"\n    }\n}\n",
                "require-dev",
                "acme/c",
                None,
                "{\n    \"name\": \"acme/app\",\n    \"require-dev\": {\n        \"acme/d\": \"^3.0\"\n    }\n}\n",
            ),
            (dups, "require", "acme/d", Some("^3.1"), rewritten),
            (dups, "require-dev", "acme/d", None, &removed),
            // `--dev`: the first `require` holds the name, so Composer edits it in place, and keeps it as the
            // section Composer decodes is the second, still not empty.
            (
                "{\n    \"keywords\": [\"a\"],\n    \"require\": {\n        \"acme/d\": \"^3.0\"\n    },\n    \"require\": {\n        \"acme/d\": \"^3.0\",\n        \"acme/e\": \"^1.0\"\n    }\n}\n",
                "require-dev",
                "acme/d",
                Some("^3.1"),
                "{\n    \"keywords\": [\"a\"],\n    \"require\": {\n    },\n    \"require\": {\n        \"acme/d\": \"^3.0\",\n        \"acme/e\": \"^1.0\"\n    },\n    \"require-dev\": {\n        \"acme/d\": \"^3.1\"\n    }\n}\n",
            ),
            // It doesn't, so Composer rewrites the file.
            (
                "{\n    \"keywords\": [\"a\", \"b\"],\n    \"require\": {\n        \"acme/c\": \"^2.0\"\n    },\n    \"require\": {\n        \"acme/d\": \"^3.0\",\n        \"acme/e\": \"^1.0\"\n    }\n}\n",
                "require-dev",
                "acme/d",
                Some("^3.1"),
                "{\n    \"keywords\": [\n        \"a\",\n        \"b\"\n    ],\n    \"require\": {\n        \"acme/e\": \"^1.0\"\n    },\n    \"require-dev\": {\n        \"acme/d\": \"^3.1\"\n    }\n}\n",
            ),
            // Composer's schema rejects this file on load; `JsonConfigSource::removeLink`, run on it, rewrites it.
            (
                "{\n  \"require\": [1, {\"x\": {}}]\n}\n",
                "require",
                "a/b",
                None,
                "{\n  \"require\": [\n    1,\n    {\n      \"x\": []\n    }\n  ]\n}\n",
            ),
        ];
        for (text, section, name, constraint, expected) in cases {
            let edited = match constraint {
                Some(c) => {
                    let other = if section == "require" {
                        "require-dev"
                    } else {
                        "require"
                    };
                    json_set(text, section, other, name, c, false)
                }
                None => json_remove(text, section, name),
            };
            assert_eq!(edited.unwrap(), expected, "{text}");
        }
    }
}
