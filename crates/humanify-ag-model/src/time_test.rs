//! The one UTC clock formatter.

use crate::time::{civil_from_days, iso_from_unix_ms};

#[test]
fn a_unix_millisecond_count_formats_as_iso_utc() {
    assert_eq!(iso_from_unix_ms(0), "1970-01-01T00:00:00.000Z");
    assert_eq!(
        iso_from_unix_ms(1_704_067_200_000),
        "2024-01-01T00:00:00.000Z"
    );
    assert_eq!(
        iso_from_unix_ms(1_709_210_096_789),
        "2024-02-29T12:34:56.789Z"
    );
    assert_eq!(
        iso_from_unix_ms(4_102_444_799_999),
        "2099-12-31T23:59:59.999Z"
    );
}

#[test]
fn days_since_the_epoch_are_a_civil_date() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(19_782), (2024, 2, 29));
    assert_eq!(civil_from_days(-1), (1969, 12, 31));
}
