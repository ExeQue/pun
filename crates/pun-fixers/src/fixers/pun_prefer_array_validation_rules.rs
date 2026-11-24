use crate::analyzers::analysis::NamespaceUseType;
use crate::analyzers::arguments_analyzer::ArgumentsAnalyzer;
use crate::analyzers::fully_qualified_name_analyzer::FullyQualifiedNameAnalyzer;
use pun_format::Fixer;
use pun_tokens::php_compat::strtolower;
use pun_tokens::Kind::*;
use pun_tokens::tokens_analyzer::ClassyElementType;
use pun_tokens::{Token, Tokens, TokensAnalyzer};
use std::collections::BTreeMap;
use std::path::Path;

struct PreferArrayValidationRules;

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(PreferArrayValidationRules)
}

impl Fixer for PreferArrayValidationRules {
    fn name(&self) -> &'static str {
        "Pun/prefer_array_validation_rules"
    }
    fn priority(&self) -> i32 {
        0
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_all_token_kinds_found(&[T_DOUBLE_ARROW, T_CONSTANT_ENCAPSED_STRING])
            && tokens.is_any_token_kinds_found(&[T_FUNCTION, T_DOUBLE_COLON, T_OBJECT_OPERATOR])
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        let mut found = BTreeMap::new();
        let methods: Vec<usize> = TokensAnalyzer::new(tokens)
            .get_classy_elements()
            .into_iter()
            .filter(|(i, e)| {
                e.kind == ClassyElementType::Method
                    && tokens
                        .get_next_meaningful_token(*i)
                        .is_some_and(|n| tokens[n].get_content().eq_ignore_ascii_case(b"rules"))
            })
            .map(|(i, _)| i)
            .collect();
        for method in methods {
            let open = match tokens.get_next_token_of_kind(method, &["{".into(), ";".into()], true) {
                Some(open) if tokens[open].equals("{", true) => open,
                _ => continue,
            };
            let end = tokens.find_block_end(Tokens::BLOCK_TYPE_BRACE, open);
            let mut i = open + 1;
            while i < end {
                if tokens[i].is_kind(T_FUNCTION) {
                    if let Some(o) = tokens.get_next_token_of_kind(i, &["{".into()], true) {
                        i = tokens.find_block_end(Tokens::BLOCK_TYPE_BRACE, o);
                    }
                } else if tokens[i].is_kind(T_FN) {
                    i = TokensAnalyzer::new(tokens).get_last_token_index_of_arrow_function(i);
                } else if let Some(r) = replacement(tokens, i) {
                    found.insert(i, r);
                }
                i += 1;
            }
        }
        let names = FullyQualifiedNameAnalyzer::new(tokens);
        for i in 0..tokens.len() {
            if let Some(array) = rules_argument(tokens, &names, i) {
                found.extend(top_level_replacements(tokens, array));
            }
        }
        for (i, r) in found.into_iter().rev() {
            tokens.override_range(i, i, r);
        }
    }
}

/// The array literal of validation rules in the call whose `(` is at `open`, at the call sites
/// rector-laravel's `ValidationRuleArrayStringValueToArrayRector` matches: the second of exactly two
/// arguments of `Validator::make()` (the `Illuminate\Support\Facades\Validator` facade) or of
/// `$factory->make()` (an `Illuminate\Validation\Factory`), and the first argument of
/// `$request->validate()` (an `Illuminate\Http\Request`). A variable's class is its parameter type in
/// the enclosing function; `request()` returns the request.
fn rules_argument(
    tokens: &Tokens,
    names: &FullyQualifiedNameAnalyzer<'_>,
    open: usize,
) -> Option<usize> {
    if !tokens[open].equals("(", true) {
        return None;
    }
    let method = tokens.get_prev_meaningful_token(open)?;
    if !tokens[method].is_kind(T_STRING) {
        return None;
    }
    let operator = tokens.prev_meaningful(method);
    let make = tokens[method].get_content().eq_ignore_ascii_case(b"make");
    let (receiver, expected): (_, &[u8]) = if tokens[operator].is_kind(T_DOUBLE_COLON) && make {
        let class = class_name(tokens, names, tokens.prev_meaningful(operator));
        (class, b"illuminate\\support\\facades\\validator")
    } else if tokens[operator].is_kind(T_OBJECT_OPERATOR) {
        let object = tokens.prev_meaningful(operator);
        let class = if is_request_helper(tokens, object) {
            Some(b"illuminate\\http\\request".to_vec())
        } else {
            parameter_class(tokens, names, object)
        };
        if make {
            (class, b"illuminate\\validation\\factory")
        } else if tokens[method].get_content().eq_ignore_ascii_case(b"validate") {
            (class, b"illuminate\\http\\request")
        } else {
            return None;
        }
    } else {
        return None;
    };
    if receiver.as_deref() != Some(expected) {
        return None;
    }
    let close = tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, open);
    let arguments = ArgumentsAnalyzer.get_arguments(tokens, open, close);
    if make && arguments.len() != 2 {
        return None;
    }
    let (&start, &end) = arguments.iter().nth(usize::from(make))?;
    let mut first = tokens.next_meaningful(start - 1);
    if tokens[first].is_kind(CT_NAMED_ARGUMENT_NAME) {
        first = tokens.next_meaningful(tokens.next_meaningful(first));
    }
    let (array, last) = if tokens[first].is_kind(CT_ARRAY_BRACKET_OPEN) {
        (first, tokens.find_block_end(Tokens::BLOCK_TYPE_ARRAY_BRACKET, first))
    } else if tokens[first].is_kind(T_ARRAY) {
        let paren = tokens.next_meaningful(first);
        (paren, tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, paren))
    } else {
        return None;
    };
    (tokens.prev_meaningful(end + 1) == last).then_some(array)
}

