use crate::analyzers::abstract_function_reference_fixer::{FunctionReferenceFinder, IS_RISKY};
use pun_format::Fixer;
use pun_tokens::Kind::*;
use pun_tokens::{Token, Tokens};
use std::path::Path;

struct DirConstantFixer;

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(DirConstantFixer)
}

impl Fixer for DirConstantFixer {
    fn name(&self) -> &'static str {
        "dir_constant"
    }
    fn priority(&self) -> i32 {
        40
    }
    fn is_risky(&self) -> bool {
        IS_RISKY
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_all_token_kinds_found(&[T_STRING, T_FILE])
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        let finder = FunctionReferenceFinder::new();
        let mut curr_index = 0;
        while let Some((function_name_index, open_parenthesis, close_parenthesis)) =
            finder.find("dirname", tokens, curr_index, Some(tokens.count() - 1))
        {
            curr_index = open_parenthesis;
            let mut file_candidate_right_index =
                tokens.prev_meaningful(close_parenthesis);
            let mut trailing_comma_index = None;
            if tokens[file_candidate_right_index].equals(",", true) {
                trailing_comma_index = Some(file_candidate_right_index);
                file_candidate_right_index = tokens.prev_meaningful(file_candidate_right_index);
            }
            if !tokens[file_candidate_right_index].is_kind(T_FILE) {
                continue;
            }
            let file_candidate_left_index =
                tokens.next_meaningful(open_parenthesis);
            if !tokens[file_candidate_left_index].is_kind(T_FILE) {
                continue;
            }
            let namespace_candidate_index = tokens.prev_meaningful(function_name_index);
            if tokens[namespace_candidate_index].is_kind(T_NS_SEPARATOR) {
                tokens.remove_trailing_whitespace(namespace_candidate_index, None);
                tokens.clear_at(namespace_candidate_index);
            }
            if let Some(trailing_comma_index) = trailing_comma_index {
                if !next_non_whitespace_is_comment(tokens, trailing_comma_index) {
                    tokens.remove_trailing_whitespace(trailing_comma_index, None);
                }
                tokens.clear_token_and_merge_surrounding_whitespace(trailing_comma_index);
            }
            if !next_non_whitespace_is_comment(tokens, close_parenthesis) {
                tokens.remove_leading_whitespace(close_parenthesis, None);
            }
            tokens.clear_token_and_merge_surrounding_whitespace(close_parenthesis);
            if !next_non_whitespace_is_comment(tokens, open_parenthesis) {
                tokens.remove_leading_whitespace(open_parenthesis, None);
            }
            tokens.remove_trailing_whitespace(open_parenthesis, None);
            tokens.clear_token_and_merge_surrounding_whitespace(open_parenthesis);
            tokens.set(file_candidate_left_index, Token::new(T_DIR, "__DIR__"));
            tokens.clear_token_and_merge_surrounding_whitespace(function_name_index);
        }
    }
}

fn next_non_whitespace_is_comment(tokens: &Tokens, index: usize) -> bool {
    tokens
        .get_next_non_whitespace(index, None)
        .is_some_and(|i| tokens[i].is_comment())
}
