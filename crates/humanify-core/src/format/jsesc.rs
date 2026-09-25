//! `jsesc(value, { quotes: "double", wrap: true, minimal: false })` —
//! jsesc 3.0.2 with the generator's `jsescOption`, the only way a string
//! WITHOUT raw source text is printed (a synthesized StringLiteral: the
//! beautifier's `"a".concat("b")` fold, the `using` desugar's names).
//!
//! Escaped: `"` (the quote), `\`, the single-escape controls (`\b \f \n
//! \r \t`), `\0` when no digit follows, and every other code unit outside
//! printable ASCII — `\xHH` below U+0100, `\uHHHH` above, UPPERCASE hex,
//! an astral char as its surrogate pair. `'` and `` ` `` pass through.

/// The quoted, escaped literal.
pub fn jsesc_double(value: &str) -> String {
    let units: Vec<u16> = value.encode_utf16().collect();
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for (i, &u) in units.iter().enumerate() {
        match u {
            // [ !#-&(-[\]-_a-~]: printable ASCII except " ' \ `
            0x20 | 0x21 | 0x23..=0x26 | 0x28..=0x5B | 0x5D..=0x5F | 0x61..=0x7E => {
                out.push(u as u8 as char);
            }
            0x22 => out.push_str("\\\""),
            0x27 | 0x60 => out.push(u as u8 as char),
            0x5C => out.push_str("\\\\"),
            0x08 => out.push_str("\\b"),
            0x0C => out.push_str("\\f"),
            0x0A => out.push_str("\\n"),
            0x0D => out.push_str("\\r"),
            0x09 => out.push_str("\\t"),
            0x00 if !units.get(i + 1).is_some_and(|n| (0x30..=0x39).contains(n)) => {
                out.push_str("\\0");
            }
            _ if u < 0x100 => out.push_str(&format!("\\x{u:02X}")),
            _ => out.push_str(&format!("\\u{u:04X}")),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::jsesc_double;

    #[test]
    fn escapes_like_jsesc_with_the_generator_options() {
        // Each expectation is jsesc 3.0.2's output (node -e, the generator's
        // jsescOption).
        assert_eq!(jsesc_double("ab"), r#""ab""#);
        assert_eq!(jsesc_double("a\"b'c`d"), r#""a\"b'c`d""#);
        assert_eq!(jsesc_double("\\"), r#""\\""#);
        assert_eq!(jsesc_double("\n\r\t\u{8}\u{c}\u{b}"), r#""\n\r\t\b\f\x0B""#);
        assert_eq!(jsesc_double("\u{0}a\u{0}1"), r#""\0a\x001""#);
        assert_eq!(jsesc_double("\u{7f}\u{e9}"), r#""\x7F\xE9""#);
        assert_eq!(
            jsesc_double("\u{2028}\u{ff}\u{100}"),
            r#""\u2028\xFF\u0100""#
        );
        assert_eq!(jsesc_double("\u{1f600}"), r#""\uD83D\uDE00""#);
    }
}
