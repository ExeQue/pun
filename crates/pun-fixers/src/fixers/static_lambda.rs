use pun_format::Fixer;
use pun_tokens::php_compat::strtolower;
use pun_tokens::{Kind::*, Token, Tokens, TokensAnalyzer};
use std::path::Path;

struct StaticLambdaFixer;

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(StaticLambdaFixer)
}

impl Fixer for StaticLambdaFixer {
    fn name(&self) -> &'static str {
        "static_lambda"
    }
    fn priority(&self) -> i32 {
        0
    }
    fn is_risky(&self) -> bool {
        true
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_any_token_kinds_found(&[T_FUNCTION, T_FN])
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        let mut index = tokens.count() as isize - 4;
        while index > 0 {
            let i = index as usize;
            index -= 1;
            let analyzer = TokensAnalyzer::new(tokens);
            if !tokens[i].is_given_kind(&[T_FUNCTION, T_FN]) || !analyzer.is_lambda(i) {
                continue;
            }
            let prev = tokens.get_prev_meaningful_token(i).unwrap();
            if tokens[prev].is_kind(T_STATIC) {
                continue;
            }
            let arguments_start_index = tokens
                .get_next_token_of_kind(i, &["(".into()], true)
                .unwrap();
            let arguments_end_index =
                tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, arguments_start_index);
            let (lambda_open_index, lambda_end_index) = if tokens[i].is_kind(T_FUNCTION) {
                let open = tokens
                    .get_next_token_of_kind(arguments_end_index, &["{".into()], true)
                    .unwrap();
                (open, tokens.find_block_end(Tokens::BLOCK_TYPE_BRACE, open))
            } else {
                let open = tokens
                    .get_next_token_of_kind(arguments_end_index, &[T_DOUBLE_ARROW.into()], true)
                    .unwrap();
                (open, analyzer.get_last_token_index_of_arrow_function(i))
            };
            if has_possible_reference_to_this(tokens, lambda_open_index, lambda_end_index) {
                continue;
            }
            if is_direct_subject_of_binding(tokens, i, lambda_end_index) {
                continue;
            }
            tokens.insert_at(
                i,
                vec![
                    Token::new(T_STATIC, "static"),
                    Token::new(T_WHITESPACE, " "),
                ],
            );
            index -= 4;
        }
    }
}

fn is_direct_subject_of_binding(tokens: &Tokens, lambda_index: usize, lambda_end_index: usize) -> bool {
    let Some(after_lambda_index) = tokens.get_next_meaningful_token(lambda_end_index) else {
        return false;
    };
    if !tokens[after_lambda_index].equals(")", true) {
        return false;
    }
    let wrap_start_index = tokens.find_block_start(Tokens::BLOCK_TYPE_PARENTHESIS, after_lambda_index);
    if tokens.get_next_meaningful_token(wrap_start_index) != Some(lambda_index) {
        return false;
    }
    if let Some(before_wrap_index) = tokens.get_prev_meaningful_token(wrap_start_index)
        && tokens[before_wrap_index].equals_any(
            &[")".into(), "]".into(), T_STRING.into(), T_VARIABLE.into(), CT_ARRAY_INDEX_BRACE_CLOSE.into()],
            true,
        )
    {
        return false;
    }
    let object_operator_index = tokens.get_next_meaningful_token(after_lambda_index).unwrap();
    if !tokens[object_operator_index].is_object_operator() {
        return false;
    }
    let method_index = tokens.get_next_meaningful_token(object_operator_index).unwrap();
    tokens[method_index].equals_any(&[(T_STRING, "bindTo").into(), (T_STRING, "call").into()], false)
}

fn has_possible_reference_to_this(tokens: &Tokens, start_index: usize, end_index: usize) -> bool {
    let mut i = start_index;
    while i <= end_index {
        let token = &tokens[i];
        if token.is_kind(T_VARIABLE) && strtolower(token.get_content()) == b"$this" {
            return true;
        }
        if token.is_given_kind(&[
            T_INCLUDE,
            T_INCLUDE_ONCE,
            T_REQUIRE,
            T_REQUIRE_ONCE,
            CT_DYNAMIC_VAR_BRACE_OPEN,
            T_EVAL,
        ]) {
            return true;
        }
        if token.is_classy() {
            let open_brace_index = tokens
                .get_next_token_of_kind(i, &["{".into()], true)
                .unwrap();
            i = tokens.get_next_meaningful_token(i).unwrap();
            if i <= open_brace_index && has_possible_reference_to_this(tokens, i, open_brace_index)
            {
                return true;
            }
            i = tokens.find_block_end(Tokens::BLOCK_TYPE_BRACE, open_brace_index) + 1;
            continue;
        }
        if token.equals("$", true) {
            let next_index = tokens.get_next_meaningful_token(i).unwrap();
            if tokens[next_index].is_kind(T_VARIABLE) {
                return true;
            }
        }
        if token.equals((T_STRING, "parent"), false) {
            return true;
        }
        i += 1;
    }
    false
}
