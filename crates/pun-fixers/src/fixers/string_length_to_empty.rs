use crate::analyzers::abstract_function_reference_fixer::{
    FunctionReferenceFinder, IS_RISKY, is_candidate,
};
use crate::analyzers::arguments_analyzer::ArgumentsAnalyzer;
use pun_format::Fixer;
use pun_tokens::Kind::{self, *};
use pun_tokens::{Proto, Token, Tokens};
use serde_json::Value;
use std::path::Path;

#[derive(Default)]
struct StringLengthToEmptyFixer {
    /// Pun's `identical_or_greater_only`: only `strlen($x) === 0` and `strlen($x) > 0` (either side), the forms
    /// Rector's `StrlenZeroToIdenticalEmptyStringRector` changes.
    identical_or_greater_only: bool,
}

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(StringLengthToEmptyFixer::default())
}

/// Whether Rector changes this comparison: `0` with `===`, or `>` towards `0` (`strlen($x) > 0`, `0 < strlen($x)`).
fn is_rector_form(operator: &Token, operand: &Token, yoda: bool) -> bool {
    operand.get_content() == b"0"
        && (operator.is_kind(T_IS_IDENTICAL) || operator.equals(if yoda { "<" } else { ">" }, true))
}

fn get_replacement_yoda(operator: &Token, operand: &Token) -> Option<Kind> {
    if operand.get_content() == b"0" {
        if operator.is_given_kind(&[T_IS_IDENTICAL, T_IS_GREATER_OR_EQUAL]) {
            return Some(T_IS_IDENTICAL);
        }
        if operator.is_kind(T_IS_NOT_IDENTICAL) || operator.equals("<", true) {
            return Some(T_IS_NOT_IDENTICAL);
        }
        return None;
    }
    if operator.is_kind(T_IS_SMALLER_OR_EQUAL) {
        return Some(T_IS_NOT_IDENTICAL);
    }
    if operator.equals(">", true) {
        return Some(T_IS_IDENTICAL);
    }
    None
}

fn get_replacement_not_yoda(operator: &Token, operand: &Token) -> Option<Kind> {
    if operand.get_content() == b"0" {
        if operator.is_given_kind(&[T_IS_IDENTICAL, T_IS_SMALLER_OR_EQUAL]) {
            return Some(T_IS_IDENTICAL);
        }
        if operator.is_kind(T_IS_NOT_IDENTICAL) || operator.equals(">", true) {
            return Some(T_IS_NOT_IDENTICAL);
        }
        return None;
    }
    if operator.is_kind(T_IS_GREATER_OR_EQUAL) {
        return Some(T_IS_NOT_IDENTICAL);
    }
    if operator.equals("<", true) {
        return Some(T_IS_IDENTICAL);
    }
    None
}

fn is_operand_of_interest(token: &Token) -> bool {
    token.is_kind(T_LNUMBER) && matches!(token.get_content(), b"0" | b"1")
}

fn is_operator_of_interest(token: &Token) -> bool {
    token.is_given_kind(&[
        T_IS_IDENTICAL,
        T_IS_NOT_IDENTICAL,
        T_IS_SMALLER_OR_EQUAL,
        T_IS_GREATER_OR_EQUAL,
    ]) || token.equals("<", true)
        || token.equals(">", true)
}

fn is_of_higher_precedence(token: &Token) -> bool {
    const CHARS: &[Proto<'static>] = &[
        Proto::Char(b"!"),
        Proto::Char(b"%"),
        Proto::Char(b"*"),
        Proto::Char(b"+"),
        Proto::Char(b"-"),
        Proto::Char(b"."),
        Proto::Char(b"/"),
        Proto::Char(b"~"),
        Proto::Char(b"?"),
    ];
    token.is_given_kind(&[T_INSTANCEOF, T_POW, T_SL, T_SR]) || token.equals_any(CHARS, true)
}

fn keep_parentheses(
    tokens: &Tokens,
    open_parenthesis_index: usize,
    close_parenthesis_index: usize,
) -> bool {
    let mut i = tokens.next_meaningful(open_parenthesis_index);
    if tokens[i].is_cast() {
        i = tokens.next_meaningful(i);
    }
    while i < close_parenthesis_index {
        let token = &tokens[i];
        if token.is_given_kind(&[T_VARIABLE, T_STRING])
            || token.is_object_operator()
            || token.is_whitespace(None)
            || token.is_comment()
        {
            i += 1;
            continue;
        }
        if let Some((block_type, true)) = Tokens::detect_block_type(token) {
            i = tokens.find_block_end(block_type, i) + 1;
            continue;
        }
        return true;
    }
    false
}

fn find_str_length_calls(tokens: &Tokens) -> Vec<(usize, usize, usize)> {
    let finder = FunctionReferenceFinder::new();
    let mut candidates = Vec::new();
    let count = tokens.count();
    let mut i = 0;
    while i < count {
        let Some(candidate) = finder.find("strlen", tokens, i, Some(count)) else {
            break;
        };
        i = candidate.1 + 1;
        candidates.push(candidate);
    }
    candidates.reverse();
    candidates
}

