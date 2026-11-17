use crate::OptionsError;

pub(crate) fn configure(
    options: &serde_json::Value,
    mut set: impl FnMut(&str, &serde_json::Value) -> Option<()>,
) -> Result<(), OptionsError> {
    match options {
        serde_json::Value::Null => Ok(()),
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if set(key, value).is_none() {
                    return Err(
                        format!("unknown option `{key}` or a value of the wrong type").into(),
                    );
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

/// A Pun-only list option of a rule merged from several Rector rules, naming the parts that run (one per Rector rule),
/// so `pun migrate` can keep the rule to the Rector rules rector.php runs: whether `part` is listed under `key`. Every
/// part runs when the option is not set; another key, or a name not in `parts`, is an error.
pub(crate) fn part_on(
    options: &serde_json::Value,
    key: &str,
    parts: &[&str],
    part: &str,
) -> Result<bool, OptionsError> {
    let mut on = true;
    configure(options, |k, value| {
        let list = value.as_array().filter(|_| k == key)?;
        let names = (list.iter())
            .map(|v| v.as_str().filter(|s| parts.contains(s)))
            .collect::<Option<Vec<&str>>>()?;
        on = names.contains(&part);
        Some(())
    })?;
    Ok(on)
}
