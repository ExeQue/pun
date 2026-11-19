//! The check `composer install` runs before it touches `vendor/` ("Verifying lock file contents can be installed on
//! current platform."): every locked package is fixed, so the lock installs only when each requirement of the root
//! and of the locked packages is met by the platform or another locked package, and no conflict holds. Ported from
//! `Installer::doInstall` and the problem texts of `Problem`, `Rule` and `SolverProblemsException`.

use crate::constraint::{self, Constraint};
use crate::error::Result;
use crate::lock::Lock;
use crate::resolver::package::{Link, Origin, Package, alias_package, load};
use crate::resolver::platform::{from_probe, ini_files};
use crate::version::{IgnorePlatform, is_platform, normalize};
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
    let mut fixed = root_package(manifest, &mut arena)?;
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
    let locked = locked_repository(lock, dev_mode, &mut arena)?;
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

/// The root package as the check sees it: without its requirements, named and versioned as Composer defaults them;
/// its alias first, as in a repository.
fn root_package(manifest: &Map<String, Value>, arena: &mut Vec<Package>) -> Result<Vec<usize>> {
    let mut root = manifest.clone();
    root.entry("name").or_insert_with(|| "__root__".into());
    root.entry("version")
        .or_insert_with(|| "1.0.0+no-version-set".into());
    root.remove("require");
    root.remove("require-dev");
    load(root, Origin::Root, arena)
}

/// `Locker::getLockedRepository`: the locked packages (with `packages-dev` when `dev`), each branch alias before
/// its package, then the root's inline aliases recorded in the lock.
fn locked_repository(lock: &Lock, dev: bool, arena: &mut Vec<Package>) -> Result<Vec<usize>> {
    let mut locked = vec![];
    for data in lock
        .packages
        .iter()
        .chain(lock.packages_dev.iter().filter(|_| dev))
    {
        locked.extend(load(data.0.clone(), Origin::Locked, arena)?);
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
                arena,
                of,
                s("alias_normalized"),
                s("alias"),
                true,
            ));
        }
    }
    Ok(locked)
}

/// `Locker::getMissingRequirementInfo`, which `composer install` prints before it checks the platform: each root
/// requirement (and, with `dev_mode`, each of `require-dev`) that no locked package, nor the root, meets. Empty when
/// every one is met; otherwise Composer exits 4 unless `config.allow-missing-requirements` is set.
pub fn missing_requirements(
    manifest: &Map<String, Value>,
    lock: &Lock,
    dev_mode: bool,
) -> Result<Vec<String>> {
    let sets = [
        ("require", "Required", false),
        ("require-dev", "Required (in require-dev)", true),
    ];
    let sets = &sets[..if dev_mode { 2 } else { 1 }];
    if met_by_name(manifest, lock, sets) {
        return Ok(vec![]);
    }
    let mut lines = vec![];
    for (key, description, dev) in sets {
        let mut arena = vec![];
        let mut repo = locked_repository(lock, *dev, &mut arena)?;
        repo.extend(root_package(manifest, &mut arena)?);
        let links = (manifest
            .get(*key)
            .and_then(Value::as_object)
            .into_iter()
            .flatten())
        .filter_map(|(k, v)| Some((k.to_ascii_lowercase(), v.as_str()?)));
        for (target, pretty) in links {
            if is_platform(&target) || pretty == "self.version" {
                continue;
            }
            let c = constraint::parse(pretty)?;
            if find(&arena, &repo, &target, Some(&c)).is_some() {
                continue;
            }
            lines.push(match find(&arena, &repo, &target, None) {
                Some(p) => format!(
                    "- {description} package \"{target}\" is in the lock file as \"{}\" but that does not satisfy your constraint \"{pretty}\".",
                    described(p, &target)
                ),
                None => {
                    format!("- {description} package \"{target}\" is not present in the lock file.")
                }
            });
        }
    }
    if !lines.is_empty() {
        lines.extend([
            "This usually happens when composer files are incorrectly merged or the composer.json file is manually edited.",
            "Read more about correctly resolving merge conflicts https://getcomposer.org/doc/articles/resolving-merge-conflicts.md",
            "and prefer using the \"require\" command over editing the composer.json file directly https://getcomposer.org/doc/03-cli.md#require--r",
        ].map(String::from));
    }
    Ok(lines)
}

