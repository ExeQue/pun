use crate::constraint::{self, Constraint};
use crate::error::{Context, Result, bail};
use crate::version::{branch_alias, normalize, stability_of};
use serde_json::{Map, Value};
use std::sync::Arc;

pub const STABILITIES: [(&str, u8); 5] = [
    ("stable", 0),
    ("RC", 5),
    ("beta", 10),
    ("alpha", 15),
    ("dev", 20),
];

pub fn stability_value(name: &str) -> u8 {
    (STABILITIES.iter())
        .find(|(s, _)| s.eq_ignore_ascii_case(name))
        .map_or(0, |(_, v)| *v)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub target: String,
    pub constraint: Constraint,
    pub pretty: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Root,
    Platform,
    Locked,
    Repository(usize),
}

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub pretty_name: String,
    pub version: String,
    pub pretty_version: String,
    pub stability: u8,
    pub requires: Vec<Link>,
    pub dev_requires: Vec<Link>,
    pub conflicts: Vec<Link>,
    pub provides: Vec<Link>,
    pub replaces: Vec<Link>,
    pub alias_of: Option<usize>,
    pub root_alias: bool,
    pub self_version_requires: bool,
    pub origin: Origin,
    pub kind: String,
    pub abandoned: bool,
    pub data: Arc<str>,
}

impl Package {
    pub fn is_dev(&self) -> bool {
        self.stability == 20
    }

    pub fn names(&self, provides: bool) -> Vec<&str> {
        let mut names = vec![self.name.as_str()];
        let links = (if provides { &self.provides[..] } else { &[] })
            .iter()
            .chain(&self.replaces);
        for link in links {
            if !names.contains(&link.target.as_str()) {
                names.push(&link.target);
            }
        }
        names
    }

    pub fn data(&self) -> Map<String, Value> {
        serde_json::from_str(&self.data).expect("the JSON a package was loaded from")
    }
}

const LINK_KINDS: [&str; 5] = ["require", "require-dev", "conflict", "provide", "replace"];

fn links_of<'a>(p: &'a mut Package, kind: &str) -> &'a mut Vec<Link> {
    match kind {
        "require" => &mut p.requires,
        "require-dev" => &mut p.dev_requires,
        "conflict" => &mut p.conflicts,
        "provide" => &mut p.provides,
        _ => &mut p.replaces,
    }
}

fn link(name: &str, pretty_version: &str, kind: &str, target: &str, pretty: &str) -> Result<Link> {
    let source = if pretty == "self.version" {
        pretty_version
    } else {
        pretty
    };
    let constraint = constraint::parse(source).with_context(|| {
        format!(
            "Link constraint in {name} {} > {target} should be a valid version constraint, got \"{source}\"",
            link_description(kind)
        )
    })?;
    Ok(Link {
        target: target.to_string(),
        constraint,
        pretty: pretty.to_string(),
    })
}

fn link_description(kind: &str) -> &str {
    match kind {
        "require" => "requires",
        "require-dev" => "devRequires",
        "conflict" => "conflicts",
        "provide" => "provides",
        _ => "replaces",
    }
}

pub fn load(
    data: Map<String, Value>,
    origin: Origin,
    arena: &mut Vec<Package>,
) -> Result<Vec<usize>> {
    let pretty_name = data
        .get("name")
        .and_then(Value::as_str)
        .with_context(|| {
            format!(
                "Unknown package has no name defined ({}).",
                Value::Object(data.clone())
            )
        })?
        .to_string();
    let pretty_version = match data.get("version") {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => bail!("Package {pretty_name} has no version defined."),
    };
    let version = match data.get("version_normalized").and_then(Value::as_str) {
        Some(v) if v != "9999999-dev" => v.to_string(),
        _ => normalize(&pretty_version).with_context(|| {
            format!("Failed to normalize version for package \"{pretty_name}\"")
        })?,
    };
    let name = pretty_name.to_ascii_lowercase();
    let mut package = Package {
        stability: stability_value(stability_of(&version)),
        name,
        pretty_name,
        version,
        pretty_version,
        requires: vec![],
        dev_requires: vec![],
        conflicts: vec![],
        provides: vec![],
        replaces: vec![],
        alias_of: None,
        root_alias: false,
        self_version_requires: false,
        origin,
        kind: (data.get("type").and_then(Value::as_str))
            .unwrap_or("library")
            .to_string(),
        abandoned: data
            .get("abandoned")
            .is_some_and(|a| a != &Value::Bool(false)),
        data: Arc::from(""),
    };
    for kind in LINK_KINDS {
        let Some(Value::Object(links)) = data.get(kind) else {
            continue;
        };
        for (target, pretty) in links {
            let Some(pretty) = pretty.as_str() else {
                continue;
            };
            let target = target.to_ascii_lowercase();
            if target == package.name {
                continue;
            }
            let l = link(
                &package.name,
                &package.pretty_version,
                kind,
                &target,
                pretty,
            )?;
            let list = links_of(&mut package, kind);
            match list.iter_mut().find(|x| x.target == target) {
                Some(existing) => *existing = l,
                None => list.push(l),
            }
        }
    }
    let alias = branch_alias(&data);
    package.data = Arc::from(Value::Object(data).to_string());
    let base = arena.len();
    arena.push(package);
    Ok(match alias {
        Some((normalized, pretty)) => {
            let alias = alias_package(arena, base, normalized, pretty, false);
            vec![alias, base]
        }
        None => vec![base],
    })
}

