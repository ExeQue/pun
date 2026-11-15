use crate::constraint;
use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl From<constraint::V> for Version {
    fn from((major, minor, patch): constraint::V) -> Self {
        Version {
            major,
            minor,
            patch,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhpRequire {
    pub constraint: String,
    pub lowest: Option<Version>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locked {
    pub version: String,
    pub semver: Option<Version>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Composer {
    pub php: Option<PhpRequire>,
    pub platform_php: Option<String>,
    pub laravel: Option<Locked>,
    pub pest: Option<Locked>,
    pub phpunit: Option<Locked>,
    pub sources: Vec<PathBuf>,
}

impl Composer {
    pub fn platform_php_version(&self) -> Option<crate::PhpVersion> {
        let mut parts = self.platform_php.as_deref()?.split('.');
        let mut part = || {
            let p = parts.next().unwrap_or("");
            let digits = p.len() - p.trim_start_matches(|c: char| c.is_ascii_digit()).len();
            p[..digits].parse::<u32>().unwrap_or(0)
        };
        let id = part() * 10_000 + part() * 100 + part();
        Some(crate::PhpVersion::from_id(id.clamp(70_100, 80_699)))
    }

    pub fn php_version(&self) -> Option<crate::PhpVersion> {
        let v = self.php.as_ref()?.lowest?;
        let major: u8 = v.major.try_into().ok()?;
        let minor: u8 = v.minor.try_into().ok()?;
        Some(crate::PhpVersion::new(major.into(), minor.into()))
    }
}

pub fn detect(root: &Path) -> Result<Composer> {
    let mut out = Composer::default();
    let (manifest, lock) = match crate::manifest::read(root)? {
        Some(m) => {
            out.sources.push(m.path);
            let lock = read_toml(&root.join("pun.lock"), &mut out.sources)?;
            (Some(Value::Object(m.json)), lock)
        }
        None => (
            read(&root.join("composer.json"), &mut out.sources)?,
            read(&root.join("composer.lock"), &mut out.sources)?,
        ),
    };
    if let Some(json) = manifest {
        out.php = php_require(&json)?;
        out.platform_php =
            (json.pointer("/config/platform/php").and_then(Value::as_str)).map(str::to_string);
    }
    for key in ["packages", "packages-dev"] {
        for p in (lock.as_ref())
            .and_then(|l| l.get(key))
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let slot = match p.get("name").and_then(Value::as_str) {
                Some("laravel/framework") => &mut out.laravel,
                Some("pestphp/pest") => &mut out.pest,
                Some("phpunit/phpunit") => &mut out.phpunit,
                _ => continue,
            };
            let version = p.get("version").and_then(Value::as_str).unwrap_or("");
            *slot = Some(Locked {
                version: version.to_string(),
                semver: semver(version),
            });
        }
    }
    Ok(out)
}

fn read_toml(path: &Path, sources: &mut Vec<PathBuf>) -> Result<Option<Value>> {
    let text = match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        r => r.with_context(|| format!("can't read {}", path.display()))?,
    };
    let table: toml::Table =
        toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))?;
    sources.push(path.to_path_buf());
    Ok(Some(crate::toml_text::to_json(&toml::Value::Table(table))))
}

fn php_require(json: &Value) -> Result<Option<PhpRequire>> {
    let Some(c) = json.pointer("/require/php").and_then(Value::as_str) else {
        return Ok(None);
    };
    Ok(Some(PhpRequire {
        constraint: c.to_string(),
        lowest: constraint::lowest(c)?.map(Version::from),
    }))
}

fn read(path: &Path, sources: &mut Vec<PathBuf>) -> Result<Option<Value>> {
    let bytes = match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        r => r.with_context(|| format!("cannot read {}", path.display()))?,
    };
    let v =
        serde_json::from_slice(&bytes).with_context(|| format!("invalid {}", path.display()))?;
    sources.push(path.to_path_buf());
    Ok(Some(v))
}

