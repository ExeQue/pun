use crate::Exit;
use crate::install::{fetcher, project};
use crate::render::{ColorChoice, Look, Role, plural};
use anyhow::{Context, Result, bail};
use pun_packages::fetch::{Fetched, Fetcher, unpack};
use pun_packages::resolver::composer_repo::{PACKAGIST, expand};
use pun_packages::version::{compare, normalize};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
pub(crate) struct VetArgs {
    #[command(subcommand)]
    command: Option<VetCommand>,
}

#[derive(clap::Subcommand)]
enum VetCommand {
    /// Create supply-chain/ with every locked package exempted at its locked version.
    Init,
    /// Record that you reviewed a version (`<package> <version>`) or the changes between two (`<package> <from>
    /// <to>`).
    Certify {
        package: String,
        /// The version reviewed, or the version the reviewed changes start from.
        version: String,
        /// The version the reviewed changes lead to.
        to: Option<String>,
        /// What the review vouches for; the default is what the lock needs of the package.
        #[arg(long, value_enum)]
        criteria: Option<Criteria>,
        /// Who reviewed it; the default is git's user.name and user.email.
        #[arg(long)]
        who: Option<String>,
        /// A note kept with the review.
        #[arg(long)]
        notes: Option<String>,
    },
    /// Show the changes between two versions of a package, to review them.
    Diff {
        package: String,
        from: String,
        to: String,
    },
    /// Unpack a version of a package into Pun's cache and print where, to review it.
    Inspect { package: String, version: String },
    /// Remove the exemptions the lock no longer needs.
    Prune,
    /// Fetch every imported audit file again and write imports.lock.
    FetchImports,
}

/// What a review vouches for. Safe-to-deploy implies safe-to-run.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, clap::ValueEnum,
)]
#[serde(rename_all = "kebab-case")]
enum Criteria {
    SafeToRun,
    SafeToDeploy,
}

