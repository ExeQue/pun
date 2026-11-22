//! PHPStan 2.3.5's own class-likes (`data/phpstan-api.php`, written by scripts/export-phpstan-api-stub.php).

use std::collections::HashMap;
use std::sync::LazyLock;

pub const PHPSTAN_API: &[u8] = include_bytes!("../data/phpstan-api.php");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    Interface,
    Trait,
}

static DECLARED: LazyLock<HashMap<String, ClassKind>> = LazyLock::new(|| {
    let mut declared = HashMap::new();
    let mut namespace = String::new();
    for line in String::from_utf8_lossy(PHPSTAN_API).lines() {
        if let Some(name) = line.strip_prefix("namespace ") {
            namespace = name.trim_end_matches(';').to_ascii_lowercase();
            continue;
        }
        let mut words = line
            .split(' ')
            .skip_while(|w| matches!(*w, "final" | "abstract" | "readonly"));
        let kind = match words.next() {
            Some("class") => ClassKind::Class,
            Some("interface") => ClassKind::Interface,
            Some("trait") => ClassKind::Trait,
            _ => continue,
        };
        let name = words.next().unwrap_or_default();
        let name = name.split(':').next().unwrap_or_default();
        let class = format!("{namespace}\\{}", name.to_ascii_lowercase());
        declared.insert(class, kind);
    }
    declared
});

pub fn class_kind(name: &str) -> Option<ClassKind> {
    let name = name.trim_start_matches('\\').to_ascii_lowercase();
    DECLARED.get(&name).copied()
}
