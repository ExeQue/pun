use std::collections::HashSet;
use std::sync::LazyLock;

const LIST: &str = include_str!("php_internal_functions.txt");

static SET: LazyLock<HashSet<&'static [u8]>> =
    LazyLock::new(|| LIST.lines().map(str::as_bytes).collect());

pub(crate) fn is_internal_function(name: &[u8]) -> bool {
    let lower = name.to_ascii_lowercase();
    SET.contains(&lower[..])
}
