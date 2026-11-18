use pun_format::Fixer;
use pun_tokens::Kind::*;
use pun_tokens::tokens_analyzer::{ClassyElementType, TokensAnalyzer};
use pun_tokens::{Token, Tokens};
use std::collections::{BTreeMap, HashMap};
use serde_json::Value;
use std::path::Path;

struct ModernSerializationMethodsFixer {
    sleep: bool,
    wakeup: bool,
}

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(ModernSerializationMethodsFixer {
        sleep: true,
        wakeup: true,
    })
}

/// The values of `lint/style/no-sleep-wakeup`'s `checks`, Pun's own option, as pun-config hands them over: Rector's
/// rules (without `Rector`), which `pun migrate` keeps to the ones rector.php runs.
const CHECKS: [&str; 2] = ["SleepToSerialize", "WakeupToUnserialize"];

impl Fixer for ModernSerializationMethodsFixer {
    fn name(&self) -> &'static str {
        "modern_serialization_methods"
    }
    fn priority(&self) -> i32 {
        0
    }
    fn is_risky(&self) -> bool {
        true
    }
    fn configure(&mut self, options: &Value) -> anyhow::Result<()> {
        // `checks` is Pun's own option, so PHP-CS-Fixer's error keeps saying the fixer has none.
        let mut upstream = options.clone();
        let checks = (upstream.as_object_mut()).and_then(|o| o.remove("checks"));
        if upstream.as_object().is_some_and(serde_json::Map::is_empty) {
            upstream = Value::Null;
        }
        if !upstream.is_null() {
            anyhow::bail!("[{}] Is not configurable.", self.name());
        }
        let Some(checks) = checks else {
            return Ok(());
        };
        let names = (checks.as_array().into_iter().flatten())
            .map(|v| v.as_str().filter(|s| CHECKS.contains(s)))
            .collect::<Option<Vec<&str>>>()
            .filter(|_| checks.is_array());
        let Some(names) = names else {
            anyhow::bail!("[{}] checks must be a list of {CHECKS:?}, not {checks}", self.name());
        };
        self.sleep = names.contains(&"SleepToSerialize");
        self.wakeup = names.contains(&"WakeupToUnserialize");
        Ok(())
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_any_token_kinds_found(&[T_CLASS, T_FUNCTION])
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        let mut methods_by_class: BTreeMap<usize, HashMap<Vec<u8>, usize>> = BTreeMap::new();
        for (index, element) in TokensAnalyzer::new(tokens).get_classy_elements() {
            if element.kind != ClassyElementType::Method {
                continue;
            }
            let function_name_index = tokens.next_meaningful(index);
            methods_by_class
                .entry(element.class_index)
                .or_default()
                .insert(
                    tokens[function_name_index].get_content().to_vec(),
                    function_name_index,
                );
        }
        let mut tokens_to_insert: BTreeMap<usize, Vec<Token>> = BTreeMap::new();
        for methods in methods_by_class.values() {
            if let Some(&sleep) = methods.get(&b"__sleep"[..])
                && self.sleep
                && !methods.contains_key(&b"__serialize"[..])
            {
                tokens.set(sleep, Token::new(T_STRING, "__serialize"));
            }
            if let Some(&wakeup) = methods.get(&b"__wakeup"[..])
                && self.wakeup
                && !methods.contains_key(&b"__unserialize"[..])
            {
                tokens.set(wakeup, Token::new(T_STRING, "__unserialize"));
                let open_parenthesis_index = tokens
                    .get_next_token_of_kind(wakeup, &["(".into()], true)
                    .unwrap();
                tokens_to_insert.insert(
                    open_parenthesis_index + 1,
                    vec![
                        Token::new(CT_ARRAY_TYPEHINT, "array"),
                        Token::new(T_WHITESPACE, " "),
                        Token::new(T_VARIABLE, "$data"),
                    ],
                );
            }
        }
        if !tokens_to_insert.is_empty() {
            tokens.insert_slices(tokens_to_insert);
        }
    }
}

#[cfg(test)]
mod tests {
    fn fix(options: serde_json::Value, code: &str) -> String {
        let mut fixer = super::fixer();
        fixer.configure(&options).unwrap();
        crate::fix_with(&*fixer, code)
    }

    /// `checks` keeps the fixer to the Rector rules rector.php runs. Rector 2.8.4, with one of `SleepToSerializeRector`
    /// and `WakeupToUnserializeRector` in `withSkip()`, leaves the other method alone; the renamed method is
    /// PHP-CS-Fixer's `modern_serialization_methods` output.
    #[test]
    fn checks_keeps_to_the_rector_rules_that_run() {
        let sleep = "<?php\n\nfinal class Sleeps\n{\n    public $a;\n\n    public function __sleep()\n    {\n        return ['a'];\n    }\n}\n";
        let wakeup = "\nfinal class Wakes\n{\n    public $a;\n\n    public function __wakeup()\n    {\n        $this->a = 1;\n    }\n}\n";
        let code = format!("{sleep}{wakeup}");
        let serialize = sleep.replace("__sleep()", "__serialize()");
        let unserialize = wakeup.replace("__wakeup()", "__unserialize(array $data)");
        let checks = |c: &[&str]| serde_json::json!({ "checks": c });
        assert_eq!(
            fix(checks(&["SleepToSerialize"]), &code),
            format!("{serialize}{wakeup}")
        );
        assert_eq!(
            fix(checks(&["WakeupToUnserialize"]), &code),
            format!("{sleep}{unserialize}")
        );
        assert_eq!(fix(checks(&[]), &code), code);
        assert_eq!(
            fix(serde_json::Value::Null, &code),
            format!("{serialize}{unserialize}")
        );
        for bad in [
            serde_json::json!({ "checks": ["Sleep"] }),
            serde_json::json!({ "checks": "SleepToSerialize" }),
        ] {
            let error = super::fixer().configure(&bad).unwrap_err().to_string();
            assert!(error.starts_with("[modern_serialization_methods] checks must be a list of"), "{error}");
        }
        let error = super::fixer().configure(&serde_json::json!({ "x": 1 }));
        assert_eq!(
            error.unwrap_err().to_string(),
            "[modern_serialization_methods] Is not configurable."
        );
    }
}
