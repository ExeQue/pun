use crate::mapping;
use anyhow::{Result, anyhow, bail};
use serde_json::{Map, Value};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Int,
    String,
    Enum,
    List,
    ListOfLists,
    Map,
    IntOrMap,
}

#[derive(Debug)]
pub struct RuleOption {
    pub name: String,
    pub upstream: String,
    pub kind: Kind,
    pub values: Vec<(String, String)>,
    pub keys: Vec<(String, String)>,
    pub null: Option<String>,
    pub nullable: bool,
    /// Pun's own option (`origin = "pun"`), which no upstream tool has.
    pub pun: bool,
}

const KINDS: [(&str, Kind); 8] = [
    ("bool", Kind::Bool),
    ("int", Kind::Int),
    ("string", Kind::String),
    ("enum", Kind::Enum),
    ("list", Kind::List),
    ("list-of-lists", Kind::ListOfLists),
    ("map", Kind::Map),
    ("int-or-map", Kind::IntOrMap),
];

fn all() -> &'static Vec<(String, Vec<RuleOption>)> {
    static T: LazyLock<Vec<(String, Vec<RuleOption>)>> = LazyLock::new(|| {
        let pairs = |e: &toml::Value, k: &str| -> Vec<(String, String)> {
            (e.get(k)
                .and_then(toml::Value::as_table)
                .into_iter()
                .flatten())
            .map(|(p, u)| (p.clone(), u.as_str().unwrap().to_string()))
            .collect()
        };
        mapping::under("options")
            .map(|(id, opts)| {
                let opts = (opts.table().iter())
                    .map(|(name, e)| {
                        let s = |k: &str| e.get(k).and_then(toml::Value::as_str).map(String::from);
                        RuleOption {
                            name: name.clone(),
                            upstream: s("upstream").unwrap(),
                            kind: KINDS
                                .iter()
                                .find(|(k, _)| s("type").unwrap() == *k)
                                .unwrap()
                                .1,
                            values: pairs(e, "values"),
                            keys: pairs(e, "keys"),
                            null: s("null"),
                            nullable: e.get("nullable").is_some(),
                            pun: s("origin").as_deref() == Some("pun"),
                        }
                    })
                    .collect();
                (id.to_string(), opts)
            })
            .collect()
    });
    &T
}

pub fn rule_options(id: &str) -> &'static [RuleOption] {
    all()
        .iter()
        .find(|(r, _)| r == id)
        .map_or(&[], |(_, o)| o.as_slice())
}

fn shorthand(opts: &[RuleOption]) -> Option<&RuleOption> {
    match opts {
        [o] if !matches!(o.kind, Kind::Bool | Kind::Map | Kind::IntOrMap) => Some(o),
        _ => None,
    }
}

