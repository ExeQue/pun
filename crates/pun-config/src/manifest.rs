use anyhow::{Context, Result, bail};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// composer.json's sections, which Pun 0.1 also read from pun.toml.
pub const SECTIONS: [&str; 15] = [
    "package",
    "require",
    "require-dev",
    "conflict",
    "provide",
    "replace",
    "suggest",
    "autoload",
    "autoload-dev",
    "scripts",
    "scripts-descriptions",
    "scripts-aliases",
    "config",
    "repositories",
    "extra",
];

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub path: PathBuf,
    pub json: Map<String, Value>,
}

/// The project's composer.json, the only place Pun reads dependencies from; `None` when there is none.
pub fn read(root: &Path) -> Result<Option<Manifest>> {
    let path = root.join("composer.json");
    let text = match std::fs::read_to_string(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        r => r.with_context(|| format!("can't read {}", path.display()))?,
    };
    let Value::Object(json) =
        serde_json::from_str(&text).with_context(|| format!("invalid {}", path.display()))?
    else {
        bail!("{} must hold an object", path.display());
    };
    Ok(Some(Manifest { path, json }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_project_composer_json() {
        let d = std::env::temp_dir().join(format!("pun-manifest-read-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let path = d.join("composer.json");
        assert_eq!(read(&d).unwrap(), None);
        std::fs::write(&path, r#"{"require": {"php": "^8.4"}}"#).unwrap();
        assert_eq!(
            Value::Object(read(&d).unwrap().unwrap().json),
            json!({"require": {"php": "^8.4"}})
        );
        std::fs::write(&path, "[]").unwrap();
        assert_eq!(
            read(&d).unwrap_err().to_string(),
            format!("{} must hold an object", path.display())
        );
        std::fs::write(&path, "{").unwrap();
        assert_eq!(
            read(&d).unwrap_err().to_string(),
            format!("invalid {}", path.display())
        );
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert_eq!(
            read(&d).unwrap_err().to_string(),
            format!("can't read {}", path.display())
        );
        std::fs::remove_dir_all(&d).unwrap();
    }
}
