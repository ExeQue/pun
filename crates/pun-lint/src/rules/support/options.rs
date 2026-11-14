pub(crate) fn configure(
    options: &serde_json::Value,
    mut set: impl FnMut(&str, &serde_json::Value) -> Option<()>,
) -> Result<(), String> {
    match options {
        serde_json::Value::Null => Ok(()),
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if set(key, value).is_none() {
                    return Err(format!(
                        "unknown option `{key}` or a value of the wrong type"
                    ));
                }
            }
            Ok(())
        }
        _ => Err("options must be a table".into()),
    }
}

pub(crate) fn uint(value: &serde_json::Value, max: u64) -> Option<usize> {
    value.as_u64().filter(|&n| n <= max).map(|n| n as usize)
}

pub(crate) fn boolean(value: &serde_json::Value) -> Option<bool> {
    value.as_bool()
}