fn quote(names: impl Iterator<Item = impl std::fmt::Display>) -> String {
    names
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn strings<'a>(values: impl Iterator<Item = &'a String>) -> String {
    values
        .map(|v| format!("{v:?}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub(crate) fn to_fixer(rule: &str, id: &str, v: toml::Value) -> Result<Value> {
    let opts = rule_options(id);
    let table = match v {
        toml::Value::Boolean(b) => return Ok(Value::Bool(b)),
        toml::Value::Table(t) if opts.is_empty() => {
            bail!(
                "`{rule}` has no options: it is true or false{}",
                if t.is_empty() {
                    ""
                } else {
                    ", without a table"
                }
            );
        }
        toml::Value::Table(t) => t,
        v => {
            if let Some(o) = shorthand(opts) {
                toml::Table::from_iter([(o.name.clone(), v)])
            } else {
                let one = match opts {
                    [] => "true or false".to_string(),
                    _ => "true, false or a table of options".to_string(),
                };
                bail!("`{rule}` must be {one}");
            }
        }
    };
    let mut out = Map::new();
    for (k, v) in table {
        let Some(o) = opts.iter().find(|o| o.name == k) else {
            let up = opts
                .iter()
                .find(|o| o.upstream == k || o.upstream.replace('_', "-") == k);
            return Err(anyhow!(match up {
                Some(o) => format!("`{rule}` has no option `{k}`: use `{}`", o.name),
                None => format!(
                    "`{rule}` has no option `{k}`; its options are {}",
                    quote(opts.iter().map(|o| &o.name))
                ),
            }));
        };
        let got = v.to_string();
        let value = fixer_value(o, v)
            .ok_or_else(|| anyhow!("`{rule}` option `{k}` = {got}: expected {}", expected(o)))?;
        out.insert(o.upstream.clone(), value);
    }
    Ok(Value::Object(out))
}

fn expected(o: &RuleOption) -> String {
    let list = strings(o.values.iter().map(|(p, _)| p));
    let base = match o.kind {
        Kind::Bool => "true or false".to_string(),
        Kind::Int => "an integer".to_string(),
        Kind::String => "a string".to_string(),
        Kind::Enum => format!("one of {list}"),
        Kind::List if o.values.is_empty() => "a list of strings".to_string(),
        Kind::List => format!("a list of {list}"),
        Kind::ListOfLists => "a list of lists of strings".to_string(),
        Kind::Map => {
            let keys = match &o.keys[..] {
                [] => "string keys".to_string(),
                k => format!("the keys {}", strings(k.iter().map(|(p, _)| p))),
            };
            let values = match &o.values[..] {
                [] => "string values".to_string(),
                _ => format!("the values {list}"),
            };
            format!("a table with {keys} and {values}")
        }
        Kind::IntOrMap => "an integer or a table of integers".to_string(),
    };
    match &o.null {
        Some(n) => format!("{base}, or {n:?}"),
        None => base,
    }
}

#[expect(
    clippy::many_single_char_names,
    reason = "each match arm binds the option value it checks under a one-letter name"
)]
fn fixer_value(o: &RuleOption, v: toml::Value) -> Option<Value> {
    use toml::Value as T;
    if let (T::String(s), Some(n)) = (&v, &o.null)
        && s == n
        && o.kind != Kind::Map
    {
        return Some(Value::Null);
    }
    let up = |s: &str| -> Option<Value> {
        match &o.values[..] {
            [] => Some(Value::from(s)),
            vs => vs
                .iter()
                .find(|(p, _)| p == s)
                .map(|(_, u)| Value::from(u.as_str())),
        }
    };
    let str_list =
        |a: &[T]| -> Option<Vec<Value>> { a.iter().map(|e| e.as_str().map(Value::from)).collect() };
    Some(match (o.kind, v) {
        (Kind::Bool, T::Boolean(b)) => Value::Bool(b),
        (Kind::Int | Kind::IntOrMap, T::Integer(i)) => Value::from(i),
        (Kind::String, T::String(s)) => Value::String(s),
        (Kind::Enum, T::String(s)) => up(&s)?,
        (Kind::List, T::Array(a)) => Value::Array(
            a.iter()
                .map(|e| e.as_str().and_then(up))
                .collect::<Option<_>>()?,
        ),
        (Kind::ListOfLists, T::Array(a)) => Value::Array(
            a.iter()
                .map(|e| e.as_array().and_then(|l| str_list(l)).map(Value::Array))
                .collect::<Option<_>>()?,
        ),
        (Kind::Map, T::Table(t)) => Value::Object(
            t.into_iter()
                .map(|(k, v)| {
                    let k = match &o.keys[..] {
                        [] => k,
                        ks => ks.iter().find(|(p, _)| *p == k)?.1.clone(),
                    };
                    let v = match v {
                        T::String(s) if o.null.as_ref() == Some(&s) => Value::Null,
                        T::String(s) => up(&s)?,
                        _ => return None,
                    };
                    Some((k, v))
                })
                .collect::<Option<_>>()?,
        ),
        (Kind::IntOrMap, T::Table(t)) => Value::Object(
            t.into_iter()
                .map(|(k, v)| v.as_integer().map(|i| (k, Value::from(i))))
                .collect::<Option<_>>()?,
        ),
        _ => return None,
    })
}

