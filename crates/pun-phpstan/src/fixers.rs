use crate::comments::{T, token_spans};
use pun_tokens::Kind;
use pun_tokens::php_compat::{rtrim_chars, strpos, strrpos, substr_count, trim};

const IGNORE_TAG: &[u8] = b"@phpstan-ignore";

struct Token<'a> {
    kind: Option<Kind>,
    text: &'a [u8],
    line: i64,
    pos: usize,
}

fn tokens(code: &[u8]) -> Vec<Token<'_>> {
    let mut pos = 0;
    pun_tokens::tokenize(code)
        .into_iter()
        .map(|t| {
            let token = Token {
                kind: t.kind,
                text: t.text,
                line: i64::from(t.line),
                pos,
            };
            pos += t.text.len();
            token
        })
        .collect()
}

fn is_comment(t: &Token) -> bool {
    t.kind == Some(Kind::T_COMMENT) || t.kind == Some(Kind::T_DOC_COMMENT)
}

fn contains(text: &[u8], needle: &[u8]) -> bool {
    strpos(text, needle, 0).is_some()
}

fn newlines(s: &[u8]) -> i64 {
    substr_count(s, b"\n") as i64
}

fn preg_match(pattern: &str, subject: &[u8]) -> bool {
    crate::php::preg(pattern)
        .expect("fixer pattern")
        .is_match(subject)
        .unwrap_or(false)
}

#[derive(Debug, PartialEq)]
pub enum Fixed {
    Code(Vec<u8>),
    PhpDoc,
}

fn next_code<'t>(tokens: &'t [Token<'t>], i: usize) -> Option<&'t Token<'t>> {
    tokens[i + 1..]
        .iter()
        .find(|t| t.kind != Some(Kind::T_WHITESPACE) && !is_comment(t))
}

fn ignored_line(tokens: &[Token], i: usize, previous: Option<&Token>) -> i64 {
    let token = &tokens[i];
    if previous.is_some_and(|p| p.line == token.line) {
        return token.line;
    }
    let end_line = token.line + newlines(token.text);
    match next_code(tokens, i) {
        Some(next) if next.line == end_line + 1 => next.line,
        _ => end_line,
    }
}

pub fn unmatched_ignore(code: &[u8], line: i64, identifier: Option<&str>) -> Option<Fixed> {
    let tokens = tokens(code);
    let mut previous: Option<&Token> = None;
    for (i, token) in tokens.iter().enumerate() {
        if !is_comment(token) {
            if token.kind != Some(Kind::T_WHITESPACE) {
                previous = Some(token);
            }
            continue;
        }
        let text = token.text;
        if !contains(text, IGNORE_TAG) {
            continue;
        }
        let end_line = token.line + newlines(text);
        let is_next_line = contains(text, b"@phpstan-ignore-next-line");
        let is_doc = token.kind == Some(Kind::T_DOC_COMMENT);
        if is_next_line || contains(text, b"@phpstan-ignore-line") {
            if identifier.is_some() {
                continue;
            }
            if is_doc {
                match remove_phpdoc_line_tag(text, token.line, line) {
                    None => continue,
                    Some(true) => return Some(Fixed::PhpDoc),
                    Some(false) => return Some(Fixed::Code(replace_comment(code, token, None))),
                }
            }
            if end_line + i64::from(is_next_line) != line {
                continue;
            }
            if !preg_match(r"~^(?://|#|/\*)\s*@phpstan-ignore-(?:next-)?line\b~", text) {
                return None;
            }
            return Some(Fixed::Code(replace_comment(code, token, None)));
        }
        let Some(identifier) = identifier else {
            continue;
        };
        if ignored_line(&tokens, i, previous) != line {
            continue;
        }
        if is_doc {
            match remove_phpdoc_identifier(text, identifier) {
                None => continue,
                Some(true) => return Some(Fixed::PhpDoc),
                Some(false) => return Some(Fixed::Code(replace_comment(code, token, None))),
            }
        }
        return match remove_comment_identifier(text, identifier) {
            CommentEdit::Missing => continue,
            CommentEdit::Text(new_text) => {
                Some(Fixed::Code(replace_comment(code, token, Some(&new_text))))
            }
            CommentEdit::Remove => Some(Fixed::Code(replace_comment(code, token, None))),
            CommentEdit::OtherContents => None,
        };
    }
    None
}

enum CommentEdit {
    Text(Vec<u8>),
    Remove,
    Missing,
    OtherContents,
}

