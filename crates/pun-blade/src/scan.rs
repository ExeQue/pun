//! Scanning primitives shared by the lexer and the tree builder (prettier-plugin-blade's
//! `lexer/scan-primitives.ts`). Offsets are byte offsets; every delimiter they look for is ASCII.

pub(crate) fn is_alpha(ch: u8) -> bool {
    ch.is_ascii_alphabetic()
}

pub(crate) fn is_digit(ch: u8) -> bool {
    ch.is_ascii_digit()
}

pub(crate) fn is_alnum(ch: u8) -> bool {
    ch.is_ascii_alphanumeric()
}

pub(crate) fn is_space(ch: u8) -> bool {
    matches!(ch, b' ' | b'\t' | b'\n' | b'\r')
}

pub(crate) fn can_start_directive_at(src: &[u8], pos: usize, boundary_start: usize) -> bool {
    if pos <= boundary_start {
        return pos == boundary_start;
    }
    let prev = src[pos - 1];
    !is_alnum(prev) && prev != b'@'
}

pub(crate) fn find(src: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > src.len() {
        return None;
    }
    if needle.len() == 1 {
        return src[from..]
            .iter()
            .position(|&b| b == needle[0])
            .map(|p| p + from);
    }
    src[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

pub(crate) fn find_line_ending(src: &[u8], pos: usize, len: usize) -> Option<usize> {
    (pos..len).find(|&i| src[i] == b'\n' || src[i] == b'\r')
}

pub(crate) fn skip_line_ending(src: &[u8], pos: &mut usize, len: usize) {
    if *pos >= len {
        return;
    }
    match src[*pos] {
        b'\n' => *pos += 1,
        b'\r' => {
            *pos += 1;
            if *pos < len && src[*pos] == b'\n' {
                *pos += 1;
            }
        }
        _ => {}
    }
}

pub(crate) fn skip_quoted_string(src: &[u8], pos: &mut usize, len: usize, quote: u8) {
    while *pos < len {
        let Some(quote_pos) = find(&src[..len], &[quote], *pos) else {
            *pos = len;
            return;
        };
        *pos = quote_pos;
        let mut backslashes = 0;
        let mut check = quote_pos;
        while check > 0 && src[check - 1] == b'\\' {
            backslashes += 1;
            check -= 1;
        }
        *pos += 1;
        if backslashes % 2 == 0 {
            return;
        }
    }
    *pos = len;
}

pub(crate) fn skip_block_comment(src: &[u8], pos: &mut usize, len: usize) {
    while *pos < len {
        let Some(star) = find(&src[..len], b"*", *pos) else {
            *pos = len;
            return;
        };
        *pos = star + 1;
        if *pos < len && src[*pos] == b'/' {
            *pos += 1;
            return;
        }
    }
    *pos = len;
}

pub(crate) fn skip_line_comment(src: &[u8], pos: &mut usize, len: usize) {
    match find_line_ending(src, *pos, len) {
        None => *pos = len,
        Some(end) => {
            *pos = end;
            skip_line_ending(src, pos, len);
        }
    }
}

pub(crate) fn skip_heredoc(src: &[u8], pos: &mut usize, len: usize) {
    if *pos >= len {
        return;
    }
    let is_nowdoc = src[*pos] == b'\'';
    if is_nowdoc {
        *pos += 1;
    }
    let delim_start = *pos;
    while *pos < len && (src[*pos].is_ascii_alphanumeric() || src[*pos] == b'_') {
        *pos += 1;
    }
    if *pos == delim_start {
        *pos = len;
        return;
    }
    let delimiter = &src[delim_start..*pos];
    if is_nowdoc && *pos < len && src[*pos] == b'\'' {
        *pos += 1;
    }
    let Some(line_end) = find_line_ending(src, *pos, len) else {
        *pos = len;
        return;
    };
    *pos = line_end;
    skip_line_ending(src, pos, len);
    let delim_len = delimiter.len();
    while *pos < len {
        if *pos + delim_len <= len && &src[*pos..*pos + delim_len] == delimiter {
            let after = *pos + delim_len;
            if after >= len || matches!(src[after], b'\n' | b'\r' | b';') {
                *pos = after;
                if *pos < len && src[*pos] == b';' {
                    *pos += 1;
                }
                skip_line_ending(src, pos, len);
                return;
            }
        }
        let Some(next) = find_line_ending(src, *pos, len) else {
            *pos = len;
            return;
        };
        *pos = next;
        skip_line_ending(src, pos, len);
    }
    *pos = len;
}

pub(crate) fn skip_template_literal(src: &[u8], pos: &mut usize, len: usize) {
    while *pos < len {
        match src[*pos] {
            b'`' => {
                *pos += 1;
                return;
            }
            b'\\' => *pos += 2,
            b'$' if *pos + 1 < len && src[*pos + 1] == b'{' => {
                *pos += 2;
                skip_template_expression(src, pos, len);
            }
            _ => *pos += 1,
        }
    }
}

pub(crate) fn skip_template_expression(src: &[u8], pos: &mut usize, len: usize) {
    let mut depth = 1;
    while depth > 0 && *pos < len {
        let byte = src[*pos];
        match byte {
            b'{' => {
                depth += 1;
                *pos += 1;
            }
            b'}' => {
                depth -= 1;
                *pos += 1;
            }
            b'\'' | b'"' => {
                *pos += 1;
                skip_quoted_string(src, pos, len, byte);
            }
            b'`' => {
                *pos += 1;
                skip_template_literal(src, pos, len);
            }
            b'/' if *pos + 1 < len => match src[*pos + 1] {
                b'/' => {
                    *pos += 2;
                    skip_line_comment(src, pos, len);
                }
                b'*' => {
                    *pos += 2;
                    skip_block_comment(src, pos, len);
                }
                _ => *pos += 1,
            },
            _ => *pos += 1,
        }
    }
}

pub(crate) fn skip_backtick_string(src: &[u8], pos: &mut usize, len: usize) {
    skip_quoted_string(src, pos, len, b'`');
}
