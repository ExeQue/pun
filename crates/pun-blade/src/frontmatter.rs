//! Front matter detection (prettier-plugin-blade's `front-matter.ts`).

#[derive(Debug, Clone)]
pub(crate) struct FrontMatter {
    pub(crate) language: String,
    pub(crate) explicit_language: Option<String>,
    pub(crate) value: String,
    pub(crate) start_delimiter: String,
    pub(crate) end_delimiter: String,
    pub(crate) raw: String,
    /// Byte offset just past the closing delimiter.
    pub(crate) end: usize,
}

fn find_line_break(text: &[u8], start: usize) -> Option<usize> {
    (start..text.len()).find(|&i| matches!(text[i], b'\n' | b'\r'))
}

fn skip_line_break(text: &[u8], index: usize) -> usize {
    if index >= text.len() {
        return index;
    }
    if text[index] == b'\r' && index + 1 < text.len() && text[index + 1] == b'\n' {
        return index + 2;
    }
    index + 1
}

fn get_front_matter(text: &str) -> Option<FrontMatter> {
    let bytes = text.as_bytes();
    let start_delimiter = text.get(..3)?;
    if start_delimiter != "---" && start_delimiter != "+++" {
        return None;
    }
    let first_break = find_line_break(bytes, 3)?;
    let content_start = skip_line_break(bytes, first_break);
    let explicit = crate::tree::js_trim(&text[3..first_break]).to_owned();
    let language = if explicit.is_empty() {
        if start_delimiter == "+++" {
            "toml".to_owned()
        } else {
            "yaml".to_owned()
        }
    } else {
        explicit.clone()
    };
    let allow_alt = start_delimiter == "---" && language == "yaml";
    let mut cursor = content_start;
    let mut found = None;
    while cursor <= text.len() {
        let line_break = find_line_break(bytes, cursor);
        let line_end = line_break.unwrap_or(text.len());
        let line = &text[cursor..line_end];
        if line == start_delimiter || (allow_alt && line == "...") {
            found = Some((cursor, line_end));
            break;
        }
        let Some(lb) = line_break else { break };
        cursor = skip_line_break(bytes, lb);
    }
    let (end_start, end_end) = found?;
    Some(FrontMatter {
        language,
        explicit_language: (!explicit.is_empty()).then_some(explicit),
        value: text[content_start..end_start].to_owned(),
        start_delimiter: start_delimiter.to_owned(),
        end_delimiter: text[end_start..end_end].to_owned(),
        raw: text[..end_end].to_owned(),
        end: end_end,
    })
}

/// Returns the front matter (if any) and the source with the front matter blanked to spaces,
/// keeping line breaks so every offset stays put.
pub(crate) fn parse_front_matter(text: &str) -> (Option<FrontMatter>, String) {
    let Some(fm) = get_front_matter(text) else {
        return (None, text.to_owned());
    };
    let mut content = String::with_capacity(text.len());
    for b in fm.raw.bytes() {
        content.push(if matches!(b, b'\r' | b'\n') {
            b as char
        } else {
            ' '
        });
    }
    content.push_str(&text[fm.raw.len()..]);
    (Some(fm), content)
}
