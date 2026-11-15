use std::collections::HashSet;

const INT_MAX: u64 = 2147483647;

pub(super) fn printf_count(format: &[u8], hh: bool) -> Option<usize> {
    Some(
        printf_uses(format, hh)?
            .iter()
            .map(|u| u.index + 1)
            .max()
            .unwrap_or(0),
    )
}

pub(super) struct Use {
    pub(super) index: usize,
    pub(super) kind: &'static str,
    pub(super) specifier: u8,
    pub(super) placeholder: Vec<u8>,
    pub(super) number: usize,
}

pub(super) fn printf_uses(format: &[u8], hh: bool) -> Option<Vec<Use>> {
    let length = format.len();
    let (mut i, mut next, mut number) = (0, 0, 0);
    let mut uses = Vec::new();
    let mut next_argument = || {
        next += 1;
        next - 1
    };
    while let Some(percent) = format[i..].iter().position(|b| *b == b'%').map(|p| p + i) {
        i = percent + 1;
        if format.get(i) == Some(&b'%') {
            i += 1;
            continue;
        }
        let mut pending = Vec::new();
        let mut argument = None;
        if !format.get(i).is_some_and(u8::is_ascii_alphabetic) {
            argument = argument_number(format, &mut i)?;
            while let Some(&char) = format.get(i) {
                if matches!(char, b' ' | b'0' | b'-' | b'+') {
                    i += 1;
                } else if char == b'\'' {
                    if length - i <= 1 {
                        return None;
                    }
                    i += 2;
                } else {
                    break;
                }
            }
            if format.get(i) == Some(&b'*') {
                i += 1;
                let width = argument_number(format, &mut i)?.unwrap_or_else(&mut next_argument);
                pending.push((width, "width"));
            } else if format.get(i).is_some_and(u8::is_ascii_digit)
                && self::number(format, &mut i) >= INT_MAX
            {
                return None;
            }
            if format.get(i) == Some(&b'.') {
                i += 1;
                if format.get(i) == Some(&b'*') {
                    i += 1;
                    let precision =
                        argument_number(format, &mut i)?.unwrap_or_else(&mut next_argument);
                    pending.push((precision, "precision"));
                } else if format.get(i).is_some_and(u8::is_ascii_digit)
                    && self::number(format, &mut i) >= INT_MAX
                {
                    return None;
                }
            }
        }
        if format.get(i) == Some(&b'l') {
            i += 1;
        }
        let argument = argument.unwrap_or_else(&mut next_argument);
        let specifier = *format.get(i)?;
        if !b"sdueEfFgGcoxXb%".contains(&specifier) && !(hh && b"hH".contains(&specifier)) {
            return None;
        }
        let placeholder = format[percent..=i].to_vec();
        number += 1;
        for (index, kind) in pending.into_iter().chain([(argument, "value")]) {
            uses.push(Use {
                index,
                kind,
                specifier,
                placeholder: placeholder.clone(),
                number,
            });
        }
        i += 1;
    }
    Some(uses)
}

fn argument_number(format: &[u8], i: &mut usize) -> Option<Option<usize>> {
    let end = *i
        + format[*i..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
    if format.get(end) != Some(&b'$') {
        return Some(None);
    }
    let value = number(format, i);
    *i += 1;
    (value > 0 && value < INT_MAX).then(|| Some(value as usize - 1))
}

fn number(format: &[u8], i: &mut usize) -> u64 {
    let start = *i;
    while format.get(*i).is_some_and(u8::is_ascii_digit) {
        *i += 1;
    }
    let digits = &format[start..*i];
    let digits = &digits[digits.iter().take_while(|b| **b == b'0').count()..];
    if digits.len() > 10 {
        return INT_MAX;
    }
    digits.iter().fold(0, |n, d| n * 10 + u64::from(d - b'0'))
}

pub(super) fn scanf_count(mut format: &[u8]) -> Option<usize> {
    const SCAN_MAX_ARGS: usize = 255;
    format = format.split(|b| *b == 0).next().unwrap();
    let mut offset = 0;
    let mut sequential = false;
    let mut positional = false;
    let mut index = 0;
    let mut maximum = 0;
    let mut assigned = HashSet::new();
    while offset < format.len() {
        let ch = format[offset];
        offset += 1;
        if ch != b'%' {
            continue;
        }
        let mut ch = *format.get(offset)?;
        offset += 1;
        if ch == b'%' {
            continue;
        }
        let suppress = ch == b'*';
        if suppress {
            ch = *format.get(offset)?;
            offset += 1;
        } else {
            let mut end = offset - 1;
            let number = decimal(format, &mut end);
            if end >= offset && format.get(end) == Some(&b'$') {
                positional = true;
                let value = number as i32;
                if sequential || value < 1 || value as usize > SCAN_MAX_ARGS {
                    return None;
                }
                index = value as usize - 1;
                maximum = maximum.max(value as usize);
                offset = end + 1;
                ch = *format.get(offset)?;
                offset += 1;
            } else {
                sequential = true;
                if positional {
                    return None;
                }
            }
        }
        if ch.is_ascii_digit() {
            offset -= 1;
            decimal(format, &mut offset);
            ch = *format.get(offset)?;
            offset += 1;
        }
        if matches!(ch, b'l' | b'L' | b'h') {
            ch = *format.get(offset)?;
            offset += 1;
        }
        if ch == b'[' {
            ch = *format.get(offset)?;
            offset += 1;
            if ch == b'^' {
                ch = *format.get(offset)?;
                offset += 1;
            }
            if ch == b']' {
                ch = *format.get(offset)?;
                offset += 1;
            }
            while ch != b']' {
                ch = *format.get(offset)?;
                offset += 1;
            }
        } else if !b"ndDioxXufeEgsc".contains(&ch) {
            return None;
        }
        if !suppress {
            if !assigned.insert(index) {
                return None;
            }
            index += 1;
        }
    }
    Some(if positional { maximum } else { index })
}

fn decimal(bytes: &[u8], offset: &mut usize) -> u64 {
    let mut value = 0u64;
    while let Some(&byte) = bytes.get(*offset).filter(|b| b.is_ascii_digit()) {
        value = value
            .saturating_mul(10)
            .saturating_add(u64::from(byte - b'0'));
        *offset += 1;
    }
    value
}
