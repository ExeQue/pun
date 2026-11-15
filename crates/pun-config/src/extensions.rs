use anyhow::{Result, bail};
use serde::{Serialize, Serializer};
use std::collections::BTreeMap;

/// What Pun knows about an extension before it runs: its name, the rule groups it owns and the locked packages that
/// turn it on.
#[derive(Debug, PartialEq, Eq)]
pub struct ExtensionMeta {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    /// The rule groups it owns. Each also owns its version groups, such as `laravel-12`.
    pub groups: &'static [&'static str],
    /// The locked packages that turn it on, the first of them also gating its version groups.
    pub packages: &'static [&'static str],
}

impl Serialize for ExtensionMeta {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.id)
    }
}

pub static LARAVEL: ExtensionMeta = ExtensionMeta {
    id: "laravel",
    name: "Laravel",
    description: "Laravel's rules and Larastan's analysis",
    groups: &["laravel"],
    packages: &["laravel/framework"],
};

pub static PEST: ExtensionMeta = ExtensionMeta {
    id: "pest",
    name: "Pest",
    description: "Rules for Pest tests",
    groups: &["pest"],
    packages: &["pestphp/pest"],
};

pub static PHPUNIT: ExtensionMeta = ExtensionMeta {
    id: "phpunit",
    name: "PHPUnit",
    description: "Rules for PHPUnit tests",
    groups: &["phpunit"],
    packages: &["phpunit/phpunit"],
};

pub static DOCTRINE: ExtensionMeta = ExtensionMeta {
    id: "doctrine",
    name: "Doctrine",
    description: "Rules for Doctrine annotations",
    groups: &["doctrine"],
    packages: &[],
};

pub static ALL: [&ExtensionMeta; 4] = [&LARAVEL, &PEST, &PHPUNIT, &DOCTRINE];

/// The extension that owns `group`, and the version in the group's name when it is a version group.
pub(crate) fn owner(group: &str) -> Option<(&'static ExtensionMeta, Option<&str>)> {
    ALL.into_iter().find_map(|e| {
        e.groups.iter().find_map(|g| {
            let rest = group.strip_prefix(g)?;
            if rest.is_empty() {
                return Some((e, None));
            }
            Some((e, Some(rest.strip_prefix('-')?)))
        })
    })
}

/// Every package some extension turns on for.
pub(crate) fn packages() -> impl Iterator<Item = &'static str> {
    ALL.into_iter().flat_map(|e| e.packages.iter().copied())
}

/// An `[extensions]` value: `auto`, the default, turns an extension on when the lock has one of its packages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Switch {
    Auto,
    On,
    Off,
}

/// The `[extensions]` table, keyed by extension ID.
pub(crate) fn switches(at: &str, t: toml::Table) -> Result<BTreeMap<&'static str, Switch>> {
    (t.into_iter())
        .map(|(name, v)| {
            let Some(e) = ALL.into_iter().find(|e| e.id == name) else {
                let ids: Vec<String> = ALL.iter().map(|e| format!("`{}`", e.id)).collect();
                bail!(
                    "{at}: [extensions] `{name}` is not an extension: expected {}",
                    ids.join(", ")
                );
            };
            let s = match v.as_str() {
                Some("auto") => Switch::Auto,
                Some("on") => Switch::On,
                Some("off") => Switch::Off,
                _ => {
                    bail!("{at}: [extensions] `{name}` = {v}: expected \"auto\", \"on\" or \"off\"")
                }
            };
            Ok((e.id, s))
        })
        .collect()
}
