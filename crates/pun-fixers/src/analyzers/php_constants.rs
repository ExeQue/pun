use std::collections::HashSet;
use std::sync::LazyLock;

const LIST: &str = include_str!("php_internal_constants.txt");

pub(crate) static INTERNAL_CONSTANTS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| LIST.lines().collect());