/// The lowercased fully qualified name of the class named by the tokens ending at `end`.
fn class_name(
    tokens: &Tokens,
    names: &FullyQualifiedNameAnalyzer<'_>,
    end: usize,
) -> Option<Vec<u8>> {
    let mut start = end;
    while start > 0 && tokens[start - 1].is_given_kind(&[T_STRING, T_NS_SEPARATOR]) {
        start -= 1;
    }
    if !tokens[end].is_kind(T_STRING) || tokens[start - 1].is_kind(T_NAMESPACE) {
        return None;
    }
    let name: Vec<u8> = (start..=end).flat_map(|i| tokens[i].get_content().to_vec()).collect();
    Some(strtolower(&names.get_fully_qualified_name(&name, end, NamespaceUseType::Class)))
}

/// Whether `index` closes a `request()` call to the global helper, with no arguments.
fn is_request_helper(tokens: &Tokens, index: usize) -> bool {
    if !tokens[index].equals(")", true) {
        return false;
    }
    let open = tokens.prev_meaningful(index);
    if !tokens[open].equals("(", true) {
        return false;
    }
    let Some(name) = tokens.get_prev_meaningful_token(open) else {
        return false;
    };
    if !tokens[name].equals((T_STRING, "request"), false) {
        return false;
    }
    let mut before = tokens.prev_meaningful(name);
    if tokens[before].is_kind(T_NS_SEPARATOR) {
        before = tokens.prev_meaningful(before);
        if tokens[before].is_kind(T_STRING) {
            return false;
        }
    }
    !tokens[before].is_given_kind(&[T_OBJECT_OPERATOR, T_NULLSAFE_OBJECT_OPERATOR, T_DOUBLE_COLON, T_FUNCTION, T_NEW])
}

/// The lowercased class a variable at `index` is declared with as a parameter of the innermost
/// function around it (or of an arrow function, which also sees its parent's variables).
fn parameter_class(
    tokens: &Tokens,
    names: &FullyQualifiedNameAnalyzer<'_>,
    index: usize,
) -> Option<Vec<u8>> {
    if !tokens[index].is_kind(T_VARIABLE) || tokens[index].equals((T_VARIABLE, "$this"), false) {
        return None;
    }
    let before = tokens.prev_meaningful(index);
    if tokens[before].is_given_kind(&[T_OBJECT_OPERATOR, T_NULLSAFE_OBJECT_OPERATOR, T_DOUBLE_COLON]) || tokens[before].equals("$", true) {
        return None;
    }
    let mut function = index;
    while let Some(f) = tokens.get_prev_token_of_kind(function, &[T_FUNCTION.into(), T_FN.into()], true) {
        function = f;
        let Some(open) = tokens.get_next_token_of_kind(f, &["(".into()], true) else {
            continue;
        };
        let close = tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, open);
        let body_end = if tokens[f].is_kind(T_FN) {
            TokensAnalyzer::new(tokens).get_last_token_index_of_arrow_function(f)
        } else {
            match tokens.get_next_token_of_kind(close, &["{".into(), ";".into()], true) {
                Some(b) if tokens[b].equals("{", true) => tokens.find_block_end(Tokens::BLOCK_TYPE_BRACE, b),
                _ => continue,
            }
        };
        if index < close || index > body_end {
            continue;
        }
        let parameter = (open + 1..close).find(|&p| {
            tokens[p].is_kind(T_VARIABLE) && tokens[p].get_content() == tokens[index].get_content()
        });
        let Some(parameter) = parameter else {
            if tokens[f].is_kind(T_FN) {
                continue;
            }
            return None;
        };
        let mut end = tokens.prev_meaningful(parameter);
        while tokens[end].is_given_kind(&[T_ELLIPSIS, CT_TYPE_INTERSECTION]) || tokens[end].equals("&", true) {
            end = tokens.prev_meaningful(end);
        }
        let mut start = end;
        while tokens[start - 1].is_given_kind(&[T_STRING, T_NS_SEPARATOR]) {
            start -= 1;
        }
        let before = tokens.prev_meaningful(start);
        if tokens[before].is_given_kind(&[CT_TYPE_ALTERNATION, CT_TYPE_INTERSECTION]) {
            return None;
        }
        return class_name(tokens, names, end);
    }
    None
}

