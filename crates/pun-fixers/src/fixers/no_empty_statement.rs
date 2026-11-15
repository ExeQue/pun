use pun_format::Fixer;
use pun_tokens::{Kind::*, Tokens, TokensAnalyzer};
use std::path::Path;

struct NoEmptyStatementFixer;

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(NoEmptyStatementFixer)
}

impl NoEmptyStatementFixer {
    fn fix_semicolon_after_curly_brace_close(
        tokens: &mut Tokens,
        index: usize,
        curly_close_index: usize,
    ) {
        let curly_opening_index =
            tokens.find_block_start(Tokens::BLOCK_TYPE_BRACE, curly_close_index);
        let before_curly_opening_index = tokens.prev_meaningful(curly_opening_index);
        if tokens[before_curly_opening_index].is_given_kind(&[
            T_ELSE,
            T_FINALLY,
            T_NAMESPACE,
            T_OPEN_TAG,
        ]) || tokens[before_curly_opening_index]
            .equals_any(&[";".into(), "{".into(), "}".into()], true)
        {
            tokens.clear_token_and_merge_surrounding_whitespace(index);
            return;
        }
        if tokens[before_curly_opening_index].is_kind(T_STRING) {
            let mut classy_test_index = tokens.prev_meaningful(before_curly_opening_index);
            while tokens[classy_test_index].equals(",", true)
                || tokens[classy_test_index].is_given_kind(&[
                    T_STRING,
                    T_NS_SEPARATOR,
                    T_EXTENDS,
                    T_IMPLEMENTS,
                ])
            {
                classy_test_index = tokens.prev_meaningful(classy_test_index);
            }
            let clear = tokens[classy_test_index].is_kind(T_NAMESPACE)
                || (tokens[classy_test_index].is_classy()
                    && !TokensAnalyzer::new(tokens).is_anonymous_class(classy_test_index));
            if clear {
                tokens.clear_token_and_merge_surrounding_whitespace(index);
            }
            return;
        }
        if !tokens[before_curly_opening_index].equals(")", true) {
            return;
        }
        let opening_brace_index =
            tokens.find_block_start(Tokens::BLOCK_TYPE_PARENTHESIS, before_curly_opening_index);
        let before_opening_brace_index = tokens.prev_meaningful(opening_brace_index);
        if tokens[before_opening_brace_index].is_given_kind(&[
            T_IF, T_ELSEIF, T_FOR, T_FOREACH, T_WHILE, T_SWITCH, T_CATCH, T_DECLARE,
        ]) {
            tokens.clear_token_and_merge_surrounding_whitespace(index);
            return;
        }
        if tokens[before_opening_brace_index].is_kind(T_STRING) {
            let before_string_index = tokens.prev_meaningful(before_opening_brace_index);
            if tokens[before_string_index].is_kind(T_FUNCTION) {
                tokens.clear_token_and_merge_surrounding_whitespace(index);
            }
        }
    }
}

impl Fixer for NoEmptyStatementFixer {
    fn name(&self) -> &'static str {
        "no_empty_statement"
    }
    fn priority(&self) -> i32 {
        40
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_token_kind_found(";")
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        let count = tokens.count();
        let mut index = 0;
        while index < count {
            if tokens[index].is_given_kind(&[T_BREAK, T_CONTINUE]) {
                index = tokens.next_meaningful(index);
                if tokens[index].equals((T_LNUMBER, "1"), true) {
                    tokens.clear_token_and_merge_surrounding_whitespace(index);
                }
                index += 1;
                continue;
            }
            if tokens[index].is_kind(T_FOR) {
                index = tokens.find_block_end(
                    Tokens::BLOCK_TYPE_PARENTHESIS,
                    tokens.next_meaningful(index),
                ) + 1;
                index += 1;
                continue;
            }
            if !tokens[index].equals(";", true) {
                index += 1;
                continue;
            }
            let previous_meaningful_index = tokens.prev_meaningful(index);
            if tokens[previous_meaningful_index]
                .equals_any(&["{".into(), ";".into(), T_OPEN_TAG.into()], true)
            {
                tokens.clear_token_and_merge_surrounding_whitespace(index);
                index += 1;
                continue;
            }
            if tokens[previous_meaningful_index].equals("}", true) {
                Self::fix_semicolon_after_curly_brace_close(
                    tokens,
                    index,
                    previous_meaningful_index,
                );
                index += 1;
                continue;
            }
            if let Some(next_index) = tokens.get_next_meaningful_token(index)
                && tokens[next_index].is_kind(CT_PROPERTY_HOOK_BRACE_CLOSE)
            {
                index += 1;
                continue;
            }
            let pre_previous_meaningful_index = tokens.prev_meaningful(previous_meaningful_index);
            if tokens[pre_previous_meaningful_index].equals_any(
                &[";".into(), "{".into(), "}".into(), T_OPEN_TAG.into()],
                true,
            ) && tokens[previous_meaningful_index].is_given_kind(&[
                T_CONSTANT_ENCAPSED_STRING,
                T_DNUMBER,
                T_LNUMBER,
                T_STRING,
                T_VARIABLE,
            ]) {
                tokens.clear_token_and_merge_surrounding_whitespace(index);
                tokens.clear_token_and_merge_surrounding_whitespace(previous_meaningful_index);
            }
            index += 1;
        }
    }
}
