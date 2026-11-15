//! The built-in extensions, as Mago lists its built-in plugins.

use pun_config::extensions as meta;
use pun_extension::{Extension, ExtensionMeta, RuleRegistry};

struct Laravel;
struct Pest;
struct Phpunit;
struct Doctrine;

impl Extension for Laravel {
    fn meta(&self) -> &'static ExtensionMeta {
        &meta::LARAVEL
    }

    fn lint(&self, r: &mut RuleRegistry) {
        r.add(pun_lint::rules::laravel());
    }
}

impl Extension for Pest {
    fn meta(&self) -> &'static ExtensionMeta {
        &meta::PEST
    }

    fn lint(&self, r: &mut RuleRegistry) {
        r.add(pun_lint::rules::pest());
    }
}

impl Extension for Phpunit {
    fn meta(&self) -> &'static ExtensionMeta {
        &meta::PHPUNIT
    }

    fn lint(&self, r: &mut RuleRegistry) {
        r.add(pun_lint::rules::phpunit());
    }
}

impl Extension for Doctrine {
    fn meta(&self) -> &'static ExtensionMeta {
        &meta::DOCTRINE
    }
}

pub(crate) static ALL: [&dyn Extension; 4] = [&Laravel, &Pest, &Phpunit, &Doctrine];

pub(crate) fn lint_rules() -> Vec<Box<dyn pun_lint::Rule>> {
    pun_extension::lint_rules(&ALL)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(id: &str) -> &str {
        id.split('/').nth(1).unwrap()
    }

    #[test]
    fn extensions_keep_to_their_groups() {
        let metas: Vec<_> = ALL.iter().map(|e| e.meta()).collect();
        assert_eq!(metas, meta::ALL);
        for rule in pun_lint::rules::core() {
            assert_eq!(meta::owner(group(rule.id())), None, "{}", rule.id());
        }
        for e in ALL {
            let mut r = RuleRegistry::default();
            e.lint(&mut r);
            for rule in r.into_rules() {
                let owner = meta::owner(group(rule.id())).map(|o| o.0);
                assert_eq!(owner, Some(e.meta()), "{}", rule.id());
            }
        }
        let ids = |rules: Vec<Box<dyn pun_lint::Rule>>| -> Vec<_> {
            rules.iter().map(|r| r.id()).collect()
        };
        assert_eq!(ids(lint_rules()), ids(pun_lint::rules::all()));
    }
}
