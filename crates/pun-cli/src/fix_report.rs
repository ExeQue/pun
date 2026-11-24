use crate::console::escape;
use std::fmt::Write as _;
use std::sync::LazyLock;

pub(crate) struct Change<'a> {
    pub(crate) file: &'a str,
    pub(crate) fixers: &'a [&'a str],
    pub(crate) diff: &'a str,
}

pub(crate) const ABOUT: &str =
    "PHP CS Fixer 3.95.25 Adalbertus by Fabien Potencier, Dariusz Ruminski and contributors.";

pub(crate) fn json(changes: &[Change], verbose: bool, decorated: bool) -> String {
    let files: Vec<serde_json::Value> = changes
        .iter()
        .map(|c| {
            let mut f = serde_json::json!({ "name": c.file });
            if verbose {
                f["appliedFixers"] = serde_json::json!(c.fixers);
            }
            if !c.diff.is_empty() {
                f["diff"] = serde_json::json!(c.diff);
            }
            f
        })
        .collect();
    let json = php_json(
        &serde_json::json!({ "about": ABOUT, "files": files, "time": { "total": 0 }, "memory": 0 }),
    );
    if decorated { escape(&json) } else { json }
}

pub(crate) fn agent(changes: &[Change], errors: &[(&str, &str)], dry_run: bool) -> String {
    let result = if !errors.is_empty() {
        "fail"
    } else if changes.is_empty() {
        "passed"
    } else if dry_run {
        "fail"
    } else {
        "fixed"
    };
    let mut v = serde_json::json!({ "tool": "pint", "result": result });
    if !changes.is_empty() {
        v["files"] = (changes.iter())
            .map(|c| serde_json::json!({ "path": c.file, "fixers": c.fixers }))
            .collect();
    }
    if !errors.is_empty() {
        v["errors"] = (errors.iter())
            .map(|(path, message)| serde_json::json!({ "path": path, "message": message }))
            .collect();
    }
    php_json(&v)
}

pub(crate) fn php_json(v: &serde_json::Value) -> String {
    let s = serde_json::to_string(v).expect("json");
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '/' => o.push_str("\\/"),
            c if c.is_ascii() => o.push(c),
            c => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    let _ = write!(o, "\\u{u:04x}");
                }
            }
        }
    }
    o
}

struct El {
    name: &'static str,
    attrs: Vec<(&'static str, String)>,
    children: Vec<El>,
    cdata: Option<String>,
}

fn el(name: &'static str, attrs: &[(&'static str, &str)]) -> El {
    El {
        name,
        attrs: attrs.iter().map(|(k, v)| (*k, v.to_string())).collect(),
        children: vec![],
        cdata: None,
    }
}

impl El {
    fn write(&self, o: &mut String, depth: usize) {
        let pad = "  ".repeat(depth);
        let _ = write!(o, "{pad}<{}", self.name);
        for (k, v) in &self.attrs {
            let v = xml_chars(v)
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
                .replace('"', "&quot;");
            let _ = write!(
                o,
                " {k}=\"{}\"",
                v.replace('\t', "&#9;")
                    .replace('\n', "&#10;")
                    .replace('\r', "&#13;")
            );
        }
        if let Some(c) = &self.cdata {
            let _ = writeln!(
                o,
                "><![CDATA[{}]]></{}>",
                xml_chars(c).replace("]]>", "]]]]><![CDATA[>"),
                self.name
            );
        } else if self.children.is_empty() {
            o.push_str("/>\n");
        } else {
            o.push_str(">\n");
            for c in &self.children {
                c.write(o, depth + 1);
            }
            let _ = writeln!(o, "{pad}</{}>", self.name);
        }
    }
}

/// `text` with each character XML 1.0 forbids (a control character other than tab and the line breaks, U+FFFE,
/// U+FFFF) as U+FFFD: one in a file name or a diff would make the whole report unreadable.
pub(crate) fn xml_chars(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\t' | '\n' | '\r' => c,
            '\0'..='\x1f' | '\u{FFFE}' | '\u{FFFF}' => '\u{FFFD}',
            c => c,
        })
        .collect()
}

fn xml(root: &El, decorated: bool) -> String {
    let mut o = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    root.write(&mut o, 0);
    if decorated { escape(&o) } else { o }
}

pub(crate) fn checkstyle(changes: &[Change], decorated: bool) -> String {
    let mut root = el("checkstyle", &[("version", ABOUT)]);
    for c in changes {
        let mut file = el("file", &[("name", c.file)]);
        for f in c.fixers {
            file.children.push(el(
                "error",
                &[
                    ("severity", "warning"),
                    ("source", &format!("PHP-CS-Fixer.{f}")),
                    ("message", &format!("Found violation(s) of type: {f}")),
                ],
            ));
        }
        root.children.push(file);
    }
    xml(&root, decorated)
}