impl Criteria {
    fn name(self) -> &'static str {
        match self {
            Criteria::SafeToRun => "safe-to-run",
            Criteria::SafeToDeploy => "safe-to-deploy",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Audit {
    who: Option<String>,
    criteria: Criteria,
    /// A full audit of this version.
    version: Option<String>,
    /// An audit of the changes `from -> to`.
    delta: Option<String>,
    notes: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Exemption {
    version: String,
    criteria: Criteria,
    notes: Option<String>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    imports: BTreeMap<String, Import>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    exemptions: BTreeMap<String, Vec<Exemption>>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Import {
    /// An https URL, or a path from supply-chain/.
    url: String,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Audits {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    audits: BTreeMap<String, Vec<Audit>>,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ImportsLock {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    imports: BTreeMap<String, LockedImport>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct LockedImport {
    url: String,
    #[serde(default)]
    audits: BTreeMap<String, Vec<Audit>>,
}

/// `supply-chain/` read.
struct Store {
    dir: PathBuf,
    config: Config,
    audits: Audits,
    imports: ImportsLock,
}

const HEADER: &str = "# pun vet: https://pun.laravel.cloud/docs/commands/vet\n\n";

impl Store {
    fn read(root: &Path) -> Result<Option<Store>> {
        let dir = root.join("supply-chain");
        if !dir.is_dir() {
            return Ok(None);
        }
        Ok(Some(Store {
            config: read_toml(&dir.join("config.toml"))?,
            audits: read_toml(&dir.join("audits.toml"))?,
            imports: read_toml(&dir.join("imports.lock"))?,
            dir,
        }))
    }

    fn write(&self) -> Result<()> {
        std::fs::create_dir_all(&self.dir)
            .with_context(|| format!("can't create {}", self.dir.display()))?;
        write_toml(&self.dir.join("config.toml"), &self.config)?;
        write_toml(&self.dir.join("audits.toml"), &self.audits)?;
        write_toml(&self.dir.join("imports.lock"), &self.imports)
    }

    /// Every audit of `name`: the project's own, then the imported ones.
    fn audits_of(&self, name: &str) -> Vec<&Audit> {
        let own = self.audits.audits.get(name).into_iter().flatten();
        let imported = (self.imports.imports.values())
            .filter_map(|i| i.audits.get(name))
            .flatten();
        own.chain(imported).collect()
    }
}

fn read_toml<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
        r => {
            let text = r.with_context(|| format!("can't read {}", path.display()))?;
            toml::from_str(&text).with_context(|| format!("invalid {}", path.display()))
        }
    }
}

fn write_toml<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let text = format!("{HEADER}{}", toml::to_string(value)?);
    std::fs::write(path, text).with_context(|| format!("can't write {}", path.display()))
}

/// A locked package and what it needs.
struct Need {
    name: String,
    version: String,
    criteria: Criteria,
}

/// The packages the lock needs vetted: `require` packages at safe-to-deploy, `require-dev` ones at safe-to-run.
/// Path packages are the project's own and metapackages hold no code, so neither needs a review.
fn needs(lock: &pun_packages::lock::Lock) -> Vec<Need> {
    let sections = [
        (&lock.packages, Criteria::SafeToDeploy),
        (&lock.packages_dev, Criteria::SafeToRun),
    ];
    let mut out = vec![];
    for (packages, criteria) in sections {
        for p in packages {
            let dist = p.dist().and_then(|d| d.get("type")).and_then(Value::as_str);
            if dist == Some("path") || p.kind() == Some("metapackage") {
                continue;
            }
            out.push(Need {
                name: p.name(),
                version: version_id(p),
                criteria,
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// A locked version as reviews name it: the version, and for a branch its commit (`dev-main#<reference>`), since a
/// branch moves.
fn version_id(p: &Map<String, Value>) -> String {
    let version = p.get("version").and_then(Value::as_str).unwrap_or_default();
    if !(version.starts_with("dev-") || version.ends_with("-dev")) {
        return version.to_string();
    }
    let reference = ["source", "dist"]
        .iter()
        .find_map(|k| p.get(*k)?.get("reference")?.as_str())
        .unwrap_or_default();
    format!("{version}#{reference}")
}

/// A version in the form reviews compare: `v1.2` and `1.2.0` are one version.
fn key(version: &str) -> String {
    let (v, reference) = version
        .split_once('#')
        .map_or((version, None), |(v, r)| (v, Some(r)));
    let v = normalize(v).unwrap_or_else(|_| v.to_string());
    match reference {
        Some(r) => format!("{v}#{r}"),
        None => v,
    }
}

#[derive(Debug, PartialEq)]
enum Verdict {
    Audited,
    Exempted,
    /// Not vetted; the reviewed version nearest to it, to review the changes from.
    Unvetted(Option<String>),
}

/// Whether a chain of reviews that meet `need` leads to its version from a full audit, or else from an exemption.
fn verdict(need: &Need, audits: &[&Audit], exemptions: &[Exemption]) -> Verdict {
    let fits = |c: Criteria| c >= need.criteria;
    let mut shown: HashMap<String, String> = HashMap::new();
    let mut node = |v: &str| {
        let k = key(v);
        shown.entry(k.clone()).or_insert_with(|| v.to_string());
        k
    };
    let mut edges: HashMap<String, Vec<String>> = HashMap::new();
    let mut audited = vec![];
    for a in audits.iter().filter(|a| fits(a.criteria)) {
        if let Some(v) = &a.version {
            audited.push(node(v));
        }
        if let Some((from, to)) = a.delta.as_deref().and_then(|d| d.split_once("->")) {
            let (from, to) = (node(from.trim()), node(to.trim()));
            edges.entry(from).or_default().push(to);
        }
    }
    let exempted: Vec<String> = (exemptions.iter())
        .filter(|e| fits(e.criteria))
        .map(|e| node(&e.version))
        .collect();
    let reach = |starts: &[String]| -> HashSet<String> {
        let mut seen: HashSet<String> = starts.iter().cloned().collect();
        let mut queue: VecDeque<String> = starts.iter().cloned().collect();
        while let Some(v) = queue.pop_front() {
            for next in edges.get(&v).into_iter().flatten() {
                if seen.insert(next.clone()) {
                    queue.push_back(next.clone());
                }
            }
        }
        seen
    };
    let target = key(&need.version);
    if reach(&audited).contains(&target) {
        return Verdict::Audited;
    }
    let all: Vec<String> = audited.iter().chain(&exempted).cloned().collect();
    let reached = reach(&all);
    if reached.contains(&target) {
        return Verdict::Exempted;
    }
    let below = (reached.iter())
        .filter(|v| compare(v, &target) < 0)
        .max_by(|a, b| compare(a, b).cmp(&0));
    let above = (reached.iter())
        .filter(|v| compare(v, &target) > 0)
        .min_by(|a, b| compare(a, b).cmp(&0));
    Verdict::Unvetted(below.or(above).map(|k| shown[k].clone()))
}

pub(crate) fn run(a: VetArgs) -> Result<Exit> {
    let project = project(None, false)?;
    let lock = (project.lock.as_ref())
        .with_context(|| "no lock to vet: run `pun install` or `pun update` first".to_string())?;
    let needs = needs(lock);
    let store = Store::read(&project.root)?;
    match a.command {
        None => check(store, &needs, &project.root, &project.manifest),
        Some(VetCommand::Init) => init(&project.root, &needs),
        Some(VetCommand::Certify {
            package,
            version,
            to,
            criteria,
            who,
            notes,
        }) => {
            let mut store = store.context(NO_STORE)?;
            let name = package.to_ascii_lowercase();
            let criteria = criteria
                .or_else(|| needs.iter().find(|n| n.name == name).map(|n| n.criteria))
                .unwrap_or(Criteria::SafeToDeploy);
            let who = who.or_else(git_who);
            let (version, delta, what) = match to {
                Some(to) => (
                    None,
                    Some(format!("{version} -> {to}")),
                    format!("from {version} to {to}"),
                ),
                None => (Some(version.clone()), None, version),
            };
            store
                .audits
                .audits
                .entry(name.clone())
                .or_default()
                .push(Audit {
                    who,
                    criteria,
                    version,
                    delta,
                    notes,
                });
            store.write()?;
            println!("Recorded your {} review of {name} {what}", criteria.name());
            Ok(Exit::Success)
        }
        Some(VetCommand::Diff { package, from, to }) => {
            let fetcher = fetcher(&project.root, &project.manifest);
            let name = package.to_ascii_lowercase();
            let work = std::env::temp_dir().join(format!("pun-vet-{}", std::process::id()));
            let (a, b) = (work.join(&from), work.join(&to));
            let unpacked = unpack_version(&fetcher, lock, &name, &from, &a)
                .and_then(|()| unpack_version(&fetcher, lock, &name, &to, &b));
            let code = unpacked.and_then(|()| {
                let status = std::process::Command::new("git")
                    .current_dir(&work)
                    .args(["diff", "--no-index", "--", &from, &to])
                    .status()
                    .context("can't run git, which pun vet diff shows the changes with")?;
                Ok(match status.code() {
                    Some(0 | 1) => Exit::Success,
                    _ => Exit::RunError,
                })
            });
            let _ = std::fs::remove_dir_all(&work);
            code
        }
        Some(VetCommand::Inspect { package, version }) => {
            let fetcher = fetcher(&project.root, &project.manifest);
            let name = package.to_ascii_lowercase();
            let dir = crate::x::cache_dir()?
                .join("vet")
                .join(&name)
                .join(&version);
            if !dir.is_dir() {
                unpack_version(&fetcher, lock, &name, &version, &dir)?;
            }
            println!("{}", dir.display());
            Ok(Exit::Success)
        }
        Some(VetCommand::Prune) => {
            let mut store = store.context(NO_STORE)?;
            let before: usize = store.config.exemptions.values().map(Vec::len).sum();
            let mut kept: BTreeMap<String, Vec<Exemption>> = BTreeMap::new();
            for need in &needs {
                let audits = store.audits_of(&need.name);
                if verdict(need, &audits, &[]) == Verdict::Audited {
                    continue;
                }
                let exemptions = store.config.exemptions.get(&need.name);
                let useful = (exemptions.into_iter().flatten()).find(|e| {
                    verdict(need, &audits, std::slice::from_ref(*e)) == Verdict::Exempted
                });
                if let Some(e) = useful {
                    kept.entry(need.name.clone()).or_default().push(e.clone());
                }
            }
            let after: usize = kept.values().map(Vec::len).sum();
            store.config.exemptions = kept;
            store.write()?;
            println!(
                "Removed {} the lock no longer needs",
                plural(before - after, "exemption", "exemptions")
            );
            Ok(Exit::Success)
        }
        Some(VetCommand::FetchImports) => {
            let mut store = store.context(NO_STORE)?;
            let fetcher = fetcher(&project.root, &project.manifest);
            store.imports.imports.clear();
            fetch_imports(&mut store, &fetcher, &needs)?;
            store.write()?;
            println!(
                "Fetched {}",
                plural(store.imports.imports.len(), "import", "imports")
            );
            Ok(Exit::Success)
        }
    }
}

const NO_STORE: &str = "there is no supply-chain/ here yet: run `pun vet init` to create it";

/// `pun vet`: reports the packages no chain of reviews vets, with the command that shows what to review.
fn check(
    store: Option<Store>,
    needs: &[Need],
    root: &Path,
    manifest: &Map<String, Value>,
) -> Result<Exit> {
    let Some(mut store) = store else {
        bail!("{NO_STORE}, with every locked package exempted");
    };
    let missing = (store.config.imports.iter()).any(|(name, i)| {
        store
            .imports
            .imports
            .get(name)
            .is_none_or(|l| l.url != i.url)
    });
    if missing {
        fetch_imports(&mut store, &fetcher(root, manifest), needs)?;
        store.write()?;
    }
    let look = Look::stdout(ColorChoice::Auto);
    let (mut audited, mut exempted, mut unvetted) = (0, 0, vec![]);
    for need in needs {
        let exemptions = store
            .config
            .exemptions
            .get(&need.name)
            .map_or(&[][..], Vec::as_slice);
        match verdict(need, &store.audits_of(&need.name), exemptions) {
            Verdict::Audited => audited += 1,
            Verdict::Exempted => exempted += 1,
            Verdict::Unvetted(from) => unvetted.push((need, from)),
        }
    }
    if unvetted.is_empty() {
        let ok = look.paint(Role::Ok, look.glyph("✔", "ok"));
        println!(
            "{ok} Vetted {}: {audited} audited, {exempted} exempted",
            plural(needs.len(), "package", "packages")
        );
        return Ok(Exit::Success);
    }
    let cross = look.paint(Role::Error, look.glyph("✖", "x"));
    println!(
        "{cross} {} of {} {} not vetted:",
        unvetted.len(),
        plural(needs.len(), "package", "packages"),
        if unvetted.len() == 1 { "is" } else { "are" }
    );
    for (need, from) in &unvetted {
        println!(
            "\n  {} {} needs {}",
            need.name,
            need.version,
            need.criteria.name()
        );
        let command = match from {
            Some(from) => format!("pun vet diff {} {from} {}", need.name, need.version),
            None => format!("pun vet inspect {} {}", need.name, need.version),
        };
        println!("    review it: {}", look.paint(Role::Dim, &command));
    }
    println!(
        "\nRecord each review with `pun vet certify <package> <version>` (`<from> <to>` for changes), or exempt the \
         package in supply-chain/config.toml."
    );
    Ok(Exit::Failure)
}

/// `pun vet init`: supply-chain/ with every locked package exempted at its version and criterion.
fn init(root: &Path, needs: &[Need]) -> Result<Exit> {
    let dir = root.join("supply-chain");
    if dir.exists() {
        bail!("supply-chain/ exists already");
    }
    let mut store = Store {
        dir,
        config: Config::default(),
        audits: Audits::default(),
        imports: ImportsLock::default(),
    };
    for need in needs {
        store
            .config
            .exemptions
            .entry(need.name.clone())
            .or_default()
            .push(Exemption {
                version: need.version.clone(),
                criteria: need.criteria,
                notes: None,
            });
    }
    store.write()?;
    println!(
        "Wrote supply-chain/ with {} exempted; review them over time and record each with `pun vet certify`",
        plural(needs.len(), "package", "packages")
    );
    Ok(Exit::Success)
}

/// Fetches each imported audit file, keeping the audits of the packages the lock has.
fn fetch_imports(store: &mut Store, fetcher: &Fetcher, needs: &[Need]) -> Result<()> {
    let wanted: HashSet<&str> = needs.iter().map(|n| n.name.as_str()).collect();
    for (name, import) in &store.config.imports {
        let text = if import.url.contains("://") {
            let fetched = (fetcher.metadata(&import.url, None)).with_context(|| {
                format!("fetching the audits {name} imports from {}", import.url)
            })?;
            let Fetched::Body(body, _) = fetched else {
                bail!("{} has no audits to import", import.url);
            };
            String::from_utf8(body).with_context(|| format!("{} is not text", import.url))?
        } else {
            let path = store.dir.join(&import.url);
            std::fs::read_to_string(&path).with_context(|| {
                format!(
                    "can't read the audits {name} imports from {}",
                    path.display()
                )
            })?
        };
        let audits: Audits = toml::from_str(&text)
            .with_context(|| format!("{} is not an audits.toml", import.url))?;
        let audits = (audits.audits.into_iter())
            .filter(|(package, _)| wanted.contains(package.as_str()))
            .collect();
        store.imports.imports.insert(
            name.clone(),
            LockedImport {
                url: import.url.clone(),
                audits,
            },
        );
    }
    store
        .imports
        .imports
        .retain(|name, _| store.config.imports.contains_key(name));
    Ok(())
}

/// Unpacks `name` at `version` into `target`: the locked package when the lock has that version, else Packagist's.
fn unpack_version(
    fetcher: &Fetcher,
    lock: &pun_packages::lock::Lock,
    name: &str,
    version: &str,
    target: &Path,
) -> Result<()> {
    let wanted = key(version);
    let locked = (lock.all())
        .find(|p| {
            p.pretty_name().eq_ignore_ascii_case(name)
                && (key(&version_id(p)) == wanted || key(p.version()) == wanted)
        })
        .cloned();
    let package = match locked {
        Some(p) => p.0,
        None => packagist_version(fetcher, name, version)?,
    };
    let (bytes, _) = fetcher.archive(&package)?;
    let kind = (package
        .get("dist")
        .and_then(|d| d.get("type"))
        .and_then(Value::as_str))
    .unwrap_or("zip");
    unpack(kind, &bytes, target)?;
    Ok(())
}

/// The metadata Packagist publishes for `name` at `version`.
fn packagist_version(fetcher: &Fetcher, name: &str, version: &str) -> Result<Map<String, Value>> {
    let wanted = key(version.split_once('#').map_or(version, |(v, _)| v));
    for file in [name.to_string(), format!("{name}~dev")] {
        let url = format!("{PACKAGIST}/p2/{file}.json");
        let Fetched::Body(body, _) = fetcher.metadata(&url, None)? else {
            continue;
        };
        let data: Value =
            serde_json::from_slice(&body).with_context(|| format!("{url} is not JSON"))?;
        let Some(versions) = data
            .get("packages")
            .and_then(|p| p.get(name))
            .and_then(Value::as_array)
        else {
            continue;
        };
        let mut found = None;
        expand(versions, |v| {
            let pretty = v.get("version").and_then(Value::as_str).unwrap_or_default();
            if found.is_none() && key(pretty) == wanted {
                found = Some(v.clone());
            }
            Ok(())
        })?;
        if let Some(found) = found {
            return Ok(found);
        }
    }
    bail!("Packagist has no {name} {version}")
}

/// `Name <email>` from git's config.
fn git_who() -> Option<String> {
    let get = |k: &str| {
        let out = std::process::Command::new("git")
            .args(["config", k])
            .output()
            .ok()?;
        let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
        (!s.is_empty()).then_some(s)
    };
    let name = get("user.name")?;
    Some(match get("user.email") {
        Some(email) => format!("{name} <{email}>"),
        None => name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn audit(criteria: Criteria, version: Option<&str>, delta: Option<&str>) -> Audit {
        Audit {
            who: None,
            criteria,
            version: version.map(String::from),
            delta: delta.map(String::from),
            notes: None,
        }
    }

    fn need(version: &str, criteria: Criteria) -> Need {
        Need {
            name: "acme/lib".into(),
            version: version.into(),
            criteria,
        }
    }

    /// A delta chain vets a version from a full audit; a weaker criterion breaks the chain; an exemption vets only
    /// what no audit reaches; the suggestion is the nearest reviewed version, below before above.
    #[test]
    fn vets_through_chains_of_reviews() {
        use Criteria::{SafeToDeploy as D, SafeToRun as R};
        let full = audit(D, Some("v1.0.0"), None);
        let step = audit(D, None, Some("1.0.0 -> 1.1.0"));
        let weak = audit(R, None, Some("1.1.0 -> 1.2.0"));
        let audits = [&full, &step, &weak];
        assert_eq!(verdict(&need("1.1.0", D), &audits, &[]), Verdict::Audited);
        assert_eq!(verdict(&need("1.2.0", R), &audits, &[]), Verdict::Audited);
        assert_eq!(
            verdict(&need("1.2.0", D), &audits, &[]),
            Verdict::Unvetted(Some("1.1.0".into()))
        );
        let exempt = Exemption {
            version: "1.2.0".into(),
            criteria: D,
            notes: None,
        };
        assert_eq!(
            verdict(&need("v1.2.0", D), &audits, std::slice::from_ref(&exempt)),
            Verdict::Exempted
        );
        let late = audit(D, Some("2.0.0"), None);
        assert_eq!(
            verdict(&need("0.9.0", D), &[&late], &[]),
            Verdict::Unvetted(Some("2.0.0".into()))
        );
        assert_eq!(
            verdict(&need("1.0.0", D), &[], &[]),
            Verdict::Unvetted(None)
        );
    }
}
