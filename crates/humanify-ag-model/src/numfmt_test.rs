//! The one thousands separator.

use crate::numfmt::{group_digits, thousands};

#[test]
fn digits_are_grouped_by_three_from_the_right() {
    assert_eq!(group_digits(""), "");
    assert_eq!(group_digits("7"), "7");
    assert_eq!(group_digits("123"), "123");
    assert_eq!(group_digits("1234"), "1,234");
    assert_eq!(group_digits("1234567"), "1,234,567");
}

#[test]
fn an_integer_prints_with_thousands_separators() {
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(68_179), "68,179");
    assert_eq!(thousands(-556_875), "-556,875");
}