pub fn alias_package(
    arena: &mut Vec<Package>,
    of: usize,
    version: String,
    pretty_version: String,
    root: bool,
) -> usize {
    let base = &arena[of];
    let shown = if pretty_version == "9999999-dev" {
        base.pretty_version.clone()
    } else {
        pretty_version.clone()
    };
    let at = |l: &Link| Link {
        target: l.target.clone(),
        constraint: Constraint::exact(&version),
        pretty: shown.clone(),
    };
    let mut self_version_requires = false;
    let mut replaced = |links: &[Link], require: bool| -> Vec<Link> {
        (links.iter())
            .map(|l| {
                if l.pretty == "self.version" {
                    self_version_requires |= require;
                    at(l)
                } else {
                    l.clone()
                }
            })
            .collect()
    };
    let requires = replaced(&base.requires, true);
    let dev_requires = replaced(&base.dev_requires, false);
    let added = |links: &[Link]| -> Vec<Link> {
        let extra = links.iter().filter(|l| l.pretty == "self.version").map(at);
        links.iter().cloned().chain(extra).collect()
    };
    let (conflicts, provides, replaces) = (
        added(&base.conflicts),
        added(&base.provides),
        added(&base.replaces),
    );
    let alias = Package {
        name: base.name.clone(),
        pretty_name: base.pretty_name.clone(),
        stability: stability_value(stability_of(&version)),
        version,
        pretty_version,
        requires,
        dev_requires,
        conflicts,
        provides,
        replaces,
        alias_of: Some(of),
        root_alias: root,
        self_version_requires,
        origin: base.origin,
        kind: base.kind.clone(),
        abandoned: base.abandoned,
        data: base.data.clone(),
    };
    arena.push(alias);
    arena.len() - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn loaded(data: Value) -> Result<Vec<Package>> {
        let mut arena = vec![];
        let ids = load(
            data.as_object().unwrap().clone(),
            Origin::Locked,
            &mut arena,
        )?;
        Ok(ids.into_iter().map(|i| arena[i].clone()).collect())
    }

    fn pretty(links: &[Link]) -> Vec<(&str, &str)> {
        (links.iter())
            .map(|l| (l.target.as_str(), l.pretty.as_str()))
            .collect()
    }

    #[test]
    fn loads_as_composer() {
        for (data, message) in [
            (
                json!({"version": "1.0"}),
                r#"Unknown package has no name defined ({"version":"1.0"})."#,
            ),
            (
                json!({"name": "acme/a"}),
                "Package acme/a has no version defined.",
            ),
            (
                json!({"name": "acme/a", "version": "x"}),
                r#"Failed to normalize version for package "acme/a": Invalid version string "x""#,
            ),
        ] {
            assert_eq!(format!("{:#}", loaded(data).unwrap_err()), message);
        }
        for (kind, description) in [
            ("require", "requires"),
            ("require-dev", "devRequires"),
            ("conflict", "conflicts"),
            ("provide", "provides"),
            ("replace", "replaces"),
        ] {
            let data = json!({"name": "acme/a", "version": "1.0", kind: {"b/b": "nope"}});
            assert_eq!(
                loaded(data).unwrap_err().to_string(),
                format!(
                    "Link constraint in acme/a {description} > b/b should be a valid version constraint, got \"nope\""
                )
            );
        }
        let p = &loaded(json!({"name": "acme/a", "version": 1.5})).unwrap()[0];
        assert_eq!(
            (p.pretty_version.as_str(), p.version.as_str()),
            ("1.5", "1.5.0.0")
        );
        let data =
            json!({"name": "acme/a", "version": "dev-master", "version_normalized": "9999999-dev"});
        assert_eq!(loaded(data).unwrap()[0].version, "dev-master");
        let data = json!({"name": "acme/a", "version": "1.0", "require": {"b/b": ["x"], "c/c": "^1", "C/C": "^2"}});
        assert_eq!(pretty(&loaded(data).unwrap()[0].requires), [("c/c", "^2")]);
    }

    #[test]
    fn aliases_as_composer() {
        let data = json!({
            "name": "acme/a",
            "version": "dev-main",
            "extra": {"branch-alias": {"dev-main": "1.0.x-dev"}},
            "require": {"acme/b": "self.version", "acme/d": "^1.0"},
            "require-dev": {"acme/e": "self.version"},
            "replace": {"acme/c": "self.version"}
        });
        let packages = loaded(data).unwrap();
        let alias = &packages[0];
        assert_eq!(alias.version, "1.0.9999999.9999999-dev");
        assert_eq!(
            pretty(&alias.requires),
            [("acme/b", "1.0.x-dev"), ("acme/d", "^1.0")]
        );
        let exact = Constraint::exact("1.0.9999999.9999999-dev");
        assert_eq!(alias.requires[0].constraint, exact);
        assert_eq!(pretty(&alias.dev_requires), [("acme/e", "1.0.x-dev")]);
        assert_eq!(
            pretty(&alias.replaces),
            [("acme/c", "self.version"), ("acme/c", "1.0.x-dev")]
        );
        assert!(alias.self_version_requires);
        assert_eq!(
            (alias.alias_of, packages[1].version.as_str()),
            (Some(0), "dev-main")
        );
    }
}
