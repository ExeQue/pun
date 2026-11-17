//! The check `composer install` runs before it touches `vendor/` ("Verifying lock file contents can be installed on
//! current platform."): every locked package is fixed, so the lock installs only when each requirement of the root
//! and of the locked packages is met by the platform or another locked package, and no conflict holds. Ported from
//! `Installer::doInstall` and the problem texts of `Problem`, `Rule` and `SolverProblemsException`.

use crate::constraint::{self, Constraint};
use crate::error::Result;
use crate::lock::Lock;
use crate::resolver::package::{Link, Origin, Package, alias_package, load};
use crate::resolver::platform::{from_probe, ini_files};
use crate::version::{IgnorePlatform, compare, is_platform, normalize};
use serde_json::{Map, Value};
use std::fmt::Write as _;

/// One problem: its lines, and the requirement each line names (for the extension hint).
struct Problem {
    lines: Vec<String>,
    required: Vec<String>,
    /// `Rule::isCausedByLock`: a locked package's requirement that another locked version does not match.
    by_lock: bool,
}

struct Check<'a> {
    arena: Vec<Package>,
    fixed: Vec<usize>,
    probe: &'a Value,
    /// The platform without `config.platform`, for the "actual" version of an overridden package.
    detected: Vec<Map<String, Value>>,
}

/// Composer's problem report when `lock` can't be installed on the platform `probe` describes, or `None`.
pub fn verify(
    manifest: &Map<String, Value>,
    lock: &Lock,
    probe: &Value,
    dev_mode: bool,
    ignore: &IgnorePlatform,
) -> Result<Option<String>> {
    let overrides = lock
        .settings
        .get("platform-overrides")
        .and_then(Value::as_object);
    let mut arena = vec![];
    let mut root = manifest.clone();
    root.entry("name").or_insert_with(|| "__root__".into());
    root.entry("version")
        .or_insert_with(|| "1.0.0+no-version-set".into());
    root.remove("require");
    root.remove("require-dev");
    let mut fixed = load(root, Origin::Root, &mut arena)?;
    let root = *fixed.last().expect("the root package");
    for data in from_probe(probe, overrides)? {
        for p in load(data, Origin::Platform, &mut arena)? {
            let provided = (arena[root].provides.iter())
                .find(|l| l.target == arena[p].name)
                .is_some_and(|l| l.constraint.matches(&Constraint::exact(&arena[p].version)));
            if !provided {
                fixed.push(p);
            }
        }
    }
    let dev = lock.packages_dev.iter().filter(|_| dev_mode);
    let mut locked = vec![];
    for data in lock.packages.iter().chain(dev) {
        locked.extend(load(data.0.clone(), Origin::Locked, &mut arena)?);
    }
    let aliases = lock.settings.get("aliases").and_then(Value::as_array);
    for a in aliases.into_iter().flatten() {
        let s = |k: &str| {
            a.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        };
        let of = (locked.iter().copied())
            .find(|&p| arena[p].alias_of.is_none() && arena[p].name == s("package"));
        if let Some(of) = of {
            locked.push(alias_package(
                &mut arena,
                of,
                s("alias_normalized"),
                s("alias"),
                true,
            ));
        }
    }
    fixed.extend(&locked);
    let check = Check {
        arena,
        fixed,
        probe,
        detected: from_probe(probe, None)?,
    };
    let mut problems = check.root_problems(manifest, lock, dev_mode, ignore)?;
    let failed = check.requirement_problems(&locked, ignore, &mut problems);
    check.conflict_problems(&failed, ignore, &mut problems);
    Ok((!problems.is_empty()).then(|| report(&problems, &ini_files(probe))))
}

/// `IgnoreListPlatformRequirementFilter`: `None` when the requirement is ignored; with `name+`, the constraint
/// also accepts every version above it.
fn filtered(ignore: &IgnorePlatform, name: &str, c: &Constraint) -> Option<Constraint> {
    if !is_platform(name) {
        return Some(c.clone());
    }
    if ignore.ignores(name) {
        return None;
    }
    if !ignore.ignores(&format!("{name}+")) {
        return Some(c.clone());
    }
    let upper = c.upper_bound();
    if upper.version == constraint::Bound::infinity().version {
        return Some(c.clone());
    }
    let above = Constraint::Single(constraint::Op::Ge, upper.version.into_owned());
    Some(Constraint::Multi(vec![c.clone(), above], false))
}

