use mago_syntax::cst::LiteralStringKind;
use pun_tokens::Kind;
use pun_tokens::php_compat::intval;

use crate::rules::phpunit::print_float;
use crate::rules::support::rector_names::doc::{Annotation, is_class_name};
use crate::rules::support::rector_names::phpdoc_parser::{ArrayItem, Key, Val, print_string};

const REMOVE_ARRAY: &str = "__remove_array__";

const SILENT_KEYS: &[(&str, &str)] = &[("Symfony\\Component\\Routing\\Annotation\\Route", "path")];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Quote {
    Single,
    Double,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Expr {
    Str(String, Quote),
    Int(i64),
    Float(f64),
    Const(&'static str),
    ClassConst(String, String),
    Array(Vec<Item>),
    New(String, Vec<Arg>),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub key: Option<Expr>,
    pub value: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Arg {
    pub name: Option<String>,
    pub value: Expr,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Attribute {
    pub name: String,
    pub args: Vec<Arg>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct AttributeConfig {
    pub tag: String,
    pub attribute_class: String,
    pub class_reference_fields: Vec<String>,
    pub use_value_as_attribute_argument: bool,
}

fn map_array(items: &[ArrayItem], nested: bool) -> Vec<Item> {
    items.iter().map(|item| map_item(item, nested)).collect()
}

fn map_item(item: &ArrayItem, nested: bool) -> Item {
    let value = match map_val(&item.value, nested) {
        Some(expr) => expr,
        None => {
            return Item {
                key: None,
                value: Expr::Str(REMOVE_ARRAY.into(), Quote::Single),
            };
        }
    };
    let key = match &item.key {
        Some(Key::Val(key)) => map_val(key, nested),
        Some(Key::Int(key)) => Some(Expr::Int(*key)),
        None => {
            if let Val::Str(value) = &item.value
                && value.starts_with('@')
                && !value.ends_with(')')
                && is_class_name(value.trim_start_matches('@'))
            {
                let annotation = Val::Annotation(Box::new(
                    crate::rules::support::rector_names::phpdoc_parser::Nested {
                        identifier: value.clone(),
                        class: String::new(),
                        values: Vec::new(),
                    },
                ));
                return map_item(
                    &ArrayItem {
                        key: None,
                        value: annotation,
                    },
                    nested,
                );
            }
            None
        }
    };
    Item { key, value }
}

fn map_val(value: &Val, nested: bool) -> Option<Expr> {
    Some(match value {
        Val::List(items) => Expr::Array(map_array(items, nested)),
        Val::Str(value) if value.contains("::") && !value.starts_with('"') => {
            class_const_fetch(value)
        }
        Val::Const(value) => match value.as_str() {
            "true" => Expr::Const("true"),
            "false" => Expr::Const("false"),
            _ => Expr::Int(intval(value)),
        },
        Val::Curly(items) => {
            let mut array = Vec::new();
            let mut position = -1;
            for item in items {
                let mut mapped = map_item(item, nested);
                let key = match &item.key {
                    Some(Key::Val(Val::Str(key))) if is_numeric(key) => intval(key),
                    _ => {
                        array.push(mapped);
                        continue;
                    }
                };
                position += 1;
                if position != key {
                    mapped.key = Some(Expr::Int(key));
                }
                array.push(mapped);
            }
            Expr::Array(array)
        }
        Val::Annotation(annotation) if nested => {
            let args = match annotation.values.is_empty() {
                true => Vec::new(),
                false => inline_array_node(map_array(&annotation.values, nested)),
            };
            Expr::New(
                annotation.identifier.trim_start_matches('@').to_string(),
                args,
            )
        }
        Val::Annotation(_) => return None,
        Val::Str(value) => {
            let quote = match value.contains('\'') && !value.contains('\n') {
                true => Quote::Double,
                false => Quote::Single,
            };
            match value.starts_with('"') && value.ends_with('"') {
                true => Expr::Str(value.trim_matches('"').to_string(), quote),
                false => Expr::Str(value.clone(), quote),
            }
        }
        Val::StringNode(value) => Expr::Str(value.clone(), string_node_quote(value)),
        Val::Null => Expr::Const("null"),
    })
}

fn string_node_quote(value: &str) -> Quote {
    match value.contains('\'') && !value.contains('\n') {
        true => Quote::Double,
        false => Quote::Single,
    }
}

fn class_const_fetch(value: &str) -> Expr {
    let string = || Expr::Str(value.to_string(), Quote::Single);
    let [class, constant] = value.split("::").collect::<Vec<_>>()[..] else {
        return string();
    };
    if class.is_empty()
        || !is_class_name(class.trim_start_matches('\\'))
        || !is_constant_name(constant)
    {
        return string();
    }
    Expr::ClassConst(class.to_string(), constant.to_string())
}

fn is_constant_name(name: &str) -> bool {
    let b = name.as_bytes();
    !b.is_empty()
        && (b[0].is_ascii_alphabetic() || b[0] == b'_' || b[0] >= 0x80)
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || *c == b'_' || *c >= 0x80)
}

fn inline_array_node(items: Vec<Item>) -> Vec<Arg> {
    items
        .into_iter()
        .map(|item| Arg {
            name: match item.key {
                Some(Expr::Str(key, _)) => Some(key),
                _ => None,
            },
            value: item.value,
        })
        .collect()
}

pub(crate) fn create(
    annotation: &Annotation,
    config: &AttributeConfig,
    name: String,
    constructor: &[(String, bool)],
    foldable: Option<&[String]>,
    nested: bool,
) -> Attribute {
    let mut values = annotation.values.clone();
    if let Some((_, key)) = SILENT_KEYS
        .iter()
        .find(|(class, _)| *class == annotation.class)
        && let Some(item) = values.iter_mut().find(|i| i.key.is_none())
    {
        item.key = Some(Key::Val(Val::Str(key.to_string())));
    }
    let mut items = map_array(&values, nested);
    for item in &mut items {
        if let Some(Expr::Str(key, _)) = &item.key
            && config.class_reference_fields.contains(key)
            && let Expr::Str(value, _) = &item.value
            && !value.is_empty()
        {
            item.value = Expr::ClassConst(
                format!("\\{}", value.trim_start_matches('\\')),
                "class".into(),
            );
        }
    }
    let mut args: Vec<Arg> = items
        .into_iter()
        .map(|item| {
            let mut value = item.value;
            if let Expr::Str(string, quote) = &mut value
                && string.contains('\'')
                && !string.contains('\n')
            {
                *quote = Quote::Double;
            }
            Arg {
                name: match item.key {
                    Some(Expr::Str(key, _)) => Some(key),
                    _ => None,
                },
                value,
            }
        })
        .collect();
    for (parameter, has_int) in constructor {
        for arg in &mut args {
            if let Expr::Array(items) = &mut arg.value
                && let Some(Item {
                    key: Some(Expr::Str(key, _)),
                    value,
                }) = items.first_mut()
                && key == parameter
                && *has_int
                && let Expr::Str(string, _) = value
                && is_numeric(string)
            {
                *value = Expr::Int(intval(string));
            }
        }
    }
    for arg in &mut args {
        if config.attribute_class == "OpenApi\\Attributes\\Property"
            && arg.name.as_deref() == Some("example")
        {
            continue;
        }
        if let Expr::Str(string, _) = &arg.value
            && is_numeric(string)
        {
            let float = php_float(string);
            arg.value = match float == intval(string) as f64 {
                true => Expr::Int(intval(string)),
                false => Expr::Float(float),
            };
        }
    }
    if let Some(parameters) = foldable {
        args = fold_unknown_named_args(args, parameters);
    }
    let mut attribute = Attribute { name, args };
    decorate(&config.attribute_class, &mut attribute);
    attribute
}

fn fold_unknown_named_args(args: Vec<Arg>, parameters: &[String]) -> Vec<Arg> {
    let mut kept = Vec::new();
    let mut options: Vec<Item> = Vec::new();
    let mut explicit: Option<&Arg> = None;
    for arg in &args {
        match arg.name.as_deref() {
            Some("options") => explicit = Some(arg),
            Some(name) if !parameters.iter().any(|p| p == name) => options.push(Item {
                key: Some(Expr::Str(name.to_string(), Quote::Single)),
                value: arg.value.clone(),
            }),
            _ => kept.push(arg.clone()),
        }
    }
    if options.is_empty() {
        return args;
    }
    if let Some(explicit) = explicit {
        let Expr::Array(items) = &explicit.value else {
            return args;
        };
        options = [items.clone(), options].concat();
    }
    kept.push(Arg {
        name: Some("options".to_string()),
        value: Expr::Array(options),
    });
    kept
}

pub(crate) fn has_remove_array_state(attribute: &Attribute) -> bool {
    attribute.args.iter().any(|arg| {
        matches!(&arg.value, Expr::Array(items) if items.iter().any(|i| matches!(&i.value, Expr::Str(s, _) if s == REMOVE_ARRAY)))
    })
}

pub(crate) fn is_invalid(attribute: &Attribute) -> bool {
    invalid_args(&attribute.args)
}

fn invalid_args(args: &[Arg]) -> bool {
    let positional_after_named = args
        .iter()
        .skip_while(|a| a.name.is_none())
        .any(|a| a.name.is_none());
    positional_after_named || args.iter().any(invalid_arg)
}

fn invalid_arg(arg: &Arg) -> bool {
    arg.name
        .as_deref()
        .is_some_and(|name| !is_constant_name(name))
        || invalid_expr(&arg.value)
}

fn invalid_expr(expr: &Expr) -> bool {
    match expr {
        Expr::New(class, args) => {
            let source = format!("<?php new {class}");
            let tokens = pun_tokens::tokenize(source.as_bytes());
            let name = matches!(
                tokens.last().and_then(|t| t.kind),
                Some(
                    Kind::T_STRING
                        | Kind::T_NAME_QUALIFIED
                        | Kind::T_NAME_FULLY_QUALIFIED
                        | Kind::T_STATIC
                )
            );
            tokens.len() != 4 || !name || invalid_args(args)
        }
        Expr::Array(items) => items.iter().any(|i| invalid_expr(&i.value)),
        _ => false,
    }
}

fn decorate(attribute_class: &str, attribute: &mut Attribute) {
    match attribute_class {
        "Doctrine\\ORM\\Mapping\\Column" => {
            if let Some(arg) = attribute
                .args
                .iter_mut()
                .find(|a| a.name.as_deref() == Some("nullable"))
                && let Expr::Str(value, _) = &arg.value
                && (value == "true" || value == "false")
            {
                arg.value = Expr::Const(if value == "true" { "true" } else { "false" });
            }
        }
        "Sensio\\Bundle\\FrameworkExtraBundle\\Configuration\\ParamConverter" => {
            if let Some(arg) = attribute.args.first_mut() {
                arg.name = None;
            }
        }
        _ => {}
    }
}

pub(crate) fn from_simple_tag(config: &AttributeConfig, value: Option<&str>) -> Attribute {
    Attribute {
        name: format!("\\{}", config.attribute_class),
        args: match value {
            Some(value) if !value.is_empty() => vec![Arg {
                name: None,
                value: Expr::Str(value.to_string(), Quote::Single),
            }],
            _ => Vec::new(),
        },
    }
}

pub(crate) fn resolve_value(value: &str, original: Option<&str>) -> (String, String) {
    let mut value = value.to_string();
    if let Some(original) = original {
        if value.is_empty() {
            value = original.to_string();
        } else {
            let comment = ltrim(original, &value);
            if !comment.is_empty() {
                value = format!("{value}\n{comment}");
            }
        }
    }
    let mut comment = String::new();
    if value.contains('\n') {
        let mut keep_joining = true;
        let mut joined = String::new();
        let mut previous_end_slash = false;
        for (key, line) in value
            .split('\n')
            .map(|l| l.strip_suffix('\r').unwrap_or(l))
            .enumerate()
        {
            if keep_joining {
                joined.push_str(line.trim_end_matches('\\'));
            }
            if line.ends_with('\\') {
                previous_end_slash = true;
                continue;
            }
            if !previous_end_slash && key > 0 {
                match comment.is_empty() {
                    true => comment.push_str(line),
                    false => comment.push_str(&format!("\n * {line}")),
                }
            }
            keep_joining = false;
        }
        value = joined;
    }
    (value, comment)
}

pub(crate) fn trailing_comment(original: &str) -> Option<String> {
    if !original.starts_with('(') {
        return None;
    }
    let bytes = original.as_bytes();
    let (mut depth, mut quote, mut i) = (0, None, 0);
    while i < bytes.len() {
        let c = bytes[i];
        match quote {
            Some(q) => {
                if c == b'\\' {
                    i += 1;
                } else if c == q {
                    quote = None;
                }
            }
            None if c == b'"' || c == b'\'' => quote = Some(c),
            None if c == b'(' => depth += 1,
            None if c == b')' => {
                depth -= 1;
                if depth == 0 {
                    let comment = original[i + 1..].trim_matches(PHP_WS);
                    return comment
                        .chars()
                        .any(char::is_alphabetic)
                        .then(|| comment.to_string());
                }
            }
            None => {}
        }
        i += 1;
    }
    None
}

const PHP_WS: [char; 6] = [' ', '\t', '\n', '\r', '\0', '\x0B'];

pub(crate) fn attribute_name(
    config: &AttributeConfig,
    identifier: &str,
    uses: &[(String, Option<String>)],
) -> (String, Option<(usize, String)>) {
    if config.attribute_class == config.tag {
        let name = identifier.trim_start_matches('@');
        return match name.strip_prefix('\\') {
            Some(fq) => (format!("\\{}", fq.trim_start_matches('\\')), None),
            None => (name.to_string(), None),
        };
    }
    let short = identifier.trim_matches('@');
    for (i, (name, alias)) in uses.iter().enumerate() {
        let Some(alias) = alias else {
            continue;
        };
        if !short.starts_with(alias.as_str()) {
            continue;
        }
        let fully_qualified = format!("{name}{}", ltrim(short, alias));
        if fully_qualified != config.tag {
            continue;
        }
        let annotation_parts = fully_qualified.split('\\').count();
        let attribute_parts: Vec<&str> = config.attribute_class.split('\\').collect();
        if annotation_parts != attribute_parts.len() {
            continue;
        }
        let import_parts = name.matches('\\').count() + 1;
        let new_import = attribute_parts[..import_parts.min(attribute_parts.len())].join("\\");
        let short_parts = attribute_parts.len() - import_parts.min(attribute_parts.len());
        let rest = &attribute_parts[attribute_parts.len()
            - if short_parts == 0 {
                attribute_parts.len()
            } else {
                short_parts
            }..];
        return (
            format!("{alias}\\{}", rest.join("\\")),
            Some((i, new_import)),
        );
    }
    (format!("\\{}", config.attribute_class), None)
}

fn ltrim<'a>(string: &'a str, characters: &str) -> &'a str {
    let set = character_mask(characters);
    let at = string
        .bytes()
        .position(|b| !set[b as usize])
        .unwrap_or(string.len());
    &string[at..]
}

fn character_mask(characters: &str) -> [bool; 256] {
    let mut set = [false; 256];
    let b = characters.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if i + 3 < b.len() && b[i + 1] == b'.' && b[i + 2] == b'.' && b[i + 3] >= b[i] {
            for c in b[i]..=b[i + 3] {
                set[c as usize] = true;
            }
            i += 4;
        } else {
            set[b[i] as usize] = true;
            i += 1;
        }
    }
    set
}

pub(crate) fn is_numeric(value: &str) -> bool {
    let trimmed = value
        .trim_start_matches([' ', '\t', '\n', '\r', '\x0B', '\x0C'])
        .trim_end_matches([' ', '\t', '\n', '\r', '\x0B', '\x0C']);
    let body = trimmed.strip_prefix(['+', '-']).unwrap_or(trimmed);
    let (mantissa, exponent) = match body.find(['e', 'E']) {
        Some(at) => (&body[..at], Some(&body[at + 1..])),
        None => (body, None),
    };
    let (int, fraction) = match mantissa.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (mantissa, None),
    };
    let digits = |s: &str| s.bytes().all(|c| c.is_ascii_digit());
    let mantissa_ok = digits(int)
        && fraction.is_none_or(digits)
        && !(int.is_empty() && fraction.is_none_or(str::is_empty));
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && digits(e)
    });
    mantissa_ok && exponent_ok
}