pub fn to_pun(fixer: &str, id: &str, options: &Value) -> Result<Value> {
    let Value::Object(m) = options else {
        return Ok(Value::Bool(true));
    };
    // A format rule's own options (the indent and line ending) are PHP-CS-Fixer's `setIndent()` and `setLineEnding()`,
    // which `pun migrate` passes as options of the rules that enforce them.
    let format = id.starts_with("format/");
    let opts: Vec<&RuleOption> = (rule_options(id).iter())
        .filter(|o| !o.pun || format)
        .collect();
    if opts.is_empty() {
        bail!("[{fixer}] Is not configurable.");
    }
    let invalid = |msg: String| anyhow!("[{fixer}] Invalid configuration: {msg}");
    let mut out = Map::new();
    for (k, v) in m {
        let Some(o) = opts.iter().find(|o| o.upstream == *k) else {
            let defined = (opts.iter().map(|o| format!("{:?}", o.upstream)))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(invalid(format!(
                "The option {k:?} does not exist. Defined options are: {defined}."
            )));
        };
        if let Some(msg) = element_types(o, v).or_else(|| sort_types(fixer, k, v)) {
            return Err(invalid(msg));
        }
        let Some(p) = pun_value(o, v) else {
            return Err(invalid(rejected(fixer, o, v)));
        };
        if !p.is_null() {
            out.insert(o.name.clone(), p);
        }
    }
    let short = shorthand(rule_options(id)).filter(|_| id.starts_with("format/"));
    Ok(match short {
        Some(o) if out.len() == 1 && out.contains_key(&o.name) => out.remove(&o.name).unwrap(),
        _ => Value::Object(out),
    })
}

pub fn natural_language_join<S: AsRef<str>>(names: impl IntoIterator<Item = S>) -> String {
    let mut names: Vec<String> = (names.into_iter())
        .map(|n| format!("\"{}\"", n.as_ref()))
        .collect();
    let last = names.pop().expect("Array of names cannot be empty.");
    if names.is_empty() {
        last
    } else {
        format!("{} and {last}", names.join(", "))
    }
}

fn sort_types(fixer: &str, option: &str, v: &Value) -> Option<String> {
    const SUPPORTED: [&str; 3] = ["class", "const", "function"];
    let (("ordered_imports", "imports_order"), Value::Array(a)) = ((fixer, option), v) else {
        return None;
    };
    let given: Vec<&str> = a.iter().filter_map(Value::as_str).collect();
    let missing: Vec<&str> = SUPPORTED
        .into_iter()
        .filter(|t| !given.contains(t))
        .collect();
    let unknown: Vec<&str> = given
        .into_iter()
        .filter(|t| !SUPPORTED.contains(t))
        .collect();
    let (what, list) = match (&missing[..], &unknown[..]) {
        ([], []) => return None,
        ([], u) => ("Unknown", u),
        (m, _) => ("Missing", m),
    };
    let types = if list.len() == 1 { "type" } else { "types" };
    Some(format!(
        "{what} sort {types} {}.",
        natural_language_join(list.iter())
    ))
}

fn allowed_types(o: &RuleOption) -> &'static [&'static str] {
    match o.kind {
        Kind::Bool => &["bool"],
        Kind::Int => &["int"],
        Kind::String => &["string"],
        Kind::ListOfLists => &["string[][]"],
        Kind::IntOrMap => &["int", "int[]"],
        Kind::Map if o.null.is_some() => &["array"],
        Kind::List | Kind::Map => &["string[]"],
        Kind::Enum => &[],
    }
}

fn debug_type(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(n) if n.is_f64() => "float",
        Value::Number(_) => "int",
        Value::String(_) => "string",
        Value::Array(_) | Value::Object(_) => "array",
    }
}

fn verify_type(ty: &str, v: &Value, invalid: &mut Vec<&'static str>, level: usize) -> bool {
    let entries: Vec<&Value> = match v {
        Value::Array(a) => a.iter().collect(),
        Value::Object(m) => m.values().collect(),
        _ => Vec::new(),
    };
    if let (Some(inner), "array") = (ty.strip_suffix("[]"), debug_type(v)) {
        let failed = (entries.into_iter())
            .filter(|e| !verify_type(inner, e, invalid, level + 1))
            .count();
        return failed == 0;
    }
    let t = debug_type(v);
    if t == ty {
        return true;
    }
    if (invalid.is_empty() || level > 0) && !invalid.contains(&t) {
        invalid.push(t);
    }
    false
}

fn element_types(o: &RuleOption, v: &Value) -> Option<String> {
    let types = allowed_types(o);
    if !v.is_array() && !v.is_object() || !types.iter().any(|t| t.ends_with("[]")) {
        return None;
    }
    let mut invalid = Vec::new();
    if types.iter().any(|t| verify_type(t, v, &mut invalid, 0)) {
        return None;
    }
    Some(format!(
        "The option {:?} with value array is expected to be of type \"{}\", but one of the elements is of type \"{}\".",
        o.upstream,
        types.join("\" or \""),
        invalid.join("|")
    ))
}