impl Fixer for StringLengthToEmptyFixer {
    fn name(&self) -> &'static str {
        "string_length_to_empty"
    }
    fn priority(&self) -> i32 {
        1
    }
    fn is_risky(&self) -> bool {
        IS_RISKY
    }
    fn configure(&mut self, options: &Value) -> anyhow::Result<()> {
        let only = match options {
            Value::Null => None,
            Value::Object(o) if o.len() == 1 => o.get("identical_or_greater_only"),
            _ => anyhow::bail!("[{}] Is not configurable.", self.name()),
        };
        self.identical_or_greater_only = match only {
            None if !options.is_null() => anyhow::bail!("[{}] Is not configurable.", self.name()),
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(v) => anyhow::bail!(
                "[{}] identical_or_greater_only must be a bool, not {v}",
                self.name()
            ),
        };
        Ok(())
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        is_candidate(tokens)
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        let arguments_analyzer = ArgumentsAnalyzer;
        for (function_name_index, open_parenthesis_index, close_parenthesis_index) in
            find_str_length_calls(tokens)
        {
            if arguments_analyzer.count_arguments(
                tokens,
                open_parenthesis_index,
                close_parenthesis_index,
            ) != 1
            {
                continue;
            }
            let next_index = tokens.next_meaningful(close_parenthesis_index);
            let mut previous_index = tokens.prev_meaningful(function_name_index);
            let mut namespace_separator_index = None;
            if tokens[previous_index].is_kind(T_NS_SEPARATOR) {
                namespace_separator_index = Some(previous_index);
                previous_index = tokens.prev_meaningful(previous_index);
            }
            let (operator_index, operand_index, replacement);
            if is_operator_of_interest(&tokens[previous_index]) {
                operator_index = previous_index;
                operand_index = tokens.prev_meaningful(previous_index);
                if !is_operand_of_interest(&tokens[operand_index])
                    || (self.identical_or_greater_only
                        && !is_rector_form(&tokens[operator_index], &tokens[operand_index], true))
                {
                    continue;
                }
                let Some(r) = get_replacement_yoda(&tokens[operator_index], &tokens[operand_index])
                else {
                    continue;
                };
                replacement = r;
                if is_of_higher_precedence(&tokens[next_index]) {
                    continue;
                }
                let left = tokens.prev_meaningful(operand_index);
                if is_of_higher_precedence(&tokens[left]) {
                    continue;
                }
            } else if is_operator_of_interest(&tokens[next_index]) {
                operator_index = next_index;
                operand_index = tokens.next_meaningful(next_index);
                if !is_operand_of_interest(&tokens[operand_index])
                    || (self.identical_or_greater_only
                        && !is_rector_form(&tokens[operator_index], &tokens[operand_index], false))
                {
                    continue;
                }
                let Some(r) =
                    get_replacement_not_yoda(&tokens[operator_index], &tokens[operand_index])
                else {
                    continue;
                };
                replacement = r;
                let right = tokens.next_meaningful(operand_index);
                if is_of_higher_precedence(&tokens[right]) {
                    continue;
                }
                if is_of_higher_precedence(&tokens[previous_index]) {
                    continue;
                }
            } else {
                continue;
            }
            let keep = keep_parentheses(tokens, open_parenthesis_index, close_parenthesis_index);
            let operand_content = if replacement == T_IS_IDENTICAL {
                "==="
            } else {
                "!=="
            };
            tokens.set(operand_index, Token::new(T_CONSTANT_ENCAPSED_STRING, "''"));
            tokens.set(operator_index, Token::new(replacement, operand_content));
            if !keep {
                tokens.clear_token_and_merge_surrounding_whitespace(close_parenthesis_index);
                tokens.clear_token_and_merge_surrounding_whitespace(open_parenthesis_index);
            }
            tokens.clear_token_and_merge_surrounding_whitespace(function_name_index);
            if let Some(ns) = namespace_separator_index {
                tokens.clear_token_and_merge_surrounding_whitespace(ns);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    fn fix(options: &serde_json::Value, code: &str) -> String {
        let mut fixer = super::fixer();
        fixer.configure(options).unwrap();
        crate::fix_with(&*fixer, code)
    }

    /// Rector 2.8.4's `StrlenZeroToIdenticalEmptyStringRector` changes `$a` to `$d` only; PHP-CS-Fixer's
    /// `string_length_to_empty` changes all twelve (its output is the expectation for the four).
    #[test]
    fn identical_or_greater_only_changes_what_rector_changes() {
        let forms = [
            ("strlen($s) === 0", "$s === ''"),
            ("0 === strlen($s)", "'' === $s"),
            ("strlen($s) > 0", "$s !== ''"),
            ("0 < strlen($s)", "'' !== $s"),
        ];
        let left = [
            "strlen($s) !== 0",
            "0 !== strlen($s)",
            "strlen($s) <= 0",
            "0 >= strlen($s)",
            "strlen($s) < 1",
            "strlen($s) >= 1",
            "1 > strlen($s)",
            "1 <= strlen($s)",
        ];
        let code = |v: &[&str]| -> String {
            let lines: String = v.iter().map(|e| format!("$x = {e};\n")).collect();
            format!("<?php\n{lines}")
        };
        let before: Vec<&str> = forms.iter().map(|f| f.0).chain(left).collect();
        let after: Vec<&str> = forms.iter().map(|f| f.1).chain(left).collect();
        let options = serde_json::json!({ "identical_or_greater_only": true });
        assert_eq!(fix(&options, &code(&before)), code(&after));
        let mut fixer = super::fixer();
        for (options, error) in [
            (serde_json::json!({ "foo": true }), "[string_length_to_empty] Is not configurable."),
            (
                serde_json::json!({ "identical_or_greater_only": 1 }),
                "[string_length_to_empty] identical_or_greater_only must be a bool, not 1",
            ),
        ] {
            assert_eq!(fixer.configure(&options).unwrap_err().to_string(), error);
        }
    }
}