fn at(p: &Package) -> String {
    format!("{} {}", p.pretty_name, p.pretty_version)
}

impl Check<'_> {
    /// The root's platform requirements, and the lock's, that the platform does not meet.
    fn root_problems(
        &self,
        manifest: &Map<String, Value>,
        lock: &Lock,
        dev_mode: bool,
        ignore: &IgnorePlatform,
    ) -> Result<Vec<Problem>> {
        let check = self;
        let mut problems = vec![];

        let links = |m: &Map<String, Value>, key: &str| -> Vec<(String, String)> {
            (m.get(key).and_then(Value::as_object).into_iter().flatten())
                .filter_map(|(k, v)| Some((k.to_ascii_lowercase(), v.as_str()?.to_string())))
                .collect()
        };
        let mut requires = links(manifest, "require");
        if dev_mode {
            requires.extend(links(manifest, "require-dev"));
        }
        let mut wanted: Vec<(String, String)> = (requires.iter())
            .filter(|(name, _)| is_platform(name))
            .cloned()
            .collect();
        let mut lock_platform = links(&lock.settings, "platform");
        if dev_mode {
            lock_platform.extend(links(&lock.settings, "platform-dev"));
        }
        for (name, pretty) in lock_platform {
            if !requires.iter().any(|(n, _)| *n == name) {
                wanted.push((name, pretty));
            }
        }
        for (name, pretty) in wanted {
            let Some(c) = filtered(ignore, &name, &constraint::parse(&pretty)?) else {
                continue;
            };
            if check.providers(&name, &c).is_empty() {
                let (prefix, reason) = check.missing(&name, &pretty);
                problems.push(Problem {
                    lines: vec![format!("{prefix}{reason}")],
                    required: vec![name],
                    by_lock: false,
                });
            }
        }
        Ok(problems)
    }

    /// The first unmet requirement of each locked package; the packages that have one.
    fn requirement_problems(
        &self,
        locked: &[usize],
        ignore: &IgnorePlatform,
        problems: &mut Vec<Problem>,
    ) -> Vec<usize> {
        let check = self;
        // The solver decides a locked package false at its first requirement nothing fixed provides, so each package
        // gets one problem, and its conflicts no longer count.
        let mut failed = vec![];
        for &p in locked {
            let package = &check.arena[p];
            if package.alias_of.is_some() {
                continue;
            }
            for link in &package.requires {
                let Some(c) = filtered(ignore, &link.target, &link.constraint) else {
                    continue;
                };
                if check.providers(&link.target, &c).is_empty() {
                    let (_, reason) = check.missing(&link.target, &link.pretty);
                    problems.push(Problem {
                        lines: vec![
                            check.fixed_line(p),
                            format!(
                                "{} requires {} {} -> {reason}",
                                at(package),
                                link.target,
                                link.pretty
                            ),
                        ],
                        required: vec![link.target.clone()],
                        by_lock: !is_platform(&link.target)
                            && (locked.iter()).any(|&l| {
                                check.arena[l].name == link.target
                                    && !link
                                        .constraint
                                        .matches(&Constraint::exact(&check.arena[l].version))
                            }),
                    });
                    failed.push(p);
                    break;
                }
            }
        }
        failed
    }

    /// The conflicts between fixed packages, except those of a package that already failed a requirement.
    fn conflict_problems(
        &self,
        failed: &[usize],
        ignore: &IgnorePlatform,
        problems: &mut Vec<Problem>,
    ) {
        let check = self;
        let mut pairs = vec![];
        for &p in &check.fixed {
            for link in &check.arena[p].conflicts {
                // `RuleSetGenerator::addConflictRules`: only a name some package has or replaces conflicts.
                let named = (check.fixed.iter())
                    .any(|&f| check.arena[f].names(false).contains(&link.target.as_str()));
                if !named || (is_platform(&link.target) && ignore.ignores(&link.target)) {
                    continue;
                }
                for q in check.providers(&link.target, &link.constraint) {
                    let other = &check.arena[q];
                    let pair = (p.min(q), p.max(q));
                    if q == p
                        || !(other.alias_of.is_none() || other.name == link.target)
                        || failed.contains(&p)
                        || failed.contains(&q)
                        || pairs.contains(&pair)
                    {
                        continue;
                    }
                    pairs.push(pair);
                    let mut fixed = [p, q];
                    fixed.sort_by_key(|&i| {
                        format!("{}-{}", check.arena[i].name, check.arena[i].version)
                    });
                    let mut lines: Vec<String> =
                        fixed.iter().map(|&i| check.fixed_line(i)).collect();
                    lines.push(check.conflict_line(p, q, link));
                    problems.push(Problem {
                        lines,
                        required: vec![],
                        by_lock: false,
                    });
                }
            }
        }
    }

    /// The fixed packages that are `name`, or provide or replace it, in `c`.
    fn providers(&self, name: &str, c: &Constraint) -> Vec<usize> {
        (self.fixed.iter().copied())
            .filter(|&p| {
                let p = &self.arena[p];
                if p.name == name {
                    return c.matches(&Constraint::exact(&p.version));
                }
                (p.provides.iter().chain(&p.replaces))
                    .any(|l| l.target == name && c.matches(&l.constraint))
            })
            .collect()
    }

    /// `Rule::getPrettyString` of a fixed package's rule.
    fn fixed_line(&self, p: usize) -> String {
        let p = &self.arena[p];
        if p.origin == Origin::Locked {
            format!(
                "{} is locked to version {} and an update of this package was not requested.",
                p.pretty_name, p.pretty_version
            )
        } else {
            format!(
                "{} is present at version {} and cannot be modified by Composer",
                p.pretty_name, p.pretty_version
            )
        }
    }

    fn conflict_line(&self, source: usize, other: usize, link: &Link) -> String {
        let other = &self.arena[other];
        let mut target = format!("{} {}", other.pretty_name, link.pretty);
        if link.target != other.name {
            for (kind, links) in [("provides", &other.provides), ("replaces", &other.replaces)] {
                if let Some(l) = links.iter().find(|l| l.target == link.target) {
                    target = format!(
                        "{} {} ({} {kind} {} {})",
                        link.target,
                        link.pretty,
                        at(other),
                        link.target,
                        l.pretty
                    );
                }
            }
        }
        format!("{} conflicts with {target}.", at(&self.arena[source]))
    }

    /// `getPlatformPackageVersion`: the version the platform has of `name`, if any.
    fn platform_version(&self, name: &str) -> Option<String> {
        let available: Vec<usize> = (self.fixed.iter().copied())
            .filter(|&p| self.arena[p].names(true).contains(&name))
            .collect();
        let selected = (available.iter().copied())
            .find(|&p| self.arena[p].origin == Origin::Platform)
            .or(available.first().copied())?;
        let p = &self.arena[selected];
        let overridden = p.data().get("extra").and_then(|e| e.get("config.platform"))
            == Some(&Value::Bool(true));
        if !overridden {
            return Some(p.pretty_version.clone());
        }
        let actual = (self.detected.iter())
            .find(|d| d["name"].as_str() == Some(p.name.as_str()))
            .and_then(|d| d["version"].as_str());
        let actual = match actual {
            Some(v) if normalize(v).is_ok_and(|v| v == p.version) => ", same as actual".to_string(),
            Some(v) => format!(", actual: {v}"),
            None => String::new(),
        };
        Some(format!(
            "{}; overridden via config.platform{actual}",
            p.pretty_version
        ))
    }

    fn loaded(&self, ext: &str) -> bool {
        (self.probe["ext"].as_array().into_iter().flatten())
            .filter_map(|e| e[0].as_str())
            .any(|e| e.eq_ignore_ascii_case(ext))
    }

    /// `Problem::getMissingPackageReason`: the root requirement's prefix and the reason.
    fn missing(&self, name: &str, pretty: &str) -> (String, String) {
        if name.starts_with("php") {
            let reason = match self.platform_version(name) {
                None => format!(
                    "the {name} package is disabled by your platform config. Enable it again with \"composer config platform.{name} --unset\"."
                ),
                Some(v) => format!("your {name} version ({v}) does not satisfy that requirement."),
            };
            return (
                format!("Root composer.json requires {name} {pretty} but "),
                reason,
            );
        }
        if let Some(ext) = name.strip_prefix("ext-") {
            let reason = match self.platform_version(name) {
                None if self.loaded(ext) => format!(
                    "the {name} package is disabled by your platform config. Enable it again with \"composer config platform.{name} --unset\"."
                ),
                None => format!(
                    "it is missing from your system. Install or enable PHP's {ext} extension."
                ),
                Some(v) => format!("it has the wrong version installed ({v})."),
            };
            return (
                format!("Root composer.json requires PHP extension {name} {pretty} but "),
                reason,
            );
        }
        if name.starts_with("lib-") {
            let reason = match name {
                "lib-icu" if self.loaded("intl") => {
                    "it has the wrong version installed, try upgrading the intl extension."
                }
                "lib-icu" => {
                    "it is missing from your system, make sure the intl extension is loaded."
                }
                _ => {
                    "it has the wrong version installed or is missing from your system, make sure to load the extension providing it."
                }
            };
            return (
                format!("Root composer.json requires linked library {name} {pretty} but "),
                reason.into(),
            );
        }
        let mut found: Vec<&Package> = (self.fixed.iter())
            .map(|&p| &self.arena[p])
            .filter(|p| p.name == name)
            .collect();
        if found.is_empty() {
            return (
                format!("Root composer.json requires {name}, it "),
                "could not be found in any version, there may be a typo in the package name."
                    .into(),
            );
        }
        found.sort_by(|a, b| compare(&a.version, &b.version).cmp(&0));
        let versions: Vec<String> = (found.iter())
            .map(|p| match p.alias_of {
                Some(of) => format!(
                    "{} (alias of {})",
                    p.pretty_version, self.arena[of].pretty_version
                ),
                None => p.pretty_version.clone(),
            })
            .collect();
        let verb = if found.len() > 1 {
            "these do"
        } else {
            "it does"
        };
        let root = if found[0].origin == Origin::Root {
            " See https://getcomposer.org/dep-on-root for details and assistance."
        } else {
            ""
        };
        (
            format!("Root composer.json requires {name} {pretty}, "),
            format!(
                "found {}[{}] but {verb} not match the constraint.{root}",
                found[0].pretty_name,
                versions.join(", ")
            ),
        )
    }
}

