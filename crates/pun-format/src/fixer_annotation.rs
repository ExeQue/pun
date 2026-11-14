use pcre2::bytes::{Regex, RegexBuilder};
use pun_tokens::Kind::*;
use pun_tokens::Tokens;
use std::sync::LazyLock;

pub(crate) type Annotations = Vec<(Vec<u8>, Vec<Vec<u8>>)>;

const PATTERN: &str =
    r"^\h*[*\/]+\h+@(php-cs-fixer-\w+\h+(?:@?[\w\/,])+|pun-ignore-file +[\w\/,.-]+)";

static UTF: LazyLock<Regex> = LazyLock::new(|| {
    RegexBuilder::new()
        .multi_line(true)
        .utf(true)
        .ucp(true)
        .build(PATTERN)
        .unwrap()
});
static BYTES: LazyLock<Regex> =
    LazyLock::new(|| RegexBuilder::new().multi_line(true).build(PATTERN).unwrap());

#[derive(Debug, PartialEq)]
pub(crate) enum FindError {
    Duplicated(String),
    UndefinedArrayKey,
}

pub(crate) fn find(tokens: &Tokens) -> Result<Annotations, FindError> {
    let mut comments: Vec<&[u8]> = vec![];
    let count = tokens.count();
    let mut index = 0;
    while index < count {
        let token = &tokens[index];
        if token.is_given_kind(&[T_OPEN_TAG, T_OPEN_TAG_WITH_ECHO, T_WHITESPACE])
            || token.equals(";", true)
        {
            index += 1;
            continue;
        }
        if token.is_kind(T_DECLARE) {
            let next_index = tokens.next_meaningful(index);
            index = tokens.find_block_end(Tokens::BLOCK_TYPE_PARENTHESIS, next_index) + 1;
            continue;
        }
        if token.is_comment() {
            comments.push(token.get_content());
            index += 1;
            continue;
        }
        break;
    }
    for token in tokens.as_slice()[index.min(count)..].iter().skip(1).rev() {
        if token.is_given_kind(&[T_CLOSE_TAG, T_WHITESPACE]) || token.equals(";", true) {
            continue;
        }
        if !token.is_comment() {
            break;
        }
        comments.push(token.get_content());
    }

    let subject = comments.join(&b'\n');
    let re = if std::str::from_utf8(&subject).is_ok() {
        &*UTF
    } else {
        &*BYTES
    };
    let mut annotations: Annotations = vec![];
    for caps in re.captures_iter(&subject) {
        let m = &caps.unwrap()[1];
        let at = m
            .iter()
            .position(|&c| c == b' ')
            .ok_or(FindError::UndefinedArrayKey)?;
        let (name, values) = (&m[..at], &m[at + 1..]);
        let values = values.split(|&c| c == b',').map(<[u8]>::to_vec);
        match annotations.iter_mut().find(|(n, _)| n == name) {
            Some((_, vals)) => vals.extend(values),
            None => annotations.push((name.to_vec(), values.collect())),
        }
    }
    for (annotation, vals) in &mut annotations {
        let mut duplicates: Vec<&[u8]> = vec![];
        for v in vals.iter() {
            if vals.iter().filter(|w| *w == v).count() > 1 && !duplicates.contains(&v.as_slice()) {
                duplicates.push(v);
            }
        }
        if !duplicates.is_empty() {
            let list: Vec<String> = duplicates
                .iter()
                .map(|d| String::from_utf8_lossy(d).into_owned())
                .collect();
            return Err(FindError::Duplicated(format!(
                "Duplicated values found for annotation \"@{}\": \"{}\".",
                String::from_utf8_lossy(annotation),
                list.join("\", \"")
            )));
        }
        vals.sort();
    }
    Ok(annotations)
}

pub(crate) fn ignored(annotations: &[(Vec<u8>, Vec<Vec<u8>>)]) -> Vec<Vec<u8>> {
    values(annotations, b"php-cs-fixer-ignore")
}

pub(crate) fn skipped(annotations: &[(Vec<u8>, Vec<Vec<u8>>)]) -> Vec<Vec<u8>> {
    let pun = values(annotations, b"pun-ignore-file")
        .into_iter()
        .filter_map(|id| {
            let id = String::from_utf8(id).ok()?;
            pun_config::fixer_of("format", &id).or_else(|| pun_config::fixer_of("lint", &id))
        })
        .map(|f| f.as_bytes().to_vec());
    ignored(annotations).into_iter().chain(pun).collect()
}

fn values(annotations: &[(Vec<u8>, Vec<Vec<u8>>)], name: &[u8]) -> Vec<Vec<u8>> {
    annotations
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}
