use crate::error::{Result, bail};
use pcre2::bytes::{Captures, Regex};
use std::cmp::Ordering;
use std::sync::LazyLock;

pub(crate) const MODIFIER: &str =
    r"[._-]?(?:(stable|beta|b|RC|alpha|a|patch|pl|p)((?:[.-]?\d+)*+)?)?([.-]?dev)?";

pub(crate) fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("valid pattern")
}

pub(crate) fn group<'a>(c: &'a Captures, i: usize) -> &'a str {
    c.get(i)
        .map_or("", |m| std::str::from_utf8(m.as_bytes()).unwrap_or(""))
}

pub(crate) static ALIAS: LazyLock<Regex> = LazyLock::new(|| re(r"^([^,\s]++) ++as ++([^,\s]++)$"));
static FLAG: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)@(?:stable|RC|beta|alpha|dev)$"));
static META: LazyLock<Regex> = LazyLock::new(|| re(r"^([^,\s+]++)\+[^\s]++$"));
static CLASSIC: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?i)^v?(\d{{1,5}}+)(\.\d++)?(\.\d++)?(\.\d++)?{MODIFIER}$"
    ))
});
static DATE: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"(?i)^v?(\d{{4}}(?:[.:-]?\d{{2}}){{1,6}}(?:[.:-]?\d{{1,3}}){{0,2}}){MODIFIER}$"
    ))
});
static DEV: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)(.*?)[.-]?dev$"));
static BRANCH: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)^v?(\d++)(\.(?:\d++|[xX*]))?(\.(?:\d++|[xX*]))?(\.(?:\d++|[xX*]))?$")
});
static NUMERIC_ALIAS: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^((\d++\.)*\d++)(?:\.x)?-dev$"));

fn stability(s: &str) -> String {
    match s.to_ascii_lowercase().as_str() {
        "a" => "alpha".into(),
        "b" => "beta".into(),
        "p" | "pl" => "patch".into(),
        "rc" => "RC".into(),
        other => other.into(),
    }
}

pub fn normalize(version: &str) -> Result<String> {
    let original = version.trim();
    let mut v = original.to_string();
    if let Some(c) = ALIAS.captures(v.as_bytes())? {
        v = group(&c, 1).to_string();
    }
    if let Some(m) = FLAG.find(v.as_bytes())? {
        v.truncate(m.start());
    }
    if matches!(v.as_str(), "master" | "trunk" | "default") {
        v = format!("dev-{v}");
    }
    if v.len() >= 4 && v[..4].eq_ignore_ascii_case("dev-") {
        return Ok(format!("dev-{}", &v[4..]));
    }
    if let Some(c) = META.captures(v.as_bytes())? {
        v = group(&c, 1).to_string();
    }
    let matched = if let Some(c) = CLASSIC.captures(v.as_bytes())? {
        let part = |i| match group(&c, i) {
            "" => ".0".to_string(),
            p => p.to_string(),
        };
        Some((
            format!("{}{}{}{}", group(&c, 1), part(2), part(3), part(4)),
            c,
            5,
        ))
    } else if let Some(c) = DATE.captures(v.as_bytes())? {
        let base = group(&c, 1)
            .chars()
            .map(|ch| if ch.is_ascii_digit() { ch } else { '.' })
            .collect();
        Some((base, c, 2))
    } else {
        None
    };
    if let Some((mut out, c, i)) = matched {
        let modifier = group(&c, i);
        if !modifier.is_empty() {
            if modifier == "stable" {
                return Ok(out);
            }
            out = format!(
                "{out}-{}{}",
                stability(modifier),
                group(&c, i + 1).trim_start_matches(['.', '-'])
            );
        }
        if !group(&c, i + 2).is_empty() {
            out.push_str("-dev");
        }
        return Ok(out);
    }
    if let Some(c) = DEV.captures(v.as_bytes())? {
        let branch = normalize_branch(group(&c, 1));
        if !branch.starts_with("dev-") {
            return Ok(branch);
        }
    }
    bail!("Invalid version string \"{original}\"")
}

