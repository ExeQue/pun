use crate::installed::Installed;
use crate::php::json_pretty_escaped;
use crate::version::name_matches;
use serde_json::{Map, Value};

/// The plugins pun runs itself instead of in the plugin host: pest-plugin through its port, and php-http/discovery,
/// which changes nothing on install.
pub const NATIVE: &[&str] = &["pestphp/pest-plugin", "php-http/discovery"];

pub fn allowed(config: &Map<String, Value>, name: &str) -> bool {
    match config.get("allow-plugins") {
        Some(Value::Bool(b)) => *b,
        Some(Value::Object(rules)) => (rules.iter())
            .find(|(pattern, _)| name_matches(pattern, name))
            .is_some_and(|(_, allow)| allow.as_bool() == Some(true)),
        _ => false,
    }
}

/// The files the native ports write after the autoload dump.
pub fn after_dump(root: &Map<String, Value>, packages: &[&Installed]) -> Vec<(String, Vec<u8>)> {
    let config = root
        .get("config")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut out = vec![];
    for p in packages {
        let name = p.name();
        if name == "pestphp/pest-plugin"
            && p.package.get("type").and_then(Value::as_str) == Some("composer-plugin")
            && allowed(&config, &name)
        {
            out.push(("pest-plugins.json".into(), pest_plugins(root, packages)));
        }
    }
    out
}

fn pest_plugins(root: &Map<String, Value>, packages: &[&Installed]) -> Vec<u8> {
    let mut plugins = vec![];
    for extra in packages
        .iter()
        .map(|p| p.package.get("extra"))
        .chain([root.get("extra")])
    {
        if let Some(Value::Array(list)) = extra
            .and_then(|e| e.get("pest"))
            .and_then(|p| p.get("plugins"))
        {
            plugins.extend(list.iter().cloned());
        }
    }
    json_pretty_escaped(&Value::Array(plugins)).into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn package(p: Value) -> Installed {
        Installed {
            package: crate::lock::LockedPackage(serde_json::from_value(p).unwrap()),
            path: None,
            dev: false,
        }
    }

    #[test]
    fn allow_plugins_rules() {
        let config = |v: Value| json!({"allow-plugins": v}).as_object().unwrap().clone();
        assert!(allowed(&config(json!(true)), "acme/x"));
        assert!(!allowed(&config(json!(false)), "acme/x"));
        assert!(!allowed(&Map::new(), "acme/x"));
        let rules = config(json!({"acme/off": false, "acme/*": true, "*/pest-plugin": true}));
        assert!(allowed(&rules, "acme/x") && allowed(&rules, "Pestphp/Pest-Plugin"));
        assert!(!allowed(&rules, "acme/off") && !allowed(&rules, "other/x"));
        let rules = config(json!({"acme/*": true, "acme/off": false}));
        assert!(allowed(&rules, "acme/off"));
        let rules = config(json!({"a*b*c": true}));
        assert!(allowed(&rules, "AxxByyC") && !allowed(&rules, "axxc"));
    }

    #[test]
    fn after_dump_runs_the_allowed_ports() {
        let root = json!({"extra": {"pest": {"plugins": ["Root\\P"]}}, "config": {"allow-plugins": {"pestphp/pest-plugin": true, "php-http/discovery": true, "acme/plugin": true}}});
        let packages = [
            package(json!({"name": "pestphp/pest-plugin", "type": "composer-plugin"})),
            package(json!({"name": "php-http/discovery", "type": "composer-plugin"})),
            package(json!({"name": "acme/plugin", "type": "composer-plugin"})),
            package(json!({"name": "acme/denied", "type": "composer-plugin"})),
            package(json!({"name": "acme/lib", "extra": {"pest": {"plugins": ["A\\B"]}}})),
        ];
        let refs: Vec<&Installed> = packages.iter().collect();
        let out = after_dump(root.as_object().unwrap(), &refs);
        assert_eq!(
            out,
            [(
                "pest-plugins.json".to_string(),
                b"[\n    \"A\\\\B\",\n    \"Root\\\\P\"\n]".to_vec()
            )]
        );
    }
}