/// Whether a locked package of each requirement's own name meets it, so Composer finds it before any alias,
/// provider or replacer: an install's usual case, settled without loading the locked packages. A version or
/// constraint that doesn't parse is left to the full check, which reports it.
fn met_by_name(manifest: &Map<String, Value>, lock: &Lock, sets: &[(&str, &str, bool)]) -> bool {
    sets.iter().all(|(key, _, dev)| {
        let packages = lock
            .packages
            .iter()
            .chain(lock.packages_dev.iter().filter(|_| *dev));
        let mut versions: std::collections::HashMap<String, Vec<Constraint>> =
            std::collections::HashMap::new();
        for p in packages {
            let pretty = p.get("version").and_then(Value::as_str).unwrap_or_default();
            let version = (p.get("version_normalized").and_then(Value::as_str))
                .filter(|v| *v != "9999999-dev")
                .map(String::from)
                .or_else(|| normalize(pretty).ok());
            (versions.entry(p.name().to_ascii_lowercase()).or_default())
                .extend(version.map(|v| Constraint::exact(&v)));
        }
        let links = (manifest
            .get(*key)
            .and_then(Value::as_object)
            .into_iter()
            .flatten())
        .filter_map(|(k, v)| Some((k.to_ascii_lowercase(), v.as_str()?)));
        links.into_iter().all(|(target, pretty)| {
            is_platform(&target)
                || pretty == "self.version"
                || constraint::parse(pretty).is_ok_and(|c| {
                    (versions.get(&target).into_iter().flatten()).any(|v| c.matches(v))
                })
        })
    })
}

/// `InstalledRepository::findPackagesWithReplacersAndProviders`, first match: the package `name` at a version
/// `constraint` accepts, or one that provides or replaces it with a link `constraint` intersects.
fn find<'a>(
    arena: &'a [Package],
    repo: &[usize],
    name: &str,
    constraint: Option<&Constraint>,
) -> Option<&'a Package> {
    let accepts = |c: &Constraint| constraint.is_none_or(|want| want.matches(c));
    (repo.iter().map(|&p| &arena[p])).find(|p| {
        if p.name == name {
            return accepts(&Constraint::exact(&p.version));
        }
        (p.provides.iter().chain(&p.replaces)).any(|l| l.target == name && accepts(&l.constraint))
    })
}

/// How the lock has `target`: the package's version, or the replace or provide link that stands for it.
fn described(p: &Package, target: &str) -> String {
    if p.name != target {
        for (links, text) in [(&p.replaces, "replaced"), (&p.provides, "provided")] {
            if let Some(l) = links.iter().find(|l| l.target == target) {
                return format!("{text} as {} by {}", l.pretty, at(p));
            }
        }
    }
    p.pretty_version.clone()
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
        // Each failed package with the index of its problem.
        let mut problem_of: Vec<(usize, usize)> = vec![];
        for &p in locked {
            let package = &check.arena[p];
            if let Some(of) = package.alias_of {
                // A locked alias needs its package (`RULE_PACKAGE_ALIAS`), so it fails with it, as its own problem.
                // Composer 2.11.2 reports none for the alias of a branch (fixture I, `dev-main as 1.0.0`), so neither
                // does Pun; the alias of a release gets one (fixture Q).
                let release = !check.arena[of].version.starts_with("dev-")
                    && !check.arena[of].version.ends_with("-dev");
                if let Some(&(_, i)) = problem_of.iter().find(|(f, _)| *f == of && release) {
                    let original = &problems[i];
                    let lines = vec![
                        check.fixed_line(p),
                        original.lines[1].clone(),
                        format!(
                            "{} is an alias of {} and thus requires it to be installed too.",
                            at(package),
                            at(&check.arena[of])
                        ),
                    ];
                    let (required, by_lock) = (original.required.clone(), original.by_lock);
                    problems.push(Problem {
                        lines,
                        required,
                        by_lock,
                    });
                    failed.push(p);
                }
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
                    problem_of.push((p, problems.len() - 1));
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
        // `Rule::getPrettyString` names the target by the conflict's constraint only when the source comes later in
        // the pool (root, platform, then the lock), and else by its version.
        let later = source > other;
        let other = &self.arena[other];
        let mut target = if later {
            format!("{} {}", other.pretty_name, link.pretty)
        } else {
            at(other)
        };
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
        // Only platform names get here (Composer's install from a lock requires no other root package), and the
        // platform has one package of each name, so Composer's list of versions and their aliases is one version.
        let Some(found) = (self.fixed.iter())
            .map(|&p| &self.arena[p])
            .find(|p| p.name == name)
        else {
            return (
                format!("Root composer.json requires {name}, it "),
                "could not be found in any version, there may be a typo in the package name."
                    .into(),
            );
        };
        let root = if found.origin == Origin::Root {
            " See https://getcomposer.org/dep-on-root for details and assistance."
        } else {
            ""
        };
        (
            format!("Root composer.json requires {name} {pretty}, "),
            format!(
                "found {}[{}] but it does not match the constraint.{root}",
                found.pretty_name, found.pretty_version
            ),
        )
    }
}

/// `SolverProblemsException::getPrettyString`.
fn report(problems: &[Problem], ini: &[String]) -> String {
    let mut text = String::from("\n");
    // `array_unique` over the problems' texts.
    let mut bodies: Vec<String> = (problems.iter())
        .map(|p| format!("\n    - {}\n", p.lines.join("\n    - ")))
        .collect();
    let mut seen = std::collections::HashSet::new();
    bodies.retain(|b| seen.insert(b.clone()));
    for (i, body) in bodies.iter().enumerate() {
        let _ = write!(text, "  Problem {}{body}", i + 1);
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