fn php_float(value: &str) -> f64 {
    value.trim().parse().unwrap_or(0.0)
}

impl std::fmt::Display for Attribute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#[{}", self.name)?;
        if !self.args.is_empty() {
            write!(f, "({})", print_args(&self.args))?;
        }
        f.write_str("]")
    }
}

fn print_args(args: &[Arg]) -> String {
    args.iter()
        .map(|arg| match &arg.name {
            Some(name) => format!("{name}: {}", print(&arg.value)),
            None => print(&arg.value),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn print(expr: &Expr) -> String {
    match expr {
        Expr::Str(value, Quote::Single) => {
            crate::rules::complexity::prefer_absolute_include::print_string(
                LiteralStringKind::SingleQuoted,
                value.as_bytes(),
            )
            .unwrap()
        }
        Expr::Str(value, Quote::Double) => {
            print_string(value.as_bytes(), "\"").expect("a Rust string is UTF-8")
        }
        Expr::Int(value) => value.to_string(),
        Expr::Float(value) => print_float(*value),
        Expr::Const(name) => name.to_string(),
        Expr::ClassConst(class, constant) => format!("{class}::{constant}"),
        Expr::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(|item| match &item.key {
                    Some(key) => format!("{} => {}", print(key), print(&item.value)),
                    None => print(&item.value),
                })
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::New(class, args) => format!("new {class}({})", print_args(args)),
    }
}
