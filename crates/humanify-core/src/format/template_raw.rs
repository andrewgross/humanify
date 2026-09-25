//! `t.templateElement({ raw, cooked })`'s validator (@babel/types 7.29.7,
//! `templateElementCookedValidator` over @babel/helper-string-parser's
//! `readStringContents("template", raw, …)`): the builder THROWS
//! "Invalid raw" when the raw text would end the template early (an
//! unescaped `` ` `` or `${`), and otherwise RECOMPUTES `cooked` from the
//! raw text — null when it holds an invalid escape.
//!
//! The beautifier's `.concat` fold builds its template from a string's
//! COOKED value used as raw (finding #42), so both effects reach the
//! output: a string with a backtick or `${` makes the whole stage-6
//! beautify throw (finding #44), and the recomputed cooked value decides
//! whether a later `` `…`.concat("s") `` appends `s` once or twice.

/// The quasi's recomputed `cooked`, or Err("Invalid raw").
pub fn template_element_cooked(raw: &str) -> Result<Option<String>, String> {
    let units: Vec<u16> = raw.encode_utf16().collect();
    let mut out: Vec<u16> = Vec::with_capacity(units.len());
    let mut invalid = false;
    let mut pos = 0usize;
    let mut chunk = 0usize;
    loop {
        if pos >= units.len() {
            // `unterminated` — the only accepted ending.
            out.extend_from_slice(&units[chunk..pos]);
            break;
        }
        let ch = units[pos];
        let at = |i: usize| units.get(i).copied();
        if ch == 0x60 || (ch == 0x24 && at(pos + 1) == Some(0x7B)) {
            return Err("Invalid raw".into());
        }
        if ch == 0x5C {
            out.extend_from_slice(&units[chunk..pos]);
            let (next, cooked) = read_escaped(&units, pos);
            match cooked {
                Some(c) => out.extend(c),
                None => invalid = true,
            }
            pos = next;
            chunk = pos;
        } else if ch == 0x0A || ch == 0x0D {
            out.extend_from_slice(&units[chunk..pos]);
            out.push(0x0A);
            pos += 1;
            if ch == 0x0D && at(pos) == Some(0x0A) {
                pos += 1;
            }
            chunk = pos;
        } else {
            pos += 1;
        }
    }
    Ok(if invalid {
        None
    } else {
        Some(String::from_utf16_lossy(&out))
    })
}

/// `readEscapedChar(input, pos, …, inTemplate = true)`: the position after
/// the escape and its cooked units (None: an invalid escape).
fn read_escaped(u: &[u16], backslash: usize) -> (usize, Option<Vec<u16>>) {
    let mut pos = backslash + 1;
    let Some(&ch) = u.get(pos) else {
        // `\` at the very end: charCodeAt → NaN → the default arm,
        // `String.fromCharCode(NaN)` — a NUL.
        return (pos + 1, Some(vec![0]));
    };
    pos += 1;
    let one = |c: u16| Some(vec![c]);
    match ch {
        0x6E => (pos, one(0x0A)),
        0x72 => (pos, one(0x0D)),
        0x74 => (pos, one(0x09)),
        0x62 => (pos, one(0x08)),
        0x76 => (pos, one(0x0B)),
        0x66 => (pos, one(0x0C)),
        0x78 => match read_hex(u, pos, 2) {
            Some((code, end)) => (end, one(code as u16)),
            None => (backslash + 1, None),
        },
        0x75 => read_code_point(u, pos, backslash),
        0x0D | 0x0A | 0x2028 | 0x2029 => {
            if ch == 0x0D && u.get(pos) == Some(&0x0A) {
                pos += 1;
            }
            (pos, Some(Vec::new()))
        }
        0x38 | 0x39 => (pos, None),
        0x30..=0x37 => {
            let start = pos - 1;
            let mut end = start;
            while end < u.len() && end < start + 3 && (0x30..=0x37).contains(&u[end]) {
                end += 1;
            }
            let mut digits: Vec<u16> = u[start..end].to_vec();
            let mut octal = parse_octal(&digits);
            if octal > 255 {
                digits.pop();
                octal = parse_octal(&digits);
            }
            let pos = start + digits.len();
            let next = u.get(pos).copied();
            if digits != [0x30] || next == Some(0x38) || next == Some(0x39) {
                (pos, None)
            } else {
                (pos, one(octal as u16))
            }
        }
        other => (pos, one(other)),
    }
}