fn remove_comment_identifier(text: &[u8], identifier: &str) -> CommentEdit {
    let list_start = strpos(text, IGNORE_TAG, 0).unwrap_or(0) + IGNORE_TAG.len();
    let Some((new_list, list_end)) = remove_identifier_from_list(&text[list_start..], identifier)
    else {
        return CommentEdit::Missing;
    };
    if let Some(new_list) = new_list {
        return CommentEdit::Text([&text[..list_start], &new_list[..]].concat());
    }
    let only_ignore = preg_match(
        r"~^(?://|#|/\*)\s*$~",
        &text[..list_start - IGNORE_TAG.len()],
    ) && preg_match(
        r"~^\s*(?:\*/)?$~",
        &text[(list_start + list_end).min(text.len())..],
    );
    if only_ignore {
        CommentEdit::Remove
    } else {
        CommentEdit::OtherContents
    }
}

fn remove_identifier_from_list(list: &[u8], identifier: &str) -> Option<(Option<Vec<u8>>, usize)> {
    struct Item {
        name: Vec<u8>,
        start: usize,
        end: usize,
    }
    let mut offset = 0;
    let mut depth = 0;
    let mut items: Vec<Item> = Vec::new();
    let mut item: Option<Item> = None;
    for (span, t) in token_spans(list) {
        if depth == 0 && t == T::End {
            break;
        }
        let end = span.end;
        match t {
            T::Open => depth += 1,
            T::Close => {
                depth -= 1;
                if depth == 0
                    && let Some(item) = &mut item
                {
                    item.end = end;
                }
            }
            T::Identifier if depth == 0 => {
                items.extend(item.take());
                item = Some(Item {
                    name: list[span].to_vec(),
                    start: offset,
                    end,
                });
            }
            _ => {}
        }
        offset = end;
    }
    items.extend(item);
    let k = items.iter().position(|i| i.name == identifier.as_bytes())?;
    if items.len() == 1 {
        return Some((None, offset));
    }
    let mut new_list;
    if k < items.len() - 1 {
        new_list = list[..items[k].start].to_vec();
        new_list.extend(&list[items[k + 1].start..]);
    } else {
        new_list = list[..items[k - 1].end].to_vec();
        new_list.extend(&list[items[k].end..]);
    }
    Some((Some(new_list), offset))
}

fn remove_phpdoc_identifier(text: &[u8], identifier: &str) -> Option<bool> {
    let tags = crate::phpdoc::tags(text)?;
    let k = tags.iter().position(|t| t.name == "@phpstan-ignore")?;
    let crate::phpdoc::TagValue::Generic(value) = &tags[k].value else {
        return None;
    };
    let (new_list, _) = remove_identifier_from_list(value.as_bytes(), identifier)?;
    Some(new_list.is_some() || tags.len() > 1 || has_text(text))
}

fn has_text(text: &[u8]) -> bool {
    let inner = text
        .strip_prefix(b"/**")
        .and_then(|t| t.strip_suffix(b"*/"))
        .unwrap_or(text);
    let first_tag = strpos(inner, b"@", 0).unwrap_or(inner.len());
    inner[..first_tag]
        .iter()
        .any(|&c| !matches!(c, b' ' | b'\t' | b'\r' | b'\n' | b'*'))
}

fn remove_phpdoc_line_tag(text: &[u8], start_line: i64, line: i64) -> Option<bool> {
    let end_line = start_line + newlines(text);
    let tags = crate::phpdoc::tags(text)?;
    let mut offsets: Vec<(&str, usize)> = Vec::new();
    let mut candidates: Vec<(usize, i64)> = Vec::new();
    let last = tags.len().checked_sub(1)?;
    for (k, tag) in tags.iter().enumerate() {
        let name = tag.name.as_str();
        if name != "@phpstan-ignore-line" && name != "@phpstan-ignore-next-line" {
            continue;
        }
        let from = offsets
            .iter()
            .find(|(n, _)| *n == name)
            .map_or(0, |(_, o)| *o);
        let offset = strpos(text, name.as_bytes(), from)?;
        offsets.retain(|(n, _)| *n != name);
        offsets.push((name, offset + 1));
        let next = i64::from(name == "@phpstan-ignore-next-line");
        candidates.push((k, start_line + newlines(&text[..offset]) + next));
    }
    for (k, mut ignored) in candidates {
        if k == last && tags[k].name == "@phpstan-ignore-next-line" {
            ignored = end_line + 1;
        }
        if ignored == line {
            return Some(tags.len() > 1 || has_text(text));
        }
    }
    None
}