/// `SolverProblemsException::getPrettyString`.
fn report(problems: &[Problem], ini: &[String]) -> String {
    let mut text = String::from("\n");
    let mut seen: Vec<String> = vec![];
    for p in problems {
        let body = format!("\n    - {}\n", p.lines.join("\n    - "));
        if !seen.contains(&body) {
            let _ = write!(text, "  Problem {}{body}", seen.len() + 1);
            seen.push(body);
        }
    }
    let mut hints = vec![];
    if text.contains("could not be found") {
        hints.push("Potential causes:\n - A typo in the package name\n - The package is not available in a stable-enough version according to your minimum-stability setting\n   see <https://getcomposer.org/doc/04-schema.md#minimum-stability-root-only> for more details.\n - It's a private package and you forgot to add a custom repository to find it\n\nRead <https://getcomposer.org/doc/articles/troubleshooting.md> for further common problems.".to_string());
    }
    let mut extensions: Vec<&str> = vec![];
    for name in problems.iter().flat_map(|p| &p.required) {
        if name.starts_with("ext-") && !extensions.contains(&name.as_str()) {
            extensions.push(name);
        }
    }
    if !extensions.is_empty() {
        hints.push(if ini.is_empty() {
            String::new()
        } else {
            let flags: Vec<String> = (extensions.iter())
                .map(|e| format!("--ignore-platform-req={e}"))
                .collect();
            format!(
                "To enable extensions, verify that they are enabled in your .ini files:\n    - {}\nYou can also run `php --ini` in a terminal to see which files are used by PHP in CLI mode.\nAlternatively, you can run pun with `{}` to temporarily ignore these required extensions.",
                ini.join("\n    - "),
                flags.join(" ")
            )
        });
    }
    if problems.iter().any(|p| p.by_lock) {
        hints.push("Use the option --with-all-dependencies (-W) to allow upgrades, downgrades and removals for packages currently locked to specific versions.".to_string());
    }
    if !hints.is_empty() {
        text.push('\n');
        text.push_str(&hints.join("\n\n"));
    }
    text
}