static STABILITY_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| re(&format!(r"(?i){MODIFIER}(?:\+.*)?$")));

pub fn stability_of(version: &str) -> &'static str {
    let version = version.split_once('#').map_or(version, |(v, _)| v);
    if version.starts_with("dev-") || version.ends_with("-dev") {
        return "dev";
    }
    if version.bytes().all(|c| c.is_ascii_digit() || c == b'.') {
        return "stable";
    }
    let lower = version.to_ascii_lowercase();
    let m = (STABILITY_SUFFIX.captures(lower.as_bytes()).ok().flatten())
        .expect("an empty suffix matches");
    if !group(&m, 3).is_empty() {
        return "dev";
    }
    match group(&m, 1) {
        "beta" | "b" => "beta",
        "alpha" | "a" => "alpha",
        "rc" => "RC",
        _ => "stable",
    }
}

pub fn normalize_branch(name: &str) -> String {
    let name = name.trim();
    let Ok(Some(c)) = BRANCH.captures(name.as_bytes()) else {
        return format!("dev-{name}");
    };
    let mut v = String::new();
    for i in 1..5 {
        match c.get(i) {
            Some(_) => v.push_str(&group(&c, i).replace(['*', 'X'], "x")),
            None => v.push_str(".x"),
        }
    }
    format!("{}-dev", v.replace('x', "9999999"))
}

pub fn branch_alias(
    package: &serde_json::Map<String, serde_json::Value>,
) -> Option<(String, String)> {
    let version = package.get("version")?.as_str()?;
    if !version.starts_with("dev-") && !version.ends_with("-dev") {
        return None;
    }
    let numeric = |b: &str| -> Option<String> {
        let c = NUMERIC_ALIAS.captures(b.as_bytes()).ok()??;
        Some(format!("{}.", group(&c, 1)))
    };
    let aliases = (package.get("extra"))
        .and_then(|e| e.get("branch-alias"))
        .and_then(|a| a.as_object());
    for (source, target) in aliases.into_iter().flatten() {
        let Some(target) = target.as_str() else {
            continue;
        };
        let Some(stem) = target.strip_suffix("-dev") else {
            continue;
        };
        let validated = if target == "9999999-dev" {
            target.to_string()
        } else {
            normalize_branch(stem)
        };
        if !validated.ends_with("-dev") || !version.eq_ignore_ascii_case(source) {
            continue;
        }
        if let (Some(s), Some(t)) = (numeric(source), numeric(target))
            && !t.to_ascii_lowercase().starts_with(&s.to_ascii_lowercase())
        {
            continue;
        }
        return Some(pretty_alias(validated));
    }
    let default = package
        .get("default-branch")
        .and_then(serde_json::Value::as_bool)
        == Some(true);
    (default && numeric(version.trim_start_matches('v')).is_none())
        .then(|| pretty_alias("9999999-dev".into()))
}

fn pretty_alias(normalized: String) -> (String, String) {
    static NINES: LazyLock<Regex> = LazyLock::new(|| re(r"(\.9{7})+"));
    let mut pretty = String::new();
    let mut at = 0;
    for m in NINES.find_iter(normalized.as_bytes()).flatten() {
        pretty.push_str(&normalized[at..m.start()]);
        pretty.push_str(".x");
        at = m.end();
    }
    pretty.push_str(&normalized[at..]);
    (normalized, pretty)
}

/// The platform requirements `--ignore-platform-req` and `--ignore-platform-reqs` leave out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IgnorePlatform {
    /// The requirements matching one of these names or `*` globs.
    Listed(Vec<String>),
    All,
}

impl Default for IgnorePlatform {
    fn default() -> IgnorePlatform {
        IgnorePlatform::Listed(vec![])
    }
}

impl IgnorePlatform {
    pub fn new(listed: Vec<String>, all: bool) -> IgnorePlatform {
        if all {
            IgnorePlatform::All
        } else {
            IgnorePlatform::Listed(listed)
        }
    }

