use crate::analyzers::alternative_syntax_analyzer::AlternativeSyntaxAnalyzer;
use pun_format::Fixer;
use pun_tokens::{Kind, Kind::*, Token, Tokens};
use std::collections::BTreeMap;
use std::path::Path;

struct ControlStructureBracesFixer;

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(ControlStructureBracesFixer)
}

const CONTROL_TOKENS: &[Kind] = &[
    T_DECLARE, T_DO, T_ELSE, T_ELSEIF, T_FINALLY, T_FOR, T_FOREACH, T_IF, T_WHILE, T_TRY, T_CATCH,
    T_SWITCH,
];

fn control_continuation_tokens(k: Kind) -> &'static [Kind] {
    match k {
        T_IF => &[T_ELSE, T_ELSEIF],
        T_DO => &[T_WHILE],
        _ => &[T_CATCH, T_FINALLY],
    }
}

fn final_control_continuation_tokens(k: Kind) -> &'static [Kind] {
    match k {
        T_IF => &[T_ELSE],
        T_DO => &[],
        _ => &[T_FINALLY],
    }
}

impl ControlStructureBracesFixer {
    fn find_parenthesis_end(tokens: &Tokens, structure_token_index: usize) -> usize {
        if tokens[structure_token_index].is_given_kind(&[T_DO, T_ELSE, T_FINALLY, T_TRY]) {
            return structure_token_index;
        }
        let next_index = tokens
            .next_meaningful(structure_token_index);
        if !tokens[next_index].equals("(", true) {
            return structure_token_index;
        }
        tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, next_index)
    }

    fn find_statement_end(tokens: &Tokens, parenthesis_end_index: usize) -> usize {
        let next_index = tokens
            .next_meaningful(parenthesis_end_index);
        let next_token = &tokens[next_index];
        if next_token.equals("{", true) {
            return tokens.find_block_end(Tokens::BLOCK_TYPE_BRACE, next_index);
        }
        if next_token.is_given_kind(CONTROL_TOKENS) {
            let parenthesis_end_index = Self::find_parenthesis_end(tokens, next_index);
            let mut end_index = Self::find_statement_end(tokens, parenthesis_end_index);
            if next_token.is_given_kind(&[T_IF, T_TRY, T_DO]) {
                let opening_token_kind = next_token.get_id().unwrap();
                let is_continuation = |n: &usize| {
                    tokens[*n].is_given_kind(control_continuation_tokens(opening_token_kind))
                };
                while let Some(n) = tokens
                    .get_next_meaningful_token(end_index)
                    .filter(is_continuation)
                {
                    let parenthesis_end_index = Self::find_parenthesis_end(tokens, n);
                    end_index = Self::find_statement_end(tokens, parenthesis_end_index);
                    if tokens[n]
                        .is_given_kind(final_control_continuation_tokens(opening_token_kind))
                    {
                        return end_index;
                    }
                }
            }
            return end_index;
        }
        let mut index = parenthesis_end_index;
        loop {
            index += 1;
            let token = &tokens[index];
            if token.equals("{", true) {
                index = tokens.find_block_end(Tokens::BLOCK_TYPE_BRACE, index);
                continue;
            }
            if token.equals(";", true) {
                return index;
            }
            if token.is_kind(T_CLOSE_TAG) {
                return tokens.get_prev_non_whitespace(index, None).unwrap();
            }
        }
    }
}

impl Fixer for ControlStructureBracesFixer {
    fn name(&self) -> &'static str {
        "control_structure_braces"
    }
    fn priority(&self) -> i32 {
        1
    }
    fn is_candidate(&self, _tokens: &Tokens) -> bool {
        true
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        let alternative_syntax_analyzer = AlternativeSyntaxAnalyzer;
        for index in (0..tokens.count()).rev() {
            let token = &tokens[index];
            if !token.is_given_kind(CONTROL_TOKENS) {
                continue;
            }
            if token.is_kind(T_ELSE)
                && tokens[tokens.next_meaningful(index)].is_kind(T_IF)
            {
                continue;
            }
            let parenthesis_end_index = Self::find_parenthesis_end(tokens, index);
            let next_after_parenthesis_end_index = tokens
                .next_meaningful(parenthesis_end_index);
            let token_after_parenthesis = &tokens[next_after_parenthesis_end_index];
            if token_after_parenthesis.equals_any(
                &[";".into(), "{".into(), ":".into(), T_CLOSE_TAG.into()],
                true,
            ) {
                continue;
            }
            let mut statement_end_index = None;
            if token_after_parenthesis.is_given_kind(&[T_IF, T_FOR, T_FOREACH, T_SWITCH, T_WHILE]) {
                let token_after_parenthesis_block_end = tokens.find_block_end(
                    Tokens::BLOCK_TYPE_PARENTHESIS,
                    tokens
                        .next_meaningful(next_after_parenthesis_end_index),
                );
                if tokens[tokens
                    .next_meaningful(token_after_parenthesis_block_end)]
                .equals(":", true)
                {
                    let mut end = alternative_syntax_analyzer.find_alternative_syntax_block_end(
                        tokens,
                        next_after_parenthesis_end_index,
                    );
                    let token_after_statement_end_index =
                        tokens.next_meaningful(end);
                    if tokens[token_after_statement_end_index].equals(";", true) {
                        end = token_after_statement_end_index;
                    }
                    statement_end_index = Some(end);
                }
            }
            let statement_end_index = statement_end_index
                .unwrap_or_else(|| Self::find_statement_end(tokens, parenthesis_end_index));
            let mut tokens_to_insert_after_statement =
                vec![Token::new(T_WHITESPACE, " "), Token::char("}")];
            if !tokens[statement_end_index].equals_any(&[";".into(), "}".into()], true) {
                tokens_to_insert_after_statement.insert(0, Token::char(";"));
            }
            tokens.insert_slices(BTreeMap::from([(
                statement_end_index + 1,
                tokens_to_insert_after_statement,
            )]));
            tokens.insert_slices(BTreeMap::from([(
                parenthesis_end_index + 1,
                vec![Token::new(T_WHITESPACE, " "), Token::char("{")],
            )]));
        }
    }
}
