use crate::analyzers::arguments_analyzer::ArgumentsAnalyzer;
use pun_format::Fixer;
use pun_tokens::Kind::*;
use pun_tokens::{Token, Tokens};
use std::path::Path;

struct NoUnsetOnPropertyFixer;

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(NoUnsetOnPropertyFixer)
}

struct UnsetInfo {
    start_index: usize,
    end_index: usize,
    is_to_transform: bool,
    is_first: bool,
}

fn get_unsets_info(tokens: &Tokens, index: usize) -> Vec<UnsetInfo> {
    let unset_start = tokens
        .get_next_token_of_kind(index, &["(".into()], true)
        .unwrap();
    let unset_end = tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, unset_start);
    let mut is_first = true;
    let mut unsets = Vec::new();
    for (start_index, end_index) in ArgumentsAnalyzer.get_arguments(tokens, unset_start, unset_end)
    {
        let start_index = tokens.next_meaningful(start_index - 1);
        let end_index = tokens.prev_meaningful(end_index + 1);
        unsets.push(UnsetInfo {
            start_index,
            end_index,
            is_to_transform: is_property(tokens, start_index, end_index),
            is_first,
        });
        is_first = false;
    }
    unsets
}

fn is_property(tokens: &Tokens, index: usize, end_index: usize) -> bool {
    if tokens[index].is_kind(T_VARIABLE) {
        if is_plain_object(tokens, tokens[index].get_content()) {
            return false;
        }
        let next_index = tokens.next_meaningful(index);
        if !tokens[next_index].is_kind(T_OBJECT_OPERATOR) {
            return false;
        }
        let next_index = tokens.next_meaningful(next_index);
        if tokens
            .get_next_meaningful_token(next_index)
            .is_some_and(|n| n < end_index)
        {
            return false;
        }
        return tokens[next_index].is_kind(T_STRING);
    }
    if tokens[index].is_given_kind(&[T_NS_SEPARATOR, T_STRING]) {
        let next_index = tokens
            .get_token_not_of_kinds_sibling(index, 1, &[T_DOUBLE_COLON, T_NS_SEPARATOR, T_STRING])
            .unwrap();
        if tokens
            .get_next_meaningful_token(next_index)
            .is_some_and(|n| n < end_index)
        {
            return false;
        }
        return tokens[next_index].is_kind(T_VARIABLE);
    }
    false
}

/// Not in PHP-CS-Fixer: whether the file makes `variable` a plain object, whose properties are its data, so that
/// `unset()` removes a key (from `json_encode()`, `foreach`, `get_object_vars()`) where `= null` keeps it: assigned
/// an `(object)` cast, `new stdClass` or `json_decode()`, or typed `stdClass` or `object`.
fn is_plain_object(tokens: &Tokens, variable: &[u8]) -> bool {
    // `name`, not `Some\name`.
    let is_global = |i: usize, name: &[u8]| {
        tokens[i].is_kind(T_STRING)
            && tokens[i].get_content().eq_ignore_ascii_case(name)
            && tokens.get_prev_meaningful_token(i).is_none_or(|p| {
                !tokens[p].is_kind(T_NS_SEPARATOR)
                    || tokens
                        .get_prev_meaningful_token(p)
                        .is_none_or(|q| !tokens[q].is_kind(T_STRING))
            })
    };
    (0..tokens.count()).any(|i| {
        if !tokens[i].is_kind(T_VARIABLE) || tokens[i].get_content() != variable {
            return false;
        }
        if tokens
            .get_prev_meaningful_token(i)
            .is_some_and(|p| is_global(p, b"stdClass") || is_global(p, b"object"))
        {
            return true;
        }
        let Some(value) = tokens
            .get_next_meaningful_token(i)
            .filter(|&a| tokens[a].equals("=", true))
            .and_then(|a| tokens.get_next_meaningful_token(a))
        else {
            return false;
        };
        let value = match tokens[value].is_kind(T_NS_SEPARATOR) {
            true => tokens.next_meaningful(value),
            false => value,
        };
        match tokens[value].get_id() {
            Some(T_OBJECT_CAST) => true,
            Some(T_NEW) => {
                let mut class = tokens.next_meaningful(value);
                if tokens[class].is_kind(T_NS_SEPARATOR) {
                    class = tokens.next_meaningful(class);
                }
                is_global(class, b"stdClass")
            }
            _ => is_global(value, b"json_decode"),
        }
    })
}