fn semver(v: &str) -> Option<Version> {
    let v = v.strip_prefix(['v', 'V']).unwrap_or(v);
    let release = v.split(['-', '+']).next()?;
    let mut parts = release.split('.').map(|p| p.parse::<u64>().ok());
    let major = parts.next()??;
    let minor = parts.next().unwrap_or(Some(0))?;
    let patch = parts.next().unwrap_or(Some(0))?;
    Some(Version {
        major,
        minor,
        patch,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(files: &[(&str, &str)]) -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("pun-composer-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        for (name, text) in files {
            std::fs::write(d.join(name), text).unwrap();
        }
        d
    }

    fn v(major: u64, minor: u64, patch: u64) -> Version {
        Version {
            major,
            minor,
            patch,
        }
    }

    fn php(constraint: &str) -> Composer {
        let json = serde_json::json!({ "require": { "php": constraint } }).to_string();
        detect(&dir(&[("composer.json", &json)])).unwrap()
    }

    fn lock(packages: &str, dev: &str) -> String {
        format!(r#"{{"packages":[{packages}],"packages-dev":[{dev}]}}"#)
    }

    fn pkg(name: &str, version: &str) -> String {
        format!(r#"{{"name":"{name}","version":"{version}"}}"#)
    }

    fn locked(version: &str, semver: Option<Version>) -> Locked {
        Locked {
            version: version.into(),
            semver,
        }
    }

    #[test]
    fn lowest_php_version() {
        let cases = [
            ("^8.2|^8.3", v(8, 2, 0), "8.2"),
            ("^8.3 || ^8.2", v(8, 2, 0), "8.2"),
            (">=8.4", v(8, 4, 0), "8.4"),
            ("8.4.*", v(8, 4, 0), "8.4"),
            ("~8.4.1", v(8, 4, 1), "8.4"),
            ("~8.3", v(8, 3, 0), "8.3"),
            (">=8.2 <8.5", v(8, 2, 0), "8.2"),
            (">=8.3 >=8.2", v(8, 3, 0), "8.3"),
            (">=8.2, <8.4 || >=8.5", v(8, 2, 0), "8.2"),
            (">=8.4 <8.3 || ^8.5", v(8, 5, 0), "8.5"),
            (">8.4", v(8, 4, 1), "8.4"),
            ("8.4.2", v(8, 4, 2), "8.4"),
            ("8.3 - 8.5", v(8, 3, 0), "8.3"),
            ("*", v(0, 0, 0), "0.0"),
        ];
        for (c, low, short) in cases {
            let got = php(c);
            assert_eq!(
                got.php,
                Some(PhpRequire {
                    constraint: c.into(),
                    lowest: Some(low)
                }),
                "{c}"
            );
            assert_eq!(
                got.php_version().map(|v| v.to_string()).as_deref(),
                Some(short),
                "{c}"
            );
        }
        let none = php(">=8.4 <8.3");
        assert_eq!(none.php.unwrap().lowest, None);
        assert_eq!(php(">=8.4 <8.3").php_version(), None);
    }

    #[test]
    fn missing_files() {
        assert_eq!(detect(&dir(&[])).unwrap(), Composer::default());
        let d = dir(&[(
            "composer.json",
            r#"{"require":{"laravel/framework":"^13.0"}}"#,
        )]);
        let c = detect(&d).unwrap();
        assert_eq!(c.php, None);
        assert_eq!(c.php_version(), None);
        assert_eq!((c.laravel, c.pest, c.phpunit), (None, None, None));
        assert_eq!(c.sources, [d.join("composer.json")]);
        let d = dir(&[(
            "composer.lock",
            &lock(&pkg("laravel/framework", "v12.0.1"), ""),
        )]);
        let c = detect(&d).unwrap();
        assert_eq!(c.php, None);
        assert_eq!(c.laravel, Some(locked("v12.0.1", Some(v(12, 0, 1)))));
        assert_eq!(c.sources, [d.join("composer.lock")]);
    }

    #[test]
    fn pun_files_win_over_composers() {
        let d = dir(&[
            ("composer.json", r#"{"require":{"php":"^8.4"}}"#),
            (
                "composer.lock",
                &lock(&pkg("laravel/framework", "v12.0.1"), ""),
            ),
            (
                "pun.toml",
                "[require]\nphp = \"^8.5\"\n\n[config.platform]\nphp = \"8.5.1\"\n",
            ),
            (
                "pun.lock",
                "version = 1\ncontent-hash = \"0\"\n\n[[packages]]\nname = \"laravel/framework\"\nversion = \"v13.1.0\"\n\n[[packages-dev]]\nname = \"pestphp/pest\"\nversion = \"v5.0.3\"\n",
            ),
        ]);
        let c = detect(&d).unwrap();
        assert_eq!(c.php.map(|p| p.lowest), Some(Some(v(8, 5, 0))));
        assert_eq!(c.platform_php.as_deref(), Some("8.5.1"));
        assert_eq!(c.laravel, Some(locked("v13.1.0", Some(v(13, 1, 0)))));
        assert_eq!(c.pest, Some(locked("v5.0.3", Some(v(5, 0, 3)))));
        assert_eq!(c.sources, [d.join("pun.toml"), d.join("pun.lock")]);
        std::fs::remove_file(d.join("pun.lock")).unwrap();
        assert_eq!(detect(&d).unwrap().laravel, None);
    }

    #[test]
    fn all_three_ecosystems() {
        let text = lock(
            &[
                pkg("laravel/pint", "v1.29.1"),
                pkg("laravel/framework", "v13.23.0"),
            ]
            .join(","),
            &[
                pkg("pestphp/pest", "v5.0.3"),
                pkg("phpunit/phpunit", "13.2.6"),
            ]
            .join(","),
        );
        let d = dir(&[
            ("composer.json", r#"{"require":{"php":"^8.4"}}"#),
            ("composer.lock", &text),
        ]);
        let c = detect(&d).unwrap();
        assert_eq!(
            c.php_version().map(|v| v.to_string()).as_deref(),
            Some("8.4")
        );
        assert_eq!(c.laravel, Some(locked("v13.23.0", Some(v(13, 23, 0)))));
        assert_eq!(c.pest, Some(locked("v5.0.3", Some(v(5, 0, 3)))));
        assert_eq!(c.phpunit, Some(locked("13.2.6", Some(v(13, 2, 6)))));
        assert_eq!(
            c.sources,
            [d.join("composer.json"), d.join("composer.lock")]
        );
    }

    #[test]
    fn each_ecosystem() {
        let only = |packages: &str, dev: &str| {
            detect(&dir(&[("composer.lock", &lock(packages, dev))])).unwrap()
        };
        let c = only(&pkg("laravel/framework", "v11.31.0"), "");
        assert_eq!(c.laravel, Some(locked("v11.31.0", Some(v(11, 31, 0)))));
        assert_eq!((c.pest, c.phpunit), (None, None));
        let c = only("", &pkg("pestphp/pest", "v4.1.0"));
        assert_eq!(c.pest, Some(locked("v4.1.0", Some(v(4, 1, 0)))));
        assert_eq!((c.laravel, c.phpunit), (None, None));
        let c = only(&pkg("phpunit/phpunit", "12.4.1"), "");
        assert_eq!(c.phpunit, Some(locked("12.4.1", Some(v(12, 4, 1)))));
        assert_eq!((c.laravel, c.pest), (None, None));
        let c = only(
            &[
                pkg("laravel/laravel", "v13.0.0"),
                pkg("illuminate/support", "v13.0.0"),
            ]
            .join(","),
            "",
        );
        assert_eq!(c.laravel, None);
    }

    #[test]
    fn locked_versions() {
        let c = detect(&dir(&[(
            "composer.lock",
            &lock(
                &[
                    pkg("laravel/framework", "dev-main"),
                    pkg("phpunit/phpunit", "12.4"),
                ]
                .join(","),
                &[pkg("pestphp/pest", "5.0.0-RC1")].join(","),
            ),
        )]))
        .unwrap();
        assert_eq!(c.laravel, Some(locked("dev-main", None)));
        assert_eq!(c.phpunit, Some(locked("12.4", Some(v(12, 4, 0)))));
        assert_eq!(c.pest, Some(locked("5.0.0-RC1", Some(v(5, 0, 0)))));
        let c = detect(&dir(&[(
            "composer.lock",
            &lock(&pkg("pestphp/pest", "3.x-dev"), ""),
        )]))
        .unwrap();
        assert_eq!(c.pest, Some(locked("3.x-dev", None)));
        let c = detect(&dir(&[("composer.lock", r#"{"packages":[]}"#)])).unwrap();
        assert_eq!(c.pest, None);
    }

    #[test]
    fn errors() {
        let e = |files: &[(&str, &str)]| format!("{:#}", detect(&dir(files)).unwrap_err());
        assert!(e(&[("composer.lock", "{")]).starts_with("invalid "));
        assert!(e(&[("composer.json", "[")]).contains("composer.json: "));
        assert!(
            e(&[("composer.json", r#"{"require":{"php":">="}}"#)])
                .starts_with("invalid constraint `>=`: ")
        );
        for name in ["composer.json", "composer.lock"] {
            let d = dir(&[]);
            std::fs::create_dir(d.join(name)).unwrap();
            let e = format!("{:#}", detect(&d).unwrap_err());
            let want = format!("cannot read {}: ", d.join(name).display());
            assert!(e.starts_with(&want), "{e}");
        }
    }
}
