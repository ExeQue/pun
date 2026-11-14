use crate::rules::support::docblocks::Doc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TagType<'s> {
    Name(&'s str),
    Union(Vec<&'s str>),
    Intersection(Vec<&'s str>),
    Other(&'s str),
}

#[derive(Debug, Clone)]
pub(crate) struct TypedTag<'s> {
    pub index: usize,
    pub name: &'s str,
    pub ty: TagType<'s>,
    pub variable: &'s str,
    pub description: &'s str,
}

impl TypedTag<'_> {
    pub(crate) fn print_with(&self, ty: &str) -> String {
        let rest = format!("{} {}", self.variable, self.description);
        let value = format!("{ty} {}", rest.trim());
        format!("{} {}", self.name, value.trim())
    }
}

pub(crate) fn typed_tag<'s>(
    doc: &Doc<'s>,
    name: &str,
    variable: Option<&str>,
) -> Result<Option<TypedTag<'s>>, ()> {
    for index in 0..doc.len() {
        let text = doc.text(index);
        let Some(rest) = text.strip_prefix(name) else {
            continue;
        };
        if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
            continue;
        }
        if text.contains('\n') {
            return Err(());
        }
        let tag = parse(index, &text[..name.len()], rest).ok_or(())?;
        if variable.is_none_or(|v| tag.variable == v) {
            return Ok(Some(tag));
        }
    }
    Ok(None)
}

fn parse<'s>(index: usize, name: &'s str, rest: &'s str) -> Option<TypedTag<'s>> {
    let value = rest.trim_start_matches([' ', '\t']);
    let mut members = Vec::new();
    let mut separator = None;
    let mut at = 0;
    loop {
        let len = member_len(&value[at..])?;
        members.push(&value[at..at + len]);
        at += len;
        let after = value[at..].trim_start_matches([' ', '\t']);
        match after.chars().next() {
            Some(c @ ('|' | '&')) if separator.is_none_or(|s| s == c) => {
                separator = Some(c);
                at = value.len() - after.len() + 1;
                at += value[at..].len() - value[at..].trim_start_matches([' ', '\t']).len();
            }
            Some('|' | '&') => return None,
            _ => break,
        }
    }
    let tail = &value[at..];
    if !tail.is_empty() && !tail.starts_with([' ', '\t']) {
        return None;
    }
    let tail = tail.trim_matches([' ', '\t']);
    let (variable, description) = match tail.strip_prefix('$') {
        Some(v) => {
            let len = v
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || !c.is_ascii()))
                .unwrap_or(v.len());
            (
                &tail[..=len],
                tail[len + 1..].trim_start_matches([' ', '\t']),
            )
        }
        None => ("", tail),
    };
    let ty = match (separator, members.as_slice()) {
        (None, [one]) if is_name(one) => TagType::Name(one),
        (None, [one]) => TagType::Other(one),
        (Some('|'), _) => TagType::Union(members),
        _ => TagType::Intersection(members),
    };
    Some(TypedTag {
        index,
        name,
        ty,
        variable,
        description,
    })
}

fn member_len(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'?'));
    if b.get(i) != Some(&b'(') {
        loop {
            if b.get(i) == Some(&b'\\') {
                i += 1;
            }
            let start = i;
            while i < b.len()
                && (b[i].is_ascii_alphanumeric()
                    || b[i] == b'_'
                    || b[i] >= 0x80
                    || (i > start && b[i] == b'-'))
            {
                i += 1;
            }
            if i == start || b[start].is_ascii_digit() {
                return None;
            }
            if b.get(i) != Some(&b'\\') {
                break;
            }
        }
    }
    let mut depth = 0usize;
    while let Some(&c) = b.get(i) {
        match c {
            b'<' | b'{' | b'(' | b'[' => depth += 1,
            b'>' | b'}' | b')' | b']' if depth > 0 => depth -= 1,
            b' ' | b'\t' | b',' | b':' | b'\'' | b'"' if depth > 0 => return None,
            _ if depth > 0 => {}
            _ => break,
        }
        i += 1;
    }
    (depth == 0 && !matches!(b.get(i), Some(b':'))).then_some(i)
}

pub(crate) fn is_name(member: &str) -> bool {
    member
        .bytes()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'\\') || c >= 0x80)
}

pub(crate) fn short_name(name: &str) -> &str {
    name.rsplit('\\').next().unwrap_or(name)
}

pub(crate) fn with_tag_type(doc: Option<&Doc<'_>>, name: &str, ty: &str) -> Option<String> {
    let added = format!("{name} {ty}");
    let Some(doc) = doc else {
        return Some(format!("/**\n * {added}\n */"));
    };
    match typed_tag(doc, name, None).ok()? {
        Some(tag) => {
            let text = doc.text_all();
            let members: Vec<&str> = match &tag.ty {
                TagType::Name(one) | TagType::Other(one) => vec![one],
                TagType::Union(m) | TagType::Intersection(m) => m.clone(),
            };
            let offset = |s: &str| s.as_ptr() as usize - text.as_ptr() as usize;
            let from = offset(members[0]);
            let last = members[members.len() - 1];
            let to = offset(last) + last.len();
            Some(format!("{}{}{}", &text[..from], ty, &text[to..]))
        }
        None => Some(doc.print_appending(&vec![false; doc.len()], &[&added])),
    }
}
