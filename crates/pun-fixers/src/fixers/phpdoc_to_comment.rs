use crate::analyzers::comments_analyzer::{
    is_before_return, is_before_structural_element, is_header_comment,
};
use crate::analyzers::fixer_configuration_resolver::resolve;
use crate::analyzers::preg::Preg;
use anyhow::Result;
use pun_format::Fixer;
use pun_tokens::Kind::*;
use pun_tokens::php_compat::strtolower;
use pun_tokens::{Token, Tokens};
use serde_json::Value;
use std::path::Path;

struct PhpdocToCommentFixer {
    ignored_tags: Vec<Vec<u8>>,
    allow_before_return_statement: bool,
}

pub(crate) fn fixer() -> Box<dyn Fixer> {
    crate::configured(PhpdocToCommentFixer {
        ignored_tags: Vec::new(),
        allow_before_return_statement: false,
    })
}

impl Fixer for PhpdocToCommentFixer {
    fn name(&self) -> &'static str {
        "phpdoc_to_comment"
    }
    fn priority(&self) -> i32 {
        25
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_token_kind_found(T_DOC_COMMENT)
    }
    fn configure(&mut self, options: &Value) -> Result<()> {
        let o = resolve(
            self.name(),
            options,
            &["ignored_tags", "allow_before_return_statement"],
        )?;
        self.ignored_tags = o
            .strings("ignored_tags", &[], &[])?
            .iter()
            .map(|t| strtolower(t.as_bytes()))
            .collect();
        self.allow_before_return_statement = o.bool("allow_before_return_statement", false)?;
        Ok(())
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        'tokens: for index in 0..tokens.count() {
            if !tokens[index].is_kind(T_DOC_COMMENT) {
                continue;
            }
            if is_header_comment(tokens, index) {
                continue;
            }
            if self.allow_before_return_statement && is_before_return(tokens, index) {
                continue;
            }
            if is_before_structural_element(tokens, index) {
                continue;
            }
            if is_var_for_next_statement(tokens, index) {
                continue;
            }
            for m in Preg::match_all(r"~\@([a-zA-Z0-9_\\\-]+)\b~", tokens[index].get_content()) {
                if self.ignored_tags.contains(&strtolower(m.str(1))) {
                    continue 'tokens;
                }
            }
            let content = tokens[index].get_content();
            let trimmed = &content[content
                .iter()
                .take_while(|&&c| c == b'/' || c == b'*')
                .count()..];
            tokens.set(index, Token::new(T_COMMENT, [&b"/*"[..], trimmed].concat()));
        }
    }
}

/// Not in PHP-CS-Fixer: whether the docblock has a `@var` tag naming a variable that the next statement uses, such
/// as `/** @var list<int> $ids */` before `return $ids;`. PHPStan reads that tag before any statement, so turning
/// it into a comment would drop the type. Statements that start with a variable or a destructuring are left to
/// upstream's assignment check.
fn is_var_for_next_statement(tokens: &Tokens, index: usize) -> bool {
    let next = tokens.get_next_meaningful_token(index);
    if next.is_none_or(|n| {
        tokens[n].is_given_kind(&[T_VARIABLE, T_LIST, CT_DESTRUCTURING_BRACKET_OPEN])
    }) {
        return false;
    }
    let names: Vec<&[u8]> = Preg::match_all(
        r"~@(?:phpstan-|psalm-)?var\s[^\r\n]*?(\$[a-zA-Z_\x80-\xff][a-zA-Z0-9_\x80-\xff]*)~",
        tokens[index].get_content(),
    )
    .iter()
    .map(|m| m.str(1))
    .collect();
    if names.is_empty() {
        return false;
    }
    let mut depth = 0usize;
    for i in index + 1..tokens.count() {
        let token = &tokens[i];
        if token.equals("(", true) || token.equals("[", true) {
            depth += 1;
        } else if token.equals(")", true) || token.equals("]", true) {
            if depth == 0 {
                return false;
            }
            depth -= 1;
        } else if depth == 0
            && (token.equals(";", true)
                || token.equals("{", true)
                || token.equals("}", true)
                || token.is_kind(T_CLOSE_TAG))
        {
            return false;
        } else if token.is_kind(T_VARIABLE) && names.contains(&token.get_content()) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    fn fix(code: &str) -> String {
        crate::fix_with(&*super::fixer(), code)
    }

    // PHPStan 2.3.5 reads `@var array{metrics: list<string>} $context` before the `return` and the `echo`
    // (`dumpType()` gives the annotated type) and ignores it once it is a `/* */` comment.
    #[test]
    fn keeps_var_tags_the_next_statement_reads() {
        let code = "<?php\n$first = true;\n\nfunction f(array $context): string\n{\n    /** @var array{metrics: list<string>} $context */\n    return $context['metrics'][0];\n}\n\n/** @phpstan-var int $x */\necho strlen((string) $x);\n";
        assert_eq!(fix(code), code);
    }

    // As PHP-CS-Fixer does: no named variable, or one the statement does not use.
    #[test]
    fn converts_var_tags_the_next_statement_does_not_read() {
        assert_eq!(
            fix("<?php\n$first = true;\n\n/** @var void */\nreturn;\n\n/** @var int $other */\nreturn $x; /** @var int $x */\n\n/** @var int $c */\n[$a] = $c;\n"),
            "<?php\n$first = true;\n\n/* @var void */\nreturn;\n\n/* @var int $other */\nreturn $x; /* @var int $x */\n\n/* @var int $c */\n[$a] = $c;\n"
        );
    }
}