#[expect(
    clippy::many_single_char_names,
    reason = "each match arm binds the option value it checks under a one-letter name"
)]
fn rejected(fixer: &str, o: &RuleOption, v: &Value) -> String {
    let k = &o.upstream;
    let ups = |l: &[(String, String)]| natural_language_join(l.iter().map(|(_, u)| u));
    let bad = |l: &[(String, String)], s: &str| !l.iter().any(|(_, u)| u == s);
    let entry = v.as_object().and_then(|m| {
        m.iter().find_map(|(e, s)| match s.as_str() {
            _ if !o.keys.is_empty() && bad(&o.keys, e) => Some((e.as_str(), None)),
            Some(s) if bad(&o.values, s) => Some((e.as_str(), Some(s))),
            _ => None,
        })
    });
    let item = (v.as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .find(|s| bad(&o.values, s));
    match (fixer, v, entry, item) {
        (_, Value::Null, ..) if o.kind != Kind::Enum => {
            let types = allowed_types(o).join("\" or \"");
            format!(
                "The option {k:?} with value null is expected to be of type \"{types}\", but is of type \"null\"."
            )
        }
        ("class_attributes_separation", _, Some((e, None)), _) => format!(
            "Unexpected element type, expected any of {}, got \"string#{e}\".",
            ups(&o.keys)
        ),
        ("class_attributes_separation", _, Some((_, Some(s))), _) => format!(
            "Unexpected spacing for element type \"{s}\", expected any of {}, got \"string#{s}\".",
            ups(&o.values)
        ),
        ("binary_operator_spaces", _, Some((e, None)), _) => format!(
            "Unexpected \"operators\" key, expected any of {}, got \"string#{e}\".",
            ups(&o.keys)
        ),
        ("binary_operator_spaces", _, Some((e, Some(s))), _) => {
            let values: Vec<String> = (o.values.iter().map(|(_, u)| format!("'{u}'")))
                .chain(["null".to_string()])
                .collect();
            format!(
                "Unexpected value for operator \"{e}\", expected any of {}, got \"string#{s}\".",
                natural_language_join(&values)
            )
        }
        ("phpdoc_tag_type", _, Some((_, Some(s))), _) => format!("Unknown tag type \"{s}\"."),
        ("group_import", _, _, Some(t)) => format!(
            "Invalid group type: {t}, allowed types: {}.",
            ups(&o.values)
        ),
        _ => {
            let accepted = match o.kind {
                Kind::Enum => {
                    let list = (o.values.iter().map(|(_, u)| format!("{u:?}")))
                        .chain(o.null.iter().map(|_| "null".into()))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!(" Accepted values are: {list}.")
                }
                _ => String::new(),
            };
            format!(
                "The option {k:?} with value {} is invalid.{accepted}",
                php_value(v)
            )
        }
    }
}

fn php_value(v: &Value) -> String {
    match v {
        Value::Array(_) | Value::Object(_) => "array".into(),
        v => v.to_string(),
    }
}

fn pun_value(o: &RuleOption, v: &Value) -> Option<Value> {
    let pun = |v: &Value| -> Option<Value> {
        match (v, &o.values[..]) {
            (Value::Null, _) if o.nullable => Some(Value::Null),
            (Value::Null, _) => o.null.as_deref().map(Value::from),
            (Value::String(s), vs) if !vs.is_empty() => vs
                .iter()
                .find(|(_, u)| u == s)
                .map(|(p, _)| Value::from(p.as_str())),
            (v, _) => Some(v.clone()),
        }
    };
    match (o.kind, v) {
        (Kind::Map | Kind::IntOrMap, Value::Array(a)) if a.is_empty() => {
            Some(Value::Object(Map::new()))
        }
        (Kind::Enum, Value::String(_) | Value::Null) => pun(v),
        (Kind::Enum, _) | (Kind::Map, Value::Null) => None,
        (Kind::List, Value::Array(a)) => a.iter().map(pun).collect::<Option<_>>().map(Value::Array),
        (Kind::Map, Value::Object(m)) => m
            .iter()
            .map(|(k, v)| {
                let k = match &o.keys[..] {
                    [] => k.clone(),
                    ks => ks.iter().find(|(_, u)| u == k)?.0.clone(),
                };
                Some((k, pun(v)?))
            })
            .collect::<Option<_>>()
            .map(Value::Object),
        (_, Value::Null) => pun(v),
        _ => Some(v.clone()),
    }
}
