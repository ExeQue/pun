//! The API an extension implements to add rules for a framework or tool to Pun's commands.

pub use pun_config::extensions::ExtensionMeta;
pub use pun_format::FixerRegistry;
pub use pun_lint::RuleRegistry;

pub trait Extension: Send + Sync {
    fn meta(&self) -> &'static ExtensionMeta;

    /// Adds the extension's format fixers.
    fn format(&self, _: &mut FixerRegistry) {}

    /// Adds the extension's lint rules, which must sit in groups its meta owns.
    fn lint(&self, _: &mut RuleRegistry) {}
}

/// Every lint rule a run can turn on: the core's, then each extension's, in `extensions`' order. The order is the
/// order `Linter` applies fixes at the same offset in, so it is part of the output.
pub fn lint_rules(extensions: &[&dyn Extension]) -> Vec<Box<dyn pun_lint::Rule>> {
    let mut registry = RuleRegistry::default();
    registry.add(pun_lint::rules::core());
    for e in extensions {
        e.lint(&mut registry);
    }
    registry.into_rules()
}

/// Every format fixer a run can turn on: the core's, then each extension's.
pub fn fixers(extensions: &[&dyn Extension]) -> Vec<Box<dyn pun_format::Fixer>> {
    let mut registry = FixerRegistry::default();
    registry.add(pun_fixers::core());
    for e in extensions {
        e.format(&mut registry);
    }
    registry.into_fixers()
}
