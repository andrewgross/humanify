//! The wall clock as UTC text: the one owner of the civil-date arithmetic.
//! Wall time is recorded, never a decision input.

/// Days since 1970-01-01 to a proleptic Gregorian `(year, month, day)`
/// (Howard Hinnant's `civil_from_days`).
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// A Unix-epoch millisecond count as `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn iso_from_unix_ms(ms: u128) -> String {
    let secs = i64::try_from(ms / 1000).expect("a time before year 292 billion");
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60,
        ms % 1000
    )
}

/// Now, as [`iso_from_unix_ms`] (JS `new Date().toISOString()`).
pub fn iso_now() -> String {
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    iso_from_unix_ms(since.as_millis())
}