fn parse_octal(d: &[u16]) -> u32 {
    d.iter().fold(0, |acc, &c| acc * 8 + u32::from(c - 0x30))
}

/// `readHexChar(…, len, forceLen = false, throwOnInvalid = false)`:
/// exactly `len` hex digits, or None.
fn read_hex(u: &[u16], pos: usize, len: usize) -> Option<(u32, usize)> {
    let mut total = 0u32;
    for i in 0..len {
        let d = hex_digit(*u.get(pos + i)?)?;
        total = total * 16 + d;
    }
    Some((total, pos + len))
}

fn hex_digit(c: u16) -> Option<u32> {
    match c {
        0x30..=0x39 => Some(u32::from(c - 0x30)),
        0x41..=0x46 => Some(u32::from(c - 0x41 + 10)),
        0x61..=0x66 => Some(u32::from(c - 0x61 + 10)),
        _ => None,
    }
}

/// `\u` escapes: `\uHHHH` or `\u{H…}` (at most U+10FFFF).
fn read_code_point(u: &[u16], pos: usize, backslash: usize) -> (usize, Option<Vec<u16>>) {
    if u.get(pos) != Some(&0x7B) {
        return match read_hex(u, pos, 4) {
            Some((code, end)) => (end, Some(vec![code as u16])),
            None => (backslash + 1, None),
        };
    }
    let start = pos + 1;
    let close = u[start..]
        .iter()
        .position(|&c| c == 0x7D)
        .map(|i| start + i);
    let Some(close) = close else {
        return (backslash + 1, None);
    };
    let digits = &u[start..close];
    if digits.is_empty() {
        return (backslash + 1, None);
    }
    let mut code: u64 = 0;
    for &d in digits {
        match hex_digit(d) {
            Some(v) => code = (code * 16 + u64::from(v)).min(u64::from(u32::MAX)),
            None => return (backslash + 1, None),
        }
    }
    let end = close + 1;
    if code > 0x10_FFFF {
        return (end, None);
    }
    let ch = char::from_u32(code as u32);
    match ch {
        Some(c) => {
            let mut buf = [0u16; 2];
            (end, Some(c.encode_utf16(&mut buf).to_vec()))
        }
        // A lone surrogate code point: its one unit.
        None => (end, Some(vec![code as u16])),
    }
}

#[cfg(test)]
mod tests {
    use super::template_element_cooked;

    #[test]
    fn cooks_and_rejects_like_the_babel_validator() {
        assert_eq!(template_element_cooked("abc"), Ok(Some("abc".into())));
        assert_eq!(template_element_cooked(""), Ok(Some(String::new())));
        assert_eq!(template_element_cooked("a\\nb"), Ok(Some("a\nb".into())));
        assert_eq!(template_element_cooked("a\\x41"), Ok(Some("aA".into())));
        assert_eq!(
            template_element_cooked("\\u{1F600}"),
            Ok(Some("\u{1F600}".into()))
        );
        assert_eq!(template_element_cooked("\\x4"), Ok(None));
        assert_eq!(template_element_cooked("\\01"), Ok(None));
        assert_eq!(template_element_cooked("\\0"), Ok(Some("\u{0}".into())));
        assert_eq!(template_element_cooked("\\\n"), Ok(Some(String::new())));
        assert_eq!(template_element_cooked("a\r\nb"), Ok(Some("a\nb".into())));
        assert!(template_element_cooked("a`b").is_err());
        assert!(template_element_cooked("a${b").is_err());
        assert_eq!(template_element_cooked("a$b\\`"), Ok(Some("a$b`".into())));
    }
}
