use std::sync::LazyLock;

/// Mago's list of the functions each PHP extension defines, in Mago's order. The data file has each extension's name
/// on a line of its own, its functions indented under it.
pub(crate) static EXTENSION_FUNCTIONS: LazyLock<Vec<(&str, Vec<&str>)>> = LazyLock::new(|| {
    let mut extensions: Vec<(&str, Vec<&str>)> = Vec::new();
    for line in include_str!("../../../data/extension-functions.txt").lines() {
        match (line.strip_prefix("    "), extensions.last_mut()) {
            (Some(function), Some((_, functions))) => functions.push(function),
            _ => extensions.push((line, Vec::new())),
        }
    }
    extensions
});
