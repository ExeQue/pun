//! `@prettier/parse-srcset`: the WHATWG srcset parsing algorithm.

pub(crate) struct Candidate {
    pub(crate) url: String,
    /// Width, height and density, as JS prints the numbers.
    pub(crate) descriptors: [Option<String>; 3],
}

fn is_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\x0c' | '\r' | ' ')
}

fn is_non_negative_integer(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `/^-?(?:[0-9]+|[0-9]*\.[0-9]+)(?:[eE][+-]?[0-9]+)?$/`
fn is_floating_point(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let (mantissa, exponent) = match s.find(['e', 'E']) {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };
    let mantissa_ok = match mantissa.split_once('.') {
        Some((int, frac)) => int.bytes().all(|b| b.is_ascii_digit()) && is_non_negative_integer(frac),
        None => is_non_negative_integer(mantissa),
    };
    let exponent_ok = exponent.is_none_or(|e| is_non_negative_integer(e.strip_prefix(['+', '-']).unwrap_or(e)));
    mantissa_ok && exponent_ok
}

/// `String(number)` for the numbers a descriptor holds.
fn js_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e21 { format!("{v:.0}") } else { format!("{v}") }
}

fn parse_descriptors(url: String, descriptors: &[String]) -> Option<Candidate> {
    let (mut w, mut d, mut h): (Option<f64>, Option<f64>, Option<f64>) = (None, None, None);
    let truthy = |v: Option<f64>| v.is_some_and(|v| v != 0.0);
    for desc in descriptors {
        let last = desc.chars().last()?;
        let value = &desc[..desc.len() - last.len_utf8()];
        if is_non_negative_integer(value) && last == 'w' {
            if truthy(w) || truthy(d) {
                return None;
            }
            let v: f64 = value.parse().ok()?;
            if v == 0.0 {
                return None;
            }
            w = Some(v);
        } else if is_floating_point(value) && last == 'x' {
            if truthy(w) || truthy(d) || truthy(h) {
                return None;
            }
            let v: f64 = value.parse().ok()?;
            if v < 0.0 {
                return None;
            }
            d = Some(v);
        } else if is_non_negative_integer(value) && last == 'h' {
            if truthy(h) || truthy(d) {
                return None;
            }
            let v: f64 = value.parse().ok()?;
            if v == 0.0 {
                return None;
            }
            h = Some(v);
        } else {
            return None;
        }
    }
    let shown = |v: Option<f64>| v.filter(|v| *v != 0.0).map(js_number);
    Some(Candidate { url, descriptors: [shown(w), shown(h), shown(d)] })
}

/// `None` where the JS parser throws.
pub(crate) fn parse_srcset(input: &str) -> Option<Vec<Candidate>> {
    let chars: Vec<char> = input.chars().collect();
    let mut pos = 0;
    let mut candidates = Vec::new();
    loop {
        while pos < chars.len() && (is_space(chars[pos]) || chars[pos] == ',') {
            pos += 1;
        }
        if pos >= chars.len() {
            return (!candidates.is_empty()).then_some(candidates);
        }
        let start = pos;
        while pos < chars.len() && !is_space(chars[pos]) {
            pos += 1;
        }
        let mut url: String = chars[start..pos].iter().collect();
        let mut descriptors = Vec::new();
        if url.ends_with(',') {
            url = url.trim_end_matches(',').to_owned();
        } else {
            while pos < chars.len() && is_space(chars[pos]) {
                pos += 1;
            }
            let mut current = String::new();
            let mut state = 0; // 0 in descriptor, 1 in parens, 2 after descriptor
            loop {
                let c = chars.get(pos).copied();
                match state {
                    0 => match c {
                        Some(c) if is_space(c) => {
                            if !current.is_empty() {
                                descriptors.push(std::mem::take(&mut current));
                                state = 2;
                            }
                        }
                        Some(',') => {
                            pos += 1;
                            if !current.is_empty() {
                                descriptors.push(std::mem::take(&mut current));
                            }
                            break;
                        }
                        Some('(') => {
                            current.push('(');
                            state = 1;
                        }
                        None => {
                            if !current.is_empty() {
                                descriptors.push(std::mem::take(&mut current));
                            }
                            break;
                        }
                        Some(c) => current.push(c),
                    },
                    1 => match c {
                        Some(')') => {
                            current.push(')');
                            state = 0;
                        }
                        None => {
                            descriptors.push(std::mem::take(&mut current));
                            break;
                        }
                        Some(c) => current.push(c),
                    },
                    _ => match c {
                        Some(c) if is_space(c) => {}
                        None => break,
                        Some(_) => {
                            state = 0;
                            pos -= 1;
                        }
                    },
                }
                pos += 1;
            }
        }
        candidates.push(parse_descriptors(url, &descriptors)?);
    }
}
