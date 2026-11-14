use crate::analyzers::fixer_configuration_resolver::resolve;
use crate::analyzers::helpers::skip_non_array_elements;
use anyhow::Result;
use pun_format::Fixer;
use pun_tokens::{Kind::*, Tokens};
use serde_json::Value;
use std::path::Path;

#[derive(Default)]
struct NoWhitespaceBeforeCommaInArrayFixer {
    after_heredoc: bool,
}

pub(crate) fn fixer() -> Box<dyn Fixer> {
    crate::configured(NoWhitespaceBeforeCommaInArrayFixer::default())
}

impl Fixer for NoWhitespaceBeforeCommaInArrayFixer {
    fn name(&self) -> &'static str {
        "no_whitespace_before_comma_in_array"
    }
    fn priority(&self) -> i32 {
        0
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_any_token_kinds_found(&[T_ARRAY, CT_ARRAY_BRACKET_OPEN])
    }
    fn configure(&mut self, options: &Value) -> Result<()> {
        let o = resolve(self.name(), options, &["after_heredoc"])?;
        self.after_heredoc = o.bool("after_heredoc", false)?;
        Ok(())
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        for index in (1..tokens.count()).rev() {
            if tokens[index].is_given_kind(&[T_ARRAY, CT_ARRAY_BRACKET_OPEN]) {
                self.fix_spacing(index, tokens);
            }
        }
    }
}

impl NoWhitespaceBeforeCommaInArrayFixer {
    fn fix_spacing(&self, index: usize, tokens: &mut Tokens) {
        let (start_index, end_index) = if tokens[index].is_kind(CT_ARRAY_BRACKET_OPEN) {
            (
                index,
                tokens.find_block_end(Tokens::BLOCK_TYPE_ARRAY_BRACKET, index),
            )
        } else {
            let s = tokens
                .get_next_token_of_kind(index, &["(".into()], true)
                .unwrap();
            (s, tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, s))
        };

        let mut i = end_index - 1;
        while i > start_index {
            i = skip_non_array_elements(i, tokens);
            let prev_index = tokens.get_prev_non_whitespace(i - 1, None).unwrap();
            if tokens[i].equals(",", true)
                && !tokens[prev_index].is_comment()
                && (self.after_heredoc || !tokens[prev_index].is_kind(T_END_HEREDOC))
            {
                tokens.remove_leading_whitespace(i, None);
            }
            i -= 1;
        }
    }
}
