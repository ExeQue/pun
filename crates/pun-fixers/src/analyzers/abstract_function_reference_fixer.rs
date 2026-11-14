use super::functions_analyzer::FunctionsAnalyzer;
use pun_tokens::{Kind::*, Tokens};

pub(crate) fn is_candidate(tokens: &Tokens) -> bool {
    tokens.is_token_kind_found(T_STRING)
}

pub(crate) const IS_RISKY: bool = true;

#[derive(Default)]
pub(crate) struct FunctionReferenceFinder {
    functions_analyzer: FunctionsAnalyzer,
}

impl FunctionReferenceFinder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn find(
        &self,
        function_name_to_search: &str,
        tokens: &Tokens,
        mut start: usize,
        end: Option<usize>,
    ) -> Option<(usize, usize, usize)> {
        let end = end.unwrap_or(tokens.count());
        loop {
            let matches = tokens.find_sequence(
                &[(T_STRING, function_name_to_search).into(), "(".into()],
                start,
                Some(end),
                |_| false,
            )?;
            let (function_name, open_parenthesis) = (matches[0], matches[1]);
            if !self
                .functions_analyzer
                .is_global_function_call(tokens, function_name)
            {
                start = open_parenthesis;
                continue;
            }
            return Some((
                function_name,
                open_parenthesis,
                tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, open_parenthesis),
            ));
        }
    }
}