pub(crate) fn junit(changes: &[Change], verbose: bool, decorated: bool) -> String {
    let mut suite = el("testsuite", &[("name", "PHP CS Fixer")]);
    let mut props = el("properties", &[]);
    props
        .children
        .push(el("property", &[("name", "about"), ("value", ABOUT)]));
    suite.children.push(props);
    if changes.is_empty() {
        suite
            .children
            .push(el("testcase", &[("name", "All OK"), ("assertions", "1")]));
        suite.attrs.extend(
            [
                ("tests", "1"),
                ("assertions", "1"),
                ("failures", "0"),
                ("errors", "0"),
            ]
            .map(|(k, v)| (k, v.to_string())),
        );
    } else {
        let mut assertions = 0;
        for c in changes {
            let base = c.file.rsplit('/').next().unwrap_or_default();
            let ext = base.rfind('.').map_or("", |i| &base[i + 1..]);
            let name = c
                .file
                .strip_suffix(&format!(".{ext}"))
                .unwrap_or(c.file)
                .replace('.', "_DOT_");
            let mut case = el(
                "testcase",
                &[
                    ("name", &name),
                    ("file", c.file),
                    ("assertions", &c.fixers.len().to_string()),
                ],
            );
            let mut text = if verbose {
                "applied fixers:\n---------------\n".to_string()
                    + &c.fixers.iter().fold(String::new(), |mut out, f| {
                        let _ = writeln!(out, "* {f}");
                        out
                    })
            } else {
                "Wrong code style\n".to_string()
            };
            if !c.diff.is_empty() {
                let _ = write!(text, "\nDiff:\n---------------\n\n{}", c.diff);
            }
            let mut failure = el("failure", &[("type", "code_style")]);
            failure.cdata = Some(
                text.trim_matches([' ', '\t', '\n', '\r', '\0', '\x0b'])
                    .to_string(),
            );
            case.children.push(failure);
            suite.children.push(case);
            assertions += c.fixers.len();
        }
        let n = changes.len().to_string();
        let a = assertions.to_string();
        suite.attrs.extend(
            [
                ("tests", n.as_str()),
                ("assertions", &a),
                ("failures", &a),
                ("errors", "0"),
            ]
            .map(|(k, v)| (k, v.to_string())),
        );
    }
    let mut root = el("testsuites", &[]);
    root.children.push(suite);
    xml(&root, decorated)
}

/// `name => [summary, docs path]` for `GitlabReporter`'s `FixerFactory` (generated: scripts/fixer-metadata.php --gitlab).
static FIXERS: LazyLock<serde_json::Map<String, serde_json::Value>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("gitlab_fixers.json")).expect("gitlab_fixers.json")
});

pub(crate) fn gitlab(changes: &[Change], decorated: bool) -> String {
    let mut report = vec![];
    for c in changes {
        let lines = gitlab_lines(c.diff);
        for f in c.fixers {
            let (description, docs) = match FIXERS.get(*f) {
                Some(m) => (
                    m[0].as_str().unwrap_or_default().to_string(),
                    format!(
                        "Check [docs](https://cs.symfony.com/doc/rules/{}.html) for more information.",
                        m[1].as_str().unwrap_or_default()
                    ),
                ),
                None => (
                    format!("PHP-CS-Fixer.{f} (custom rule)"),
                    "Check performed with a custom rule.".to_string(),
                ),
            };
            report.push(serde_json::json!({
                "check_name": format!("PHP-CS-Fixer.{f}"),
                "description": description,
                "content": { "body": format!("{ABOUT}\n{docs}") },
                "categories": ["Style"],
                "fingerprint": md5(format!("{}{f}", c.file).as_bytes()),
                "severity": "minor",
                "location": { "path": c.file, "lines": { "begin": lines.0, "end": lines.1 } },
            }));
        }
    }
    let json = php_json(&serde_json::Value::Array(report));
    if decorated { escape(&json) } else { json }
}

fn gitlab_lines(diff: &str) -> (usize, usize) {
    let lines: Vec<&str> = diff
        .split("\r\n")
        .flat_map(|l| l.split(['\r', '\n']))
        .collect();
    let header = (0..lines.len().saturating_sub(1))
        .find(|&i| lines[i].starts_with("--- ") && lines[i + 1].starts_with("+++ "));
    let Some(h) = header.and_then(|i| {
        lines
            .iter()
            .skip(i + 2)
            .position(|l| l.starts_with("@@ -"))
            .map(|p| p + i + 2)
    }) else {
        return (0, 0);
    };
    let from = lines[h][4..].split(' ').next().unwrap_or_default();
    let (start, range) = from.split_once(',').unwrap_or((from, "1"));
    let (start, range): (usize, usize) = (start.parse().unwrap_or(0), range.parse().unwrap_or(1));
    let first = lines[h + 1..]
        .iter()
        .position(|l| l.starts_with(['+', '-']))
        .unwrap_or(0);
    (start + first, start + range)
}

fn md5(input: &[u8]) -> String {
    use md5::Digest;
    md5::Md5::digest(input)
        .iter()
        .fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A control character XML forbids, in a file name or a diff, doesn't reach the report.
    #[test]
    fn xml_reports_drop_forbidden_characters() {
        let fixers = ["array_syntax"];
        let changes = [Change {
            file: "a\x01\"b.php",
            fixers: &fixers,
            diff: "-\x1b[0m\n+x\u{FFFF}\n",
        }];
        assert!(checkstyle(&changes, false).contains("<file name=\"a\u{FFFD}&quot;b.php\">"));
        let junit = junit(&changes, true, false);
        assert!(junit.contains(" file=\"a\u{FFFD}&quot;b.php\""), "{junit}");
        assert!(junit.contains("-\u{FFFD}[0m\n+x\u{FFFD}]]>"), "{junit}");
        assert!(!junit.contains(['\x01', '\x1b', '\u{FFFF}']), "{junit}");
    }

    #[test]
    fn md5_like_php() {
        assert_eq!(md5(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(
            md5(b"a.phparray_syntax"),
            "0f09a23d834ce4b60a1f762a2597926b"
        );
        assert_eq!(md5(&[b'x'; 64]), "c1bb4f81d892b2d57947682aeb252456");
        assert_eq!(
            md5("é".repeat(40).as_bytes()),
            "639258863c18c505cc2183a5e4b63394"
        );
    }
}
