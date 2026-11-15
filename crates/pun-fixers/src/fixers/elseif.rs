use pun_format::Fixer;
use pun_tokens::Kind::*;
use pun_tokens::{Token, Tokens};
use std::path::Path;

struct ElseifFixer;

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(ElseifFixer)
}

impl Fixer for ElseifFixer {
    fn name(&self) -> &'static str {
        "elseif"
    }
    fn priority(&self) -> i32 {
        40
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_all_token_kinds_found(&[T_IF, T_ELSE])
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        let mut index = 0;
        while index < tokens.len() {
            if tokens[index].is_kind(T_ELSE) {
                fix_else(tokens, index);
            }
            index += 1;
        }
    }
}

fn fix_else(tokens: &mut Tokens, index: usize) {
    let if_token_index = match tokens.get_next_meaningful_token(index) {
        Some(i) if tokens[i].is_kind(T_IF) => i,
        _ => return,
    };
    let open = tokens.next_meaningful(if_token_index);
    let condition_end_brace_index = tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, open);
    let after_condition_index = tokens.next_meaningful(condition_end_brace_index);
    if tokens[after_condition_index].equals(":", true) {
        return;
    }
    tokens.clear_at(index + 1);
    tokens.set(index, Token::new(T_ELSEIF, "elseif"));
    tokens.clear_at(if_token_index);
    let before_if_token_index = tokens
        .get_prev_non_whitespace(if_token_index, None)
        .unwrap();
    if tokens[before_if_token_index].is_comment() && tokens[if_token_index + 1].is_whitespace(None)
    {
        tokens.clear_at(if_token_index + 1);
    }
}
