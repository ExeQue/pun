const LIST: &str = include_str!("php_internal_functions.txt");

pub(crate) fn is_internal_function(name: &[u8]) -> bool {
    let lower = name.to_ascii_lowercase();
    static SET: std::sync::LazyLock<std::collections::HashSet<&'static [u8]>> =
        std::sync::LazyLock::new(|| LIST.lines().map(str::as_bytes).collect());
    SET.contains(&lower[..])
}
