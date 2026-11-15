//! Numbers as report text: the one thousands separator (JS
//! `toLocaleString("en-US")` / Python `f"{n:,}"` for integers).

/// A run of decimal digits with a `,` between every group of three, from
/// the right.
pub fn group_digits(digits: &str) -> String {
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// An integer with thousands separators (`-1,234`).
pub fn thousands(n: i64) -> String {
    let grouped = group_digits(&n.unsigned_abs().to_string());
    if n < 0 {
        format!("-{grouped}")
    } else {
        grouped
    }
}
