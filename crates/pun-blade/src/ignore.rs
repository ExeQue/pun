//! `format-ignore` / `prettier-ignore` comment markers (prettier-plugin-blade's `ignore-markers.ts`).

use crate::tree::js_trim;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IgnoreKind {
    Ignore,
    Start,
    End,
}

pub(crate) fn kind_from_payload(payload: &str) -> Option<IgnoreKind> {
    match js_trim(payload).to_lowercase().as_str() {
        "format-ignore" | "prettier-ignore" => Some(IgnoreKind::Ignore),
        "format-ignore-start" | "prettier-ignore-start" => Some(IgnoreKind::Start),
        "format-ignore-end" | "prettier-ignore-end" => Some(IgnoreKind::End),
        _ => None,
    }
}

/// `blade` selects `{{-- … --}}`, otherwise `<!-- … -->`.
pub(crate) fn comment_kind(text: &str, blade: bool) -> Option<IgnoreKind> {
    let (open, close) = if blade {
        ("{{--", "--}}")
    } else {
        ("<!--", "-->")
    };
    let inner = text.strip_prefix(open)?.strip_suffix(close)?;
    kind_from_payload(inner)
}