    pub fn ignores(&self, name: &str) -> bool {
        match self {
            IgnorePlatform::All => true,
            IgnorePlatform::Listed(patterns) => patterns.iter().any(|p| name_matches(p, name)),
        }
    }
}

pub fn name_matches(pattern: &str, name: &str) -> bool {
    let (pattern, name) = (pattern.to_ascii_lowercase(), name.to_ascii_lowercase());
    let mut parts = pattern.split('*');
    let first = parts.next().unwrap_or_default();
    let Some(mut rest) = name.strip_prefix(first) else {
        return false;
    };
    let parts: Vec<&str> = parts.collect();
    for (i, part) in parts.iter().enumerate() {
        if i + 1 == parts.len() {
            return rest.len() >= part.len() && rest.ends_with(part);
        }
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    rest.is_empty()
}

pub fn is_platform(name: &str) -> bool {
    static PLATFORM: LazyLock<Regex> = LazyLock::new(|| {
        re(
            r"(?i)^(?:php(?:-64bit|-ipv6|-zts|-debug)?|hhvm|(?:ext|lib)-[a-z0-9](?:[_.-]?[a-z0-9]+)*|composer(?:-(?:plugin|runtime)-api)?)$",
        )
    });
    PLATFORM.is_match(name.as_bytes()).unwrap_or(false)
}

pub fn compare(a: &str, b: &str) -> i32 {
    if a.is_empty() || b.is_empty() {
        return match (a.is_empty(), b.is_empty()) {
            (true, true) => 0,
            (true, false) => -1,
            _ => 1,
        };
    }
    let (mut stack_a, mut stack_b) = ([0u8; 64], [0u8; 64]);
    let (mut heap_a, mut heap_b) = (vec![], vec![]);
    let a = canonical(a, &mut stack_a, &mut heap_a);
    let b = canonical(b, &mut stack_b, &mut heap_b);
    let (mut p1, mut p2) = (0usize, 0usize);
    let (mut n1, mut n2): (Option<usize>, Option<usize>) = (Some(0), Some(0));
    let mut result = 0;
    let dot = |s: &[u8], from: usize| s[from..].iter().position(|&c| c == b'.').map(|i| from + i);
    while p1 < a.len() && p2 < b.len() && n1.is_some() && n2.is_some() {
        n1 = dot(a, p1);
        n2 = dot(b, p2);
        let s1 = &a[p1..n1.unwrap_or(a.len())];
        let s2 = &b[p2..n2.unwrap_or(b.len())];
        result = segment(s1, s2);
        if result != 0 {
            break;
        }
        if let Some(n) = n1 {
            p1 = n + 1;
        }
        if let Some(n) = n2 {
            p2 = n + 1;
        }
    }
    if result == 0 {
        if n1.is_some() {
            result = if a.get(p1).is_some_and(u8::is_ascii_digit) {
                1
            } else {
                compare(&String::from_utf8_lossy(&a[p1..]), "#N#")
            };
        } else if n2.is_some() {
            result = if b.get(p2).is_some_and(u8::is_ascii_digit) {
                -1
            } else {
                compare("#N#", &String::from_utf8_lossy(&b[p2..]))
            };
        }
    }
    result
}

fn segment(s1: &[u8], s2: &[u8]) -> i32 {
    let digit = |s: &[u8]| s.first().is_some_and(u8::is_ascii_digit);
    let number = |s: &[u8]| -> i64 {
        s.iter()
            .take_while(|c| c.is_ascii_digit())
            .fold(0i64, |n, &c| {
                n.saturating_mul(10).saturating_add(i64::from(c - b'0'))
            })
    };
    match (digit(s1), digit(s2)) {
        (true, true) => number(s1).cmp(&number(s2)) as i32,
        (false, false) => rank(s1).cmp(&rank(s2)) as i32,
        (true, false) => rank(b"#N#").cmp(&rank(s2)) as i32,
        (false, true) => rank(s1).cmp(&rank(b"#N#")) as i32,
    }
}

fn rank(form: &[u8]) -> i32 {
    const FORMS: [(&[u8], i32); 10] = [
        (b"dev", 0),
        (b"alpha", 1),
        (b"a", 1),
        (b"beta", 2),
        (b"b", 2),
        (b"RC", 3),
        (b"rc", 3),
        (b"#", 4),
        (b"pl", 5),
        (b"p", 5),
    ];
    FORMS
        .iter()
        .find(|(name, _)| form.starts_with(name))
        .map_or(-1, |f| f.1)
}

fn canonical<'b>(v: &'b str, stack: &'b mut [u8], heap: &'b mut Vec<u8>) -> &'b [u8] {
    let v = v.as_bytes();
    if v[0] == b'#' {
        return v;
    }
    let out: &mut [u8] = if v.len() * 2 <= stack.len() {
        stack
    } else {
        heap.resize(v.len() * 2, 0);
        heap
    };
    out[0] = v[0];
    let mut len = 1;
    let mut last = v[0];
    let digit = |c: u8| c.is_ascii_digit() && c != b'.';
    let other = |c: u8| !c.is_ascii_digit() && c != b'.';
    for &c in &v[1..] {
        let mut dot = |out: &mut [u8]| {
            if out[len - 1] != b'.' {
                out[len] = b'.';
                len += 1;
            }
        };
        if matches!(c, b'-' | b'_' | b'+') {
            dot(out);
        } else if (other(last) && digit(c)) || (digit(last) && other(c)) {
            dot(out);
            out[len] = c;
            len += 1;
        } else if !c.is_ascii_alphanumeric() {
            dot(out);
        } else {
            out[len] = c;
            len += 1;
        }
        last = c;
    }
    if out[len - 1] == b'.' {
        len -= 1;
    }
    &out[..len]
}

