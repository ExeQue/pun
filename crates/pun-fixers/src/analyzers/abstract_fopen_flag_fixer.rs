use super::abstract_function_reference_fixer::FunctionReferenceFinder;
use super::arguments_analyzer::ArgumentsAnalyzer;
use pun_tokens::{Kind::*, Tokens};

pub(crate) fn is_candidate(tokens: &Tokens) -> bool {
    tokens.is_all_token_kinds_found(&[T_STRING, T_CONSTANT_ENCAPSED_STRING])
}

pub(crate) fn apply_fix(
    tokens: &mut Tokens,
    fix_fopen_flag_token: impl Fn(&mut Tokens, usize, usize),
) {
    let finder = FunctionReferenceFinder::new();
    let mut index = 0;
    let end = tokens.count() - 1;
    while let Some((_, open, close)) = finder.find("fopen", tokens, index, Some(end)) {
        index = open;
        let arguments = ArgumentsAnalyzer.get_arguments(tokens, index, close);
        if arguments.len() < 2 || arguments.len() > 4 {
            continue;
        }
        let (&start, &arg_end) = arguments.iter().nth(1).unwrap();
        fix_fopen_flag_token(tokens, start, arg_end);
    }
}

pub(crate) fn flag_index(tokens: &Tokens, start: usize, end: usize) -> Option<usize> {
    let mut argument_flag_index = None;
    for i in start..=end {
        if tokens[i].is_given_kind(&[T_WHITESPACE, T_COMMENT, T_DOC_COMMENT]) {
            continue;
        }
        if argument_flag_index.is_some() {
            return None;
        }
        argument_flag_index = Some(i);
    }
    argument_flag_index.filter(|&i| tokens[i].is_kind(T_CONSTANT_ENCAPSED_STRING))
}

pub(crate) fn split_literal(content: &[u8]) -> (&[u8], u8, &[u8]) {
    if content[0] == b'b' || content[0] == b'B' {
        (&content[..1], content[1], &content[2..content.len() - 1])
    } else {
        (&[], content[0], &content[1..content.len() - 1])
    }
}

pub(crate) fn is_valid_mode_string(mode: &[u8]) -> bool {
    if mode.is_empty() || mode.len() > 13 {
        return false;
    }
    let mut valid_flags = b"abcertwx".to_vec();
    let Some(at) = valid_flags.iter().position(|&f| f == mode[0]) else {
        return false;
    };
    valid_flags.remove(at);
    for i in 1..mode.len() {
        if let Some(at) = valid_flags.iter().position(|&f| f == mode[i]) {
            valid_flags.remove(at);
            continue;
        }
        if mode[i] != b'+' || !b"acrwx".contains(&mode[i - 1]) {
            return false;
        }
    }
    true
}
