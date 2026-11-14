pub mod analyzers;
pub mod prettier;

include!(concat!(env!("OUT_DIR"), "/fixers.rs"));

pub(crate) fn configured(mut f: impl pun_format::Fixer + 'static) -> Box<dyn pun_format::Fixer> {
    f.configure(&serde_json::Value::Null)
        .expect("the defaults are a valid configuration");
    Box::new(f)
}

#[cfg(test)]
pub(crate) fn fix_with(fixer: &dyn pun_format::Fixer, code: &str) -> String {
    let mut tokens = pun_tokens::Tokens::from_code(code.as_bytes());
    fixer.apply(std::path::Path::new("t.php"), &mut tokens);
    String::from_utf8(tokens.generate_code()).unwrap()
}
