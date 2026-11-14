use pun_format::Fixer;
use pun_tokens::Kind::*;
use pun_tokens::{Token, Tokens};
use std::path::Path;

struct NoUselessNullsafeOperatorFixer;

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(NoUselessNullsafeOperatorFixer)
}

impl Fixer for NoUselessNullsafeOperatorFixer {
    fn name(&self) -> &'static str {
        "no_useless_nullsafe_operator"
    }
    fn priority(&self) -> i32 {
        0
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_all_token_kinds_found(&[T_VARIABLE, T_NULLSAFE_OBJECT_OPERATOR])
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        let mut index = tokens.len() as isize - 1;
        while index >= 0 {
            let i = index as usize;
            index -= 1;
            if !tokens[i].is_kind(T_NULLSAFE_OBJECT_OPERATOR) {
                continue;
            }
            let prev = tokens.get_prev_meaningful_token(i).unwrap();
            index = prev as isize - 1;
            if !tokens[prev].is_kind(T_VARIABLE)
                || pun_tokens::php_compat::strtolower(tokens[prev].get_content()) != b"$this"
            {
                continue;
            }
            tokens.set(i, Token::new(T_OBJECT_OPERATOR, "->"));
        }
    }
}
