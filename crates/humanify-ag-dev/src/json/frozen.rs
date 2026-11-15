//! The identity spelling of a library file: the text whose sha256 is a
//! recorded identity (a corpus's `corpusSha256`, every layer id, the sha of
//! every layer and truth file on the data roots). It is frozen: one byte
//! more or less changes every recorded identity, so no new record should be
//! written in it (enforced: frozen_test::recorded_identities_are_unchanged).
//!
//! The spelling: keys sorted by code point; ints as digits, floats as the
//! shortest round-trip digits, positional when the decimal exponent is in
//! -5 < e < 16 (always with a fraction: `100.0`), else `d.ddde±XX` with at
//! least two exponent digits; `"` `\` and `\n \r \t \b \f` get short
//! escapes and every other character outside space..`~` is `\uXXXX` in
//! lowercase hex (a character beyond the BMP as its surrogate pair).
//! [`indented`]: one space per level, `,` at line end, `": "` after a key,
//! an empty list or object as `[]` / `{}`. [`compact`]: no whitespace.

use serde_json::{Number, Value};

/// The indented identity text with a trailing newline: what a corpus
/// manifest's and a layer's identity is the sha256 of.
pub fn identity_text(value: &Value) -> String {
    let mut out = String::new();
    write(&mut out, value, Some(0));
    out.push('\n');
    out
}

/// The compact identity text: a layer or truth file's gzipped bytes.
pub fn compact(value: &Value) -> String {
    let mut out = String::new();
    write(&mut out, value, None);
    out
}

/// A float's digits in the identity spelling.
pub fn float_text(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    if x == 0.0 {
        return format!("{sign}0.0");
    }
    // Rust's `{:e}` with no precision is the shortest round-trip digits.
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').expect("{:e} has an exponent");
    let exp: i32 = exp.parse().expect("an integer exponent");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    if !(-5 < exp && exp < 16) {
        let m = if digits.len() > 1 {
            format!("{}.{}", &digits[..1], &digits[1..])
        } else {
            digits
        };
        let es = if exp < 0 { '-' } else { '+' };
        return format!("{sign}{m}e{es}{:02}", exp.abs());
    }
    if exp >= 0 {
        let point = exp as usize + 1;
        if digits.len() <= point {
            format!("{sign}{digits}{}.0", "0".repeat(point - digits.len()))
        } else {
            format!("{sign}{}.{}", &digits[..point], &digits[point..])
        }
    } else {
        format!("{sign}0.{}{digits}", "0".repeat((-exp - 1) as usize))
    }
}

fn number_text(n: &Number) -> String {
    if n.is_f64() {
        float_text(n.as_f64().expect("an f64 number"))
    } else {
        n.to_string()
    }
}

/// A string in the identity spelling, quotes included.
pub fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            ' '..='~' => out.push(ch),
            _ => {
                let mut buf = [0u16; 2];
                for unit in ch.encode_utf16(&mut buf) {
                    out.push_str(&format!("\\u{unit:04x}"));
                }
            }
        }
    }
    out.push('"');
}

/// Before an item: a newline and the indent when indented.
fn item_break(out: &mut String, depth: Option<usize>) {
    if let Some(d) = depth {
        out.push('\n');
        out.extend(std::iter::repeat_n(' ', d));
    }
}

fn write(out: &mut String, value: &Value, depth: Option<usize>) {
    let inner = depth.map(|d| d + 1);
    let key_sep = if depth.is_some() { ": " } else { ":" };
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&number_text(n)),
        Value::String(s) => write_str(out, s),
        Value::Array(items) if items.is_empty() => out.push_str("[]"),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                item_break(out, inner);
                write(out, item, inner);
            }
            item_break(out, depth);
            out.push(']');
        }
        Value::Object(entries) if entries.is_empty() => out.push_str("{}"),
        Value::Object(entries) => {
            // serde_json's map is a BTreeMap here: already sorted by key.
            out.push('{');
            for (i, (k, v)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                item_break(out, inner);
                write_str(out, k);
                out.push_str(key_sep);
                write(out, v, inner);
            }
            item_break(out, depth);
            out.push('}');
        }
    }
}

#[cfg(test)]
#[path = "frozen_test.rs"]
mod frozen_test;