fn replace_comment(code: &[u8], token: &Token, new_text: Option<&[u8]>) -> Vec<u8> {
    let mut start = token.pos;
    let mut end = start + token.text.len();
    if let Some(new_text) = new_text {
        return [&code[..start], new_text, &code[end..]].concat();
    }
    let line_start = strrpos(&code[..start], b"\n").map_or(0, |p| p + 1);
    let line_end = strpos(code, b"\n", end);
    let before = &code[line_start..start];
    let after = &code[end..line_end.unwrap_or(code.len())];
    if trim(before).is_empty() && trim(after).is_empty() {
        let rest = line_end.map_or(&[][..], |e| &code[e + 1..]);
        return [&code[..line_start], rest].concat();
    }
    if !trim(before).is_empty() {
        start = line_start + rtrim_chars(before, b" \t").len();
    } else {
        end += after
            .iter()
            .take_while(|&&c| c == b' ' || c == b'\t')
            .count();
    }
    [&code[..start], &code[end..]].concat()
}

pub fn line_ignore(code: &[u8], line: i64) -> bool {
    let tokens = tokens(code);
    let mut previous: Option<&Token> = None;
    for (i, token) in tokens.iter().enumerate() {
        if !is_comment(token) {
            if token.kind != Some(Kind::T_WHITESPACE) {
                previous = Some(token);
            }
            continue;
        }
        let text = token.text;
        let is_next_line = contains(text, b"@phpstan-ignore-next-line");
        if !is_next_line && !contains(text, b"@phpstan-ignore-line") {
            continue;
        }
        let tag: &[u8] = if is_next_line {
            b"@phpstan-ignore-next-line"
        } else {
            b"@phpstan-ignore-line"
        };
        let tag_pos = strpos(text, tag, 0).unwrap_or(0);
        let end_line = token.line + newlines(text);
        let closing = if text.ends_with(b"*/") { 2 } else { 0 };
        let rest_start = (tag_pos + tag.len()).min(text.len());
        let rest = &text[rest_start..(text.len() - closing).max(rest_start)];
        let ignored = if token.kind == Some(Kind::T_DOC_COMMENT) {
            let at = token.line + newlines(&text[..tag_pos]);
            match (is_next_line, has_phpdoc_tag(rest)) {
                (false, _) => at,
                (true, true) => at + 1,
                (true, false) => end_line + 1,
            }
        } else {
            end_line + i64::from(is_next_line)
        };
        if ignored != line {
            continue;
        }
        if substr_count(text, IGNORE_TAG) != 1 || has_phpdoc_tag(rest) {
            return false;
        }
        if !balanced(rest) {
            return false;
        }
        let after_code = previous.is_some_and(|p| p.line == token.line);
        if ignored_line(&tokens, i, previous) == line {
            return true;
        }
        if !is_next_line || !after_code {
            return false;
        }
        return match unmatched_ignore(code, line, None) {
            None => false,
            Some(Fixed::Code(new_code)) => {
                append_to_line(&new_code, line - newlines(code) + newlines(&new_code))
            }
            Some(Fixed::PhpDoc) => append_to_line(code, line),
        };
    }
    false
}

fn append_to_line(code: &[u8], line: i64) -> bool {
    let tokens = tokens(code);
    let mut last_code: Option<&Token> = None;
    for (i, token) in tokens.iter().enumerate() {
        let comment = is_comment(token);
        if !comment && token.kind != Some(Kind::T_WHITESPACE) {
            last_code = Some(token);
        }
        if token.line != line {
            continue;
        }
        if comment && contains(token.text, IGNORE_TAG) {
            return false;
        }
        if token.kind != Some(Kind::T_WHITESPACE) || !contains(token.text, b"\n") {
            continue;
        }
        return last_code.is_some_and(|c| c.line == line) && i > 0 && tokens[i - 1].line == line;
    }
    false
}

fn has_phpdoc_tag(text: &[u8]) -> bool {
    preg_match(r"~@[a-z_\\]~i", text)
}

fn balanced(text: &[u8]) -> bool {
    let mut depth = 0;
    for &c in text {
        if c == b'(' {
            depth += 1;
        } else if c == b')' {
            depth -= 1;
            if depth < 0 {
                return false;
            }
        }
    }
    depth == 0
}
