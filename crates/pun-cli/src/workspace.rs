use anyhow::{Context, Result, bail};
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

pub(crate) struct Workspace {
    pub(crate) root: PathBuf,
    globs: Vec<String>,
    pub(crate) members: Vec<Member>,
}

pub(crate) struct Member {
    pub(crate) name: String,
    pub(crate) dir: PathBuf,
}

pub(crate) fn find(dir: &Path) -> Result<Option<Workspace>> {
    for root in dir.ancestors() {
        let Some(globs) = globs(root)? else { continue };
        let workspace = load(root, globs)?;
        let inside = root == dir || workspace.members.iter().any(|m| dir.starts_with(&m.dir));
        return Ok(inside.then_some(workspace));
    }
    Ok(None)
}

fn globs(root: &Path) -> Result<Option<Vec<String>>> {
    let path = root.join("pun.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    if !text.contains("workspace") {
        return Ok(None);
    }
    let table: toml::Table =
        toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))?;
    let Some(workspace) = table.get("workspace") else {
        return Ok(None);
    };
    let members = (workspace.as_table())
        .filter(|t| t.keys().all(|k| k == "members"))
        .and_then(|t| t.get("members"))
        .and_then(toml::Value::as_array)
        .and_then(|m| {
            m.iter()
                .map(|g| g.as_str().map(String::from))
                .collect::<Option<Vec<_>>>()
        });
    match members {
        Some(m) => Ok(Some(m)),
        None => bail!(
            "{}: [workspace] takes one key, members, a list of directories or globs such as \"packages/*\"",
            path.display()
        ),
    }
}

fn load(root: &Path, globs: Vec<String>) -> Result<Workspace> {
    let mut members: Vec<Member> = vec![];
    for glob in &globs {
        for dir in expand(root, glob) {
            let Some(name) = name_of(&dir)? else { continue };
            if let Some(other) = members.iter().find(|m| m.name == name && m.dir != dir) {
                bail!(
                    "two workspace members are named {name}: {} and {}",
                    other.dir.display(),
                    dir.display()
                );
            }
            if !members.iter().any(|m| m.dir == dir) {
                members.push(Member { name, dir });
            }
        }
    }
    members.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Workspace {
        root: root.to_path_buf(),
        globs,
        members,
    })
}

fn name_of(dir: &Path) -> Result<Option<String>> {
    let manifest = match pun_config::manifest::read(dir)? {
        Some(m) => m.json,
        None if dir.join("composer.json").is_file() => bail!(
            "the workspace member in {} keeps its dependencies in composer.json: run `pun migrate` there",
            dir.display()
        ),
        None => return Ok(None),
    };
    match manifest.get("name").and_then(Value::as_str) {
        Some(name) => Ok(Some(name.to_ascii_lowercase())),
        None => bail!(
            "the workspace member in {} has no name: set `name` under [package]",
            dir.display()
        ),
    }
}

fn expand(root: &Path, glob: &str) -> Vec<PathBuf> {
    let mut dirs = vec![root.to_path_buf()];
    for segment in glob.split('/').filter(|s| !s.is_empty() && *s != ".") {
        if !segment.contains(['*', '?']) {
            dirs = dirs.into_iter().map(|d| d.join(segment)).collect();
            continue;
        }
        let mut next = vec![];
        for dir in &dirs {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let hidden = name.starts_with('.') && !segment.starts_with('.');
                if !hidden && entry.path().is_dir() && wildcard(segment.as_bytes(), name.as_bytes())
                {
                    next.push(entry.path());
                }
            }
        }
        dirs = next;
    }
    dirs.retain(|d| d.is_dir());
    dirs.sort();
    dirs
}

fn wildcard(pattern: &[u8], name: &[u8]) -> bool {
    match (pattern.split_first(), name.split_first()) {
        (None, None) => true,
        (Some((b'*', rest)), _) => {
            wildcard(rest, name)
                || name
                    .split_first()
                    .is_some_and(|(_, n)| wildcard(pattern, n))
        }
        (Some((b'?', rest)), Some((_, n))) => wildcard(rest, n),
        (Some((p, rest)), Some((c, n))) => p == c && wildcard(rest, n),
        _ => false,
    }
}

impl Workspace {
    pub(crate) fn member(&self, name: Option<&str>, dir: &Path) -> Result<Option<&Member>> {
        match name {
            Some(name) => {
                let name = name.to_ascii_lowercase();
                match self.members.iter().find(|m| m.name == name) {
                    Some(m) => Ok(Some(m)),
                    None => bail!(
                        "{name} is not a workspace member; the members are {}",
                        self.members
                            .iter()
                            .map(|m| m.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                }
            }
            None => Ok(self.members.iter().find(|m| dir.starts_with(&m.dir))),
        }
    }

    pub(crate) fn manifest(&self, mut manifest: Map<String, Value>) -> Map<String, Value> {
        let mut repositories: Vec<Value> = (self.globs.iter())
            .map(|g| json!({"type": "path", "url": g, "options": {"symlink": true}}))
            .collect();
        match manifest.get("repositories") {
            Some(Value::Array(own)) => repositories.extend(own.iter().cloned()),
            Some(Value::Object(own)) => repositories.extend(own.iter().map(|(k, v)| match v {
                Value::Bool(false) => json!({ k.as_str(): false }),
                v => v.clone(),
            })),
            _ => {}
        }
        manifest.insert("repositories".into(), Value::Array(repositories));
        let required = |m: &Map<String, Value>, name: &str| {
            ["require", "require-dev"].iter().any(|s| {
                (m.get(*s).and_then(Value::as_object))
                    .is_some_and(|r| r.keys().any(|k| k.eq_ignore_ascii_case(name)))
            })
        };
        for member in &self.members {
            if !required(&manifest, &member.name) {
                let require = manifest.entry("require").or_insert_with(|| json!({}));
                if let Some(r) = require.as_object_mut() {
                    r.insert(member.name.clone(), "*@dev".into());
                }
            }
        }
        manifest
    }
}

#[cfg(test)]
mod tests {
    use super::wildcard;

    #[test]
    fn matches_wildcards() {
        for (pattern, name, want) in [
            ("*", "billing", true),
            ("bill*", "billing", true),
            ("*ing", "billing", true),
            ("b?lling", "billing", true),
            ("b?lling", "blling", false),
            ("*x*", "billing", false),
            ("billing", "billings", false),
        ] {
            assert_eq!(
                wildcard(pattern.as_bytes(), name.as_bytes()),
                want,
                "{pattern} {name}"
            );
        }
    }
}
