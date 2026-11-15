//! The API an extension implements to add rules for a framework or tool to Pun's commands.

pub use pun_config::extensions::ExtensionMeta;
pub use pun_lint::RuleRegistry;

pub trait Extension: Send + Sync {
    fn meta(&self) -> &'static ExtensionMeta;

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
