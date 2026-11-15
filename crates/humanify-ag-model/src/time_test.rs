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

#[test]
fn a_clock_reading_formats_as_its_unix_milliseconds() {
    use std::time::{Duration, UNIX_EPOCH};
    let at = UNIX_EPOCH + Duration::from_millis(1_709_210_096_789);
    assert_eq!(
        crate::time::iso_from_system_time(at),
        iso_from_unix_ms(1_709_210_096_789)
    );
}

#[test]
fn a_clock_reading_before_1970_formats_as_js_does() {
    // `new Date(-1).toISOString()` and `new Date(-86400001)`.
    use std::time::{Duration, UNIX_EPOCH};
    let iso = |ms| crate::time::iso_from_system_time(UNIX_EPOCH - Duration::from_millis(ms));
    assert_eq!(iso(1), "1969-12-31T23:59:59.999Z");
    assert_eq!(iso(86_400_001), "1969-12-30T23:59:59.999Z");
    let half = UNIX_EPOCH - Duration::from_micros(500);
    assert_eq!(
        crate::time::iso_from_system_time(half),
        "1969-12-31T23:59:59.999Z"
    );
}
