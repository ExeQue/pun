use crate::analyzers::fixer_configuration_resolver::resolve;
use anyhow::Result;
use pun_format::Fixer;
use pun_tokens::{Kind::*, Tokens, TokensAnalyzer};
use serde_json::Value;
use std::path::Path;

#[derive(Default)]
struct UnaryOperatorSpacesFixer {
    only_dec_inc: bool,
}

pub(crate) fn fixer() -> Box<dyn Fixer> {
    crate::configured(UnaryOperatorSpacesFixer::default())
}

impl Fixer for UnaryOperatorSpacesFixer {
    fn name(&self) -> &'static str {
        "unary_operator_spaces"
    }
    fn priority(&self) -> i32 {
        0
    }
    fn is_candidate(&self, _tokens: &Tokens) -> bool {
        true
    }
    fn configure(&mut self, options: &Value) -> Result<()> {
        let o = resolve(self.name(), options, &["only_dec_inc"])?;
        self.only_dec_inc = o.bool("only_dec_inc", false)?;
        Ok(())
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        for index in (0..tokens.count()).rev() {
            if self.only_dec_inc && !tokens[index].is_given_kind(&[T_DEC, T_INC]) {
                continue;
            }
            // The tokens either check below can accept: their first test is one of these.
            let may_be_unary = match tokens[index].get_id() {
                None => matches!(tokens[index].get_content(), b"!" | b"~" | b"@" | b"+" | b"-" | b"&"),
                Some(k) => matches!(
                    k,
                    T_INC
                        | T_DEC
                        | T_ELLIPSIS
                        | CT_RETURN_REF
                        | T_AMPERSAND_FOLLOWED_BY_VAR_OR_VARARG
                        | T_AMPERSAND_NOT_FOLLOWED_BY_VAR_OR_VARARG
                ),
            };
            if !may_be_unary {
                continue;
            }
            if TokensAnalyzer::new(tokens).is_unary_successor_operator(index) {
                let prev = tokens.get_prev_non_whitespace(index, None).unwrap();
                if !tokens[prev].is_comment() {
                    tokens.remove_leading_whitespace(index, None);
                }
                continue;
            }
            if TokensAnalyzer::new(tokens).is_unary_predecessor_operator(index) {
                tokens.remove_trailing_whitespace(index, None);
            }
        }
    }
}
