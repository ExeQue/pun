use mago_phpdoc_syntax::PHPDocParser;
use mago_phpdoc_syntax::cst::{Tag, TagValue};

pub(crate) fn with_tag<R>(text: &str, f: impl FnOnce(&Tag<'_>) -> R) -> Option<R> {
    let arena = mago_allocator::LocalArena::new();
    let content = format!("/** {text} */");
    let content: &[u8] = mago_allocator::Arena::alloc_slice_copy(&arena, content.as_bytes());
    let document = PHPDocParser::parse(&arena, mago_database::file::FileId::zero(), content);
    document.tags().next().map(f)
}

pub(crate) fn named(tag: &Tag<'_>, name: &str) -> bool {
    tag.vendor.is_none() && tag.name.value == name.as_bytes()
}

pub(crate) fn is_valid(tag: &Tag<'_>) -> bool {
    !matches!(tag.value, TagValue::Invalid(_))
}