/// The rules of the top-level items of the array literal opened at `open`.
fn top_level_replacements(tokens: &Tokens, open: usize) -> Vec<(usize, Vec<Token>)> {
    let close = if tokens[open].is_kind(CT_ARRAY_BRACKET_OPEN) {
        tokens.find_block_end(Tokens::BLOCK_TYPE_ARRAY_BRACKET, open)
    } else {
        tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, open)
    };
    let mut out = Vec::new();
    let mut i = open + 1;
    while i < close {
        if let Some((block, true)) = Tokens::detect_block_type(&tokens[i]) {
            i = tokens.find_block_end(block, i);
        } else if let Some(r) = replacement(tokens, i) {
            out.push((i, r));
        }
        i += 1;
    }
    out
}

fn replacement(tokens: &Tokens, i: usize) -> Option<Vec<Token>> {
    if !tokens[i].is_kind(T_CONSTANT_ENCAPSED_STRING) {
        return None;
    }
    let arrow = tokens
        .get_prev_meaningful_token(i)
        .filter(|&a| tokens[a].is_kind(T_DOUBLE_ARROW))?;
    tokens
        .get_prev_meaningful_token(arrow)
        .filter(|&k| tokens[k].is_kind(T_CONSTANT_ENCAPSED_STRING))?;
    tokens.get_next_meaningful_token(i).filter(|&n| {
        tokens[n].equals_any(
            &[",".into(), ")".into(), CT_ARRAY_BRACKET_CLOSE.into()],
            true,
        )
    })?;
    let raw = tokens[i].get_content();
    let quote = raw[0];
    if !(quote == b'\'' || quote == b'"') {
        return None;
    }
    let content = &raw[1..raw.len() - 1];
    if !content.contains(&b'|') || content.windows(6).any(|w| w == b"regex:") {
        return None;
    }
    let segments: Vec<&[u8]> = content.split(|&b| b == b'|').collect();
    if segments
        .iter()
        .any(|s| s.is_empty() || s.iter().any(|b| matches!(b, b'\'' | b'\\' | b'$')))
    {
        return None;
    }
    let mut out = vec![Token::new(CT_ARRAY_BRACKET_OPEN, "[")];
    for (n, s) in segments.iter().enumerate() {
        if n > 0 {
            out.push(Token::char(","));
            out.push(Token::new(T_WHITESPACE, " "));
        }
        out.push(Token::new(
            T_CONSTANT_ENCAPSED_STRING,
            [b"'", *s, b"'"].concat(),
        ));
    }
    out.push(Token::new(CT_ARRAY_BRACKET_CLOSE, "]"));
    Some(out)
}

#[cfg(test)]
mod tests {
    fn fix(code: &str) -> String {
        crate::fix_with(&*super::fixer(), code)
    }

    fn class(name: &str, method: &str, items: &str) -> String {
        format!(
            "<?php\n\nclass {name}\n{{\n    public function {method}(): array\n    {{\n        return [\n{items}        ];\n    }}\n}}\n"
        )
    }