fn update_tokens(tokens: &mut Tokens, unset_info: &UnsetInfo, is_last_unset: bool) {
    if unset_info.is_first && unset_info.is_to_transform {
        let brace_index = tokens
            .get_prev_token_of_kind(unset_info.start_index, &["(".into()], true)
            .unwrap();
        let unset_index = tokens
            .get_prev_token_of_kind(brace_index, &[T_UNSET.into()], true)
            .unwrap();
        tokens.clear_token_and_merge_surrounding_whitespace(brace_index);
        tokens.clear_token_and_merge_surrounding_whitespace(unset_index);
    }
    if is_last_unset && unset_info.is_to_transform {
        let brace_index = tokens
            .get_next_token_of_kind(unset_info.end_index, &[")".into()], true)
            .unwrap();
        let previous_index = tokens.prev_meaningful(brace_index);
        if tokens[previous_index].equals(",", true) {
            tokens.clear_token_and_merge_surrounding_whitespace(previous_index);
        }
        tokens.clear_token_and_merge_surrounding_whitespace(brace_index);
    }
    if !is_last_unset {
        let comma_index = tokens
            .get_next_token_of_kind(unset_info.end_index, &[",".into()], true)
            .unwrap();
        tokens.set(comma_index, Token::char(";"));
    }
    if !unset_info.is_to_transform && !is_last_unset {
        tokens.insert_at(unset_info.end_index + 1, vec![Token::char(")")]);
    }
    if !unset_info.is_to_transform && !unset_info.is_first {
        tokens.insert_at(
            unset_info.start_index,
            vec![Token::new(T_UNSET, "unset"), Token::char("(")],
        );
    }
    if unset_info.is_to_transform {
        tokens.insert_at(
            unset_info.end_index + 1,
            vec![
                Token::new(T_WHITESPACE, " "),
                Token::char("="),
                Token::new(T_WHITESPACE, " "),
                Token::new(T_STRING, "null"),
            ],
        );
    }
}

impl Fixer for NoUnsetOnPropertyFixer {
    fn name(&self) -> &'static str {
        "no_unset_on_property"
    }
    fn priority(&self) -> i32 {
        25
    }
    fn is_risky(&self) -> bool {
        true
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_token_kind_found(T_UNSET)
            && tokens.is_any_token_kinds_found(&[T_OBJECT_OPERATOR, T_DOUBLE_COLON])
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        for index in (0..tokens.count()).rev() {
            if !tokens[index].is_kind(T_UNSET) {
                continue;
            }
            let unsets_info = get_unsets_info(tokens, index);
            if !unsets_info.iter().any(|u| u.is_to_transform) {
                continue;
            }
            let mut is_last_unset = true;
            for unset_info in unsets_info.iter().rev() {
                update_tokens(tokens, unset_info, is_last_unset);
                is_last_unset = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    fn fix(code: &str) -> String {
        crate::fix_with(&*super::fixer(), code)
    }

    #[test]
    fn keeps_plain_object_removals() {
        // PHP-CS-Fixer 3.95.25 writes `$wire->remove_with_phrase = null;` here, which leaves the field in
        // `json_encode($wire)`. Pun keeps the `unset()` of a plain object's property and still rewrites the rest.
        assert_eq!(
            fix(
                "<?php\n$wire = (object) ['remove_with_phrase' => null];\nunset($wire->remove_with_phrase, $this->b);\n"
            ),
            "<?php\n$wire = (object) ['remove_with_phrase' => null];\nunset($wire->remove_with_phrase); $this->b = null;\n"
        );
        for same in [
            "<?php\n$o = new stdClass;\nunset($o->a);\n",
            "<?php\n$o = new \\stdClass();\nunset($o->a);\n",
            "<?php\n$o = json_decode($json);\nunset($o->a);\n",
            "<?php\n$o = \\json_decode($json, false);\nunset($o->a);\n",
            "<?php\nfunction f(\\stdClass $o) { unset($o->a); }\n",
            "<?php\nfunction f(?object $o) { unset($o->a); }\n",
        ] {
            assert_eq!(fix(same), same);
        }
        // PHP-CS-Fixer's output: another class named `stdClass`, or a parameter of a named class.
        assert_eq!(
            fix("<?php\n$o = new Foo\\stdClass;\nunset($o->a, $this->b);\n"),
            "<?php\n$o = new Foo\\stdClass;\n$o->a = null; $this->b = null;\n"
        );
        assert_eq!(
            fix("<?php\nfunction f(Foo $wire) { unset($wire->a); }\n"),
            "<?php\nfunction f(Foo $wire) { $wire->a = null; }\n"
        );
    }
}
