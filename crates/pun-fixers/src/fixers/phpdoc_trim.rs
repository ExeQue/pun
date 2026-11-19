use crate::analyzers::preg::Preg;
use pun_format::Fixer;
use pun_tokens::{Kind, Token, Tokens};
use std::path::Path;

struct PhpdocTrimFixer;

pub(crate) fn fixer() -> Box<dyn Fixer> {
    Box::new(PhpdocTrimFixer)
}

impl Fixer for PhpdocTrimFixer {
    fn name(&self) -> &'static str {
        "phpdoc_trim"
    }
    fn priority(&self) -> i32 {
        -5
    }
    fn is_candidate(&self, tokens: &Tokens) -> bool {
        tokens.is_token_kind_found(Kind::T_DOC_COMMENT)
    }
    fn fix(&self, _file: &Path, tokens: &mut Tokens) {
        for index in 0..tokens.count() {
            if !tokens[index].is_kind(Kind::T_DOC_COMMENT) {
                continue;
            }
            let content = fix_start(tokens[index].get_content());
            let content = fix_end(&content);
            tokens.set(index, Token::new(Kind::T_DOC_COMMENT, content));
        }
    }
}

fn fix_start(content: &[u8]) -> Vec<u8> {
    Preg::replace(
        r"~
                (^/\*\*)            # DocComment begin
                (?:
                    \R\h*(?:\*\h*)? # lines without useful content
                    (?!\R\h*\*/)    # not followed by a DocComment end
                )+
                (\R\h*(?:\*\h*)?\S) # first line with useful content
            ~x",
        "$1$2",
        content,
    )
}

fn fix_end(content: &[u8]) -> Vec<u8> {
    if content.is_ascii() && !blank_line_before_end(content) {
        return content.to_vec();
    }
    Preg::replace(
        r"~
                (\R\h*(?:\*\h*)?\S.*?) # last line with useful content
                (?:
                    (?<!/\*\*)         # not preceded by a DocComment start
                    \R\h*(?:\*\h*)?    # lines without useful content
                )+
                (\R\h*\*/$)            # DocComment end
            ~xu",
        "$1$2",
        content,
    )
}

/// Whether the line before the one that closes the ASCII doc block `content` (`\h*\*/`) holds no more than
/// `\h*(?:\*\h*)?`: `fix_end` only matches when one such line precedes the closing one.
fn blank_line_before_end(content: &[u8]) -> bool {
    let newline = |b: &u8| matches!(b, b'\n' | b'\r' | b'\x0b' | b'\x0c');
    let horizontal = |b: &u8| matches!(b, b' ' | b'\t');
    let rest = content.strip_suffix(b"*/").unwrap_or(content);
    let rest = &rest[..rest.len() - rest.iter().rev().take_while(|b| horizontal(b)).count()];
    let rest = match rest {
        [rest @ .., b'\r', b'\n'] => rest,
        [rest @ .., b] if newline(b) => rest,
        _ => return false,
    };
    let Some(start) = rest.iter().rposition(newline) else {
        return false;
    };
    let line = &rest[start + 1..];
    let line = &line[line.iter().take_while(|b| horizontal(b)).count()..];
    let line = line.strip_prefix(b"*").unwrap_or(line);
    line.iter().all(horizontal)
}