    #[test]
    fn mago_cases() {
        assert_eq!(
            fix(&class(
                "StoreUserRequest",
                "rules",
                "            'name' => 'required|string|max:255',\n"
            )),
            class(
                "StoreUserRequest",
                "rules",
                "            'name' => ['required', 'string', 'max:255'],\n"
            )
        );
        assert_eq!(
            fix(&class(
                "StoreUserRequest",
                "rules",
                "            'name' => 'required|string|max:255',\n            'email' => 'required|email',\n"
            )),
            class(
                "StoreUserRequest",
                "rules",
                "            'name' => ['required', 'string', 'max:255'],\n            'email' => ['required', 'email'],\n"
            )
        );
        for same in [
            class(
                "StoreUserRequest",
                "rules",
                "            'name' => ['required', 'string'],\n",
            ),
            class(
                "StoreUserRequest",
                "rules",
                "            'name' => 'required',\n",
            ),
            class(
                "StoreUserRequest",
                "rules",
                "            'slug' => 'required|regex:/^[a-z]+|[0-9]+$/',\n",
            ),
            class("Report", "build", "            'label' => 'left|right',\n"),
        ] {
            assert_eq!(fix(&same), same);
        }
    }

    #[test]
    fn edge_cases() {
        assert_eq!(
            fix(&class(
                "R",
                "RULES",
                "            \"a\" => \"nullable|int\"\n"
            )),
            class("R", "RULES", "            \"a\" => ['nullable', 'int']\n")
        );
        for same in [
            class("R", "rules", "            'a' => 'x||y',\n"),
            class("R", "rules", "            'a' => 'x|' . $y,\n"),
            class("R", "rules", "            'a' => \"x|$y\",\n"),
            class("R", "rules", "            'a' => fn () => ['b' => 'x|y'],\n"),
            class("R", "rules", "            'a' => function () { return ['b' => 'x|y']; },\n"),
            "<?php function rules() { return ['a' => 'x|y']; }".into(),
            "<?php abstract class R { abstract public function rules(); public function x() { return ['a' => 'x|y']; } }".into(),
            class("R", "rules", "            'a' => b'x|y',\n"),
        ] {
            assert_eq!(fix(&same), same);
        }
    }

    /// The call sites of rector-laravel 2.7.3's `ValidationRuleArrayStringValueToArrayRector`, with
    /// laravel/framework 12 autoloaded: `Validator::make()` and `Factory::make()` with two arguments,
    /// and `Request::validate()`. The expected output is what Rector wrote for this input.
    #[test]
    fn rector_call_sites() {
        let input = r"<?php

namespace App;

use Illuminate\Http\Request;
use Illuminate\Support\Facades\Validator;
use Illuminate\Validation\Factory;

final class Example
{
    public function run(array $input, ?Request $nullable, Factory $factory, Request|int $union, $untyped): mixed
    {
        Validator::make($input, ['a' => 'required|string']);
        \Illuminate\Support\Facades\Validator::make($input, ['b' => 'required|string']);
        \Validator::make($input, ['c' => 'required|string']);
        Validator::make($input, ['d' => 'required|string'], ['messages']);
        Validator::make($input, rules: ['e' => 'required|string']);
        $factory->make($input, ['f' => 'required|string']);
        validator($input, ['g' => 'required|string']);
        $nullable->validate(['h' => 'required|string'], ['m' => 'x|y']);
        $union->validate(['i' => 'required|string']);
        $untyped->validate(['j' => 'required|string']);
        request()->validate(['k' => 'required|string']);
        Validator::make($input, ['l' => ['p' => 'q|r']]);
        $fn = function (Request $r) {
            $r->validate(['n' => 'a|b']);
        };

        return Validator::make($input, ['o' => 'required|string'])->validate();
    }
}
";
        let expected = r"<?php

namespace App;

use Illuminate\Http\Request;
use Illuminate\Support\Facades\Validator;
use Illuminate\Validation\Factory;

final class Example
{
    public function run(array $input, ?Request $nullable, Factory $factory, Request|int $union, $untyped): mixed
    {
        Validator::make($input, ['a' => ['required', 'string']]);
        \Illuminate\Support\Facades\Validator::make($input, ['b' => ['required', 'string']]);
        \Validator::make($input, ['c' => 'required|string']);
        Validator::make($input, ['d' => 'required|string'], ['messages']);
        Validator::make($input, rules: ['e' => ['required', 'string']]);
        $factory->make($input, ['f' => ['required', 'string']]);
        validator($input, ['g' => 'required|string']);
        $nullable->validate(['h' => ['required', 'string']], ['m' => 'x|y']);
        $union->validate(['i' => 'required|string']);
        $untyped->validate(['j' => 'required|string']);
        request()->validate(['k' => ['required', 'string']]);
        Validator::make($input, ['l' => ['p' => 'q|r']]);
        $fn = function (Request $r) {
            $r->validate(['n' => ['a', 'b']]);
        };

        return Validator::make($input, ['o' => ['required', 'string']])->validate();
    }
}
";
        assert_eq!(fix(input), expected);
    }
}
