//! `parse_number` is the TS `parseNumber` (number-utils.ts): `parseInt(v, 10)`
//! then a NaN check — NOT a strict integer parse. The harness never passes
//! a malformed number, but a typo resolves the TS way or the two binaries
//! disagree on the run's settings.

use crate::util::parse_number;

#[test]
fn parse_number_is_parse_int_radix_10() {
    assert_eq!(parse_number("7"), Ok(7.0));
    assert_eq!(
        parse_number("12abc"),
        Ok(12.0),
        "stops at the first non-digit"
    );
    assert_eq!(
        parse_number("  \t\n42"),
        Ok(42.0),
        "leading JS whitespace skipped"
    );
    assert_eq!(
        parse_number("\u{FEFF}5"),
        Ok(5.0),
        "U+FEFF is JS whitespace"
    );
    assert_eq!(parse_number("+5"), Ok(5.0));
    assert_eq!(parse_number("-3"), Ok(-3.0));
    assert_eq!(parse_number("1e3"), Ok(1.0), "no exponent in parseInt");
    assert_eq!(parse_number("7.9"), Ok(7.0), "no fraction in parseInt");
    assert_eq!(parse_number("0x10"), Ok(0.0), "radix 10: the x ends it");
    assert!(parse_number("-0").unwrap().is_sign_negative());
    assert_eq!(parse_number("99999999999999999999"), Ok(1e20));
}

#[test]
fn parse_number_rejects_what_parse_int_calls_nan() {
    for bad in ["", "abc", " ", "+", "-", "x1", "\u{85}1"] {
        assert_eq!(
            parse_number(bad),
            Err(format!("Invalid number: {bad}")),
            "{bad:?}"
        );
    }
}