pub fn ordering(a: &str, b: &str) -> Ordering {
    compare(a, b).cmp(&0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ignored_platform_requirements() {
        let listed = IgnorePlatform::new(vec!["ext-*".into()], false);
        assert!(listed.ignores("ext-json") && !listed.ignores("php"));
        assert!(IgnorePlatform::new(vec![], true).ignores("php"));
        assert!(!IgnorePlatform::default().ignores("php"));
    }

    #[test]
    fn normalize_matches_composer() {
        for (input, want) in [
            ("1.0.0", "1.0.0.0"),
            ("1.2.3.4", "1.2.3.4"),
            ("1.0.0RC1dev", "1.0.0.0-RC1-dev"),
            ("1.0.0-rC15-dev", "1.0.0.0-RC15-dev"),
            ("1.0.0.RC.15-dev", "1.0.0.0-RC15-dev"),
            ("1.0.0.pl3-dev", "1.0.0.0-patch3-dev"),
            ("1.0-dev", "1.0.0.0-dev"),
            ("10.4.13-b5", "10.4.13.0-beta5"),
            ("v1.13.11-beta.0", "1.13.11.0-beta0"),
            ("v1.0.0", "1.0.0.0"),
            ("2010.01.02", "2010.01.02.0"),
            ("20100102-203040", "20100102.203040"),
            ("2010-01-02-10-20-30.5", "2010.01.02.10.20.30.5"),
            ("20100102.x-dev", "20100102.9999999.9999999.9999999-dev"),
            ("master", "dev-master"),
            ("DEV-FOOBAR", "dev-FOOBAR"),
            ("1.x-dev", "1.9999999.9999999.9999999-dev"),
            ("dev-master as 1.0.0", "dev-master"),
            ("1.0.0+foo@dev", "1.0.0.0"),
            ("1.0.0-alpha.3.1+foo", "1.0.0.0-alpha3.1"),
            ("1.0.0-stable", "1.0.0.0"),
            ("041.x-dev", "041.9999999.9999999.9999999-dev"),
            (" 1.0.0", "1.0.0.0"),
            ("1.0.0a1", "1.0.0.0-alpha1"),
        ] {
            assert_eq!(normalize(input).unwrap(), want, "{input}");
        }
        for bad in [
            "",
            "a",
            "1.0.0-meh",
            "1.0.0.0.0",
            "feature-foo",
            "1.0.1-SNAPSHOT",
            "foo bar-dev",
            "^1",
        ] {
            assert!(normalize(bad).is_err(), "{bad}");
        }
        assert_eq!(
            normalize("1.0.0-meh").unwrap_err().to_string(),
            "Invalid version string \"1.0.0-meh\""
        );
    }

    #[test]
    fn branch_aliases_match_composer() {
        let alias = |v: serde_json::Value| branch_alias(v.as_object().expect("an object"));
        let pair = |a: &str, b: &str| Some((a.to_string(), b.to_string()));
        assert_eq!(
            alias(
                json!({"version": "dev-main", "extra": {"branch-alias": {"dev-main": "2.x-dev"}}})
            ),
            pair("2.9999999.9999999.9999999-dev", "2.x-dev")
        );
        assert_eq!(
            alias(json!({"version": "dev-main", "default-branch": true})),
            pair("9999999-dev", "9999999-dev")
        );
        assert_eq!(
            alias(json!({"version": "5.x-dev", "default-branch": true})),
            None
        );
        assert_eq!(
            alias(json!({"version": "1.0.0", "extra": {"branch-alias": {"1.0.0": "1.x-dev"}}})),
            None
        );
        assert_eq!(
            alias(
                json!({"version": "2.x-dev", "extra": {"branch-alias": {"2.x-dev": "3.0.x-dev"}}})
            ),
            None
        );
        assert_eq!(
            alias(json!({"version": "dev-a", "extra": {"branch-alias": {"dev-a": "1.0", "x": 1}}})),
            None
        );
        assert_eq!(
            alias(json!({"version": "dev-a", "extra": {"branch-alias": {"dev-a": "9999999-dev"}}})),
            pair("9999999-dev", "9999999-dev")
        );
    }

    #[test]
    fn compare_matches_php() {
        for (a, b, want) in [
            ("8.4.0.0-dev", "8.4.0.0", -1),
            ("8.4.1.0-dev", "8.4.0.0", 1),
            ("1.0", "1.0.0", -1),
            ("1.0.0", "1.0", 1),
            ("1.0-dev", "1.0", -1),
            ("1.0", "1.0-dev", 1),
            ("1.0rc1", "1.0beta2", 1),
            ("1.0pl1", "1.0", 1),
            ("1.0.x", "1.0.dev", -1),
            ("1_0", "1-0", 0),
            ("", "1", -1),
            ("1", "", 1),
            ("", "", 0),
            ("1.0.1", "1.0.rc", 1),
            ("1.0.rc", "1.0.1", -1),
            ("1.", "1.", 0),
            ("1.", "1.0", -1),
            ("e#", "x.y.z", 1),
            ("#N#", "dev", 1),
            (
                "1.2.3.4.5.6.7.8.9.10.11.12.13.14.15.16.17.18.19.20-dev",
                "1.2.3.4.5.6.7.8.9.10.11.12.13.14.15.16.17.18.19.20",
                -1,
            ),
        ] {
            assert_eq!(compare(a, b), want, "{a} {b}");
        }
        assert_eq!(ordering("2", "10"), Ordering::Less);
    }

    #[test]
    fn stability_matches_composer() {
        for (v, want) in [
            ("1.0.0", "stable"),
            ("1.0.0.0-beta1", "beta"),
            ("1.0b2", "beta"),
            ("1.0.0-alpha", "alpha"),
            ("1.0a1", "alpha"),
            ("1.0.0-RC2", "RC"),
            ("1.0.0-patch1", "stable"),
            ("1.0.0.0-dev", "dev"),
            ("dev-main", "dev"),
            ("1.0.x-dev#abc", "dev"),
            ("1.0.0dev", "dev"),
            ("1.0.0-beta1#abc", "beta"),
            ("foo bar", "stable"),
        ] {
            assert_eq!(stability_of(v), want, "{v}");
        }
    }

    #[test]
    fn platform_names() {
        assert!(
            is_platform("php") && is_platform("ext-mbstring") && is_platform("composer-plugin-api")
        );
        assert!(!is_platform("laravel/framework") && !is_platform("ext-"));
    }
}
