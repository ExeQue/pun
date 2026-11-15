use serde::{Serialize, Serializer};

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
