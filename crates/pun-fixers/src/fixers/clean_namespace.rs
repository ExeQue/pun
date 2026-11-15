use pun_format::Fixer;
use pun_tokens::{Kind::*, Tokens};
use std::path::Path;

struct CleanNamespaceFixer;

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(CleanNamespaceFixer)
}

const PHP_VERSION_ID: u32 = 8_05_00;

impl Fixer for CleanNamespaceFixer {
    fn name(&self) -> &'static str {
        "clean_namespace"
    }
    fn priority(&self) -> i32 {
        10
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        PHP_VERSION_ID < 8_00_00 && tokens.is_token_kind_found(T_NS_SEPARATOR)
    }
    fn fix(&self, _file: &Path, _tokens: &mut Tokens) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_a_candidate_on_php_8() {
        let tokens = Tokens::from_code(b"<?php\nuse Foo\\{A, B};\n");
        assert!(tokens.is_token_kind_found(T_NS_SEPARATOR));
        assert!(!fixer().is_candidate(&tokens));
        let mut tokens = tokens;
        fixer().fix(Path::new("t.php"), &mut tokens);
        assert!(!tokens.is_changed());
    }
}
