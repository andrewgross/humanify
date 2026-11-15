//! JavaScript runtime semantics the pipeline still decides or prints
//! with: `String(number)`, `Math.log`, `Math.round`, `toFixed`,
//! `localeCompare`, UTF-16 code-unit order and offsets, the JS whitespace
//! set, numeric-literal values, Node's `path.resolve`, and the array-index
//! key rule of JS object order. Each is pinned by vectors recorded from
//! JavaScript (test/parity/wp41-js-vectors.json, wpb4-vectors.json). JSON
//! itself is written with serde_json.

use std::cmp::Ordering;

/// The numeric value of an array-index key, or None for an ordinary key.
/// An array index is a canonical decimal (no sign, no leading zero unless
/// it is "0") below 2^32 - 1.
pub fn array_index(key: &str) -> Option<u32> {
    let bytes = key.as_bytes();
    if bytes.is_empty() || bytes.len() > 10 || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    if bytes.len() > 1 && bytes[0] == b'0' {
        return None;
    }
    // At most ten digits: the value fits a u64 with no overflow to handle.
    let value = bytes.iter().fold(0u64, |v, b| v * 10 + u64::from(b - b'0'));
    (value < u64::from(u32::MAX)).then_some(value as u32)
}

/// JS `Array.prototype.sort` default order on strings: UTF-16 code units.
pub fn cmp_utf16(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// The ASCII order of `String.prototype.localeCompare` (ICU root/en-US
/// collation, tertiary strength, non-ignorable punctuation), recorded from
/// Node 24 / ICU 78 (test/parity/wpb4-vectors.json `collation`). Letters
/// share a primary weight with their other case (`aA`); everything else is
/// its own primary.
const COLLATION_ASCII: &str = "\t\n\u{b}\u{c}\r _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789aAbBcCdDeEfFgGhHiIjJkKlLmMnNoOpPqQrRsStTuUvVwWxXyYzZ";

/// A character's collation element: `None` when completely ignorable
/// (the C0 controls outside TAB..CR, and DEL), else (primary, tertiary).
fn collation_element(c: char) -> Option<(u32, u32)> {
    if (c as u32) < 0x20 && !('\t'..='\r').contains(&c) || c == '\u{7f}' {
        return None;
    }
    if c.is_ascii() {
        let pos = COLLATION_ASCII
            .find(c)
            .expect("every printable ASCII char is ranked") as u32;
        return Some(if c.is_ascii_alphabetic() {
            // pairs share the lower-case slot; upper case is tertiary 1
            let base = COLLATION_ASCII.find(c.to_ascii_lowercase()).unwrap() as u32;
            (base, u32::from(c.is_ascii_uppercase()))
        } else {
            (pos, 0)
        });
    }
    // Outside ASCII: after every ASCII primary, by code point. NOT the ICU
    // order (accents are secondary differences there) — the gated corpus
    // is ASCII; a non-ASCII env-var name or path is a declared divergence.
    Some((0x1000 + c as u32, 0))
}

/// `a.localeCompare(b)` for the default locale (see [`COLLATION_ASCII`]):
/// all primaries first, then all tertiaries (lower case before upper),
/// then — only when the strings differ by ignorables alone — equal.
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    let keys = |s: &str| -> Vec<(u32, u32)> { s.chars().filter_map(collation_element).collect() };
    let (ka, kb) = (keys(a), keys(b));
    let primary = |k: &[(u32, u32)]| k.iter().map(|e| e.0).collect::<Vec<_>>();
    let tertiary = |k: &[(u32, u32)]| k.iter().map(|e| e.1).collect::<Vec<_>>();
    primary(&ka)
        .cmp(&primary(&kb))
        .then_with(|| tertiary(&ka).cmp(&tertiary(&kb)))
}

/// `Number.prototype.toString()` (ECMA-262 Number::toString, radix 10),
/// NaN and the infinities included: `dragonbox_ecma`, whose shortest digits
/// break an exact tie to the EVEN digit as the spec (and V8) do — Rust's
/// `{:e}` picks the upper one (`1658206780088562.25` → "…62.2", not
/// "…62.3"; finding #47). Checked against V8's `String(x)` on 3.5 M
/// doubles, ties included: 0 differences.
pub fn number_to_string(x: f64) -> String {
    if x == 0.0 {
        return "0".to_string(); // +0 and -0 alike
    }
    dragonbox_ecma::Buffer::new().format(x).to_string()
}

/// `Math.log(x)` exactly as V8 computes it: fdlibm's `__ieee754_log`
/// (V8 `base::ieee754::log`), which is NOT always correctly rounded — so
/// Rust's `f64::ln` (the platform libm) can differ by an ulp, and a
/// float that feeds a sort or a tie (cluster-assign's IDF weights) must
/// come out bit-identical.
pub fn math_log(x: f64) -> f64 {
    // fdlibm's constants, by their documented bit patterns (e_log.c).
    const LN2_HI: f64 = f64::from_bits(0x3fe6_2e42_fee0_0000);
    const LN2_LO: f64 = f64::from_bits(0x3dea_39ef_3579_3c76);
    const TWO54: f64 = f64::from_bits(0x4350_0000_0000_0000);
    const LG1: f64 = f64::from_bits(0x3fe5_5555_5555_5593);
    const LG2: f64 = f64::from_bits(0x3fd9_9999_9997_fa04);
    const LG3: f64 = f64::from_bits(0x3fd2_4924_9422_9359);
    const LG4: f64 = f64::from_bits(0x3fcc_71c5_1d8e_78af);
    const LG5: f64 = f64::from_bits(0x3fc7_4664_96cb_03de);
    const LG6: f64 = f64::from_bits(0x3fc3_9a09_d078_c69f);
    const LG7: f64 = f64::from_bits(0x3fc2_f112_df3e_5244);
    // `0.33333333333333333` in the C source — the double nearest 1/3.
    const THIRD: f64 = f64::from_bits(0x3fd5_5555_5555_5555);
    let hi = |v: f64| (v.to_bits() >> 32) as i32;
    let lo = |v: f64| v.to_bits() as u32;
    let with_hi = |v: f64, h: i32| f64::from_bits((u64::from(h as u32) << 32) | u64::from(lo(v)));

    let mut x = x;
    let mut hx = hi(x);
    let lx = lo(x);
    let mut k: i32 = 0;
    if hx < 0x0010_0000 {
        if ((hx & 0x7fff_ffff) as u32 | lx) == 0 {
            return f64::NEG_INFINITY;
        }
        if hx < 0 {
            return f64::NAN;
        }
        k -= 54;
        x *= TWO54;
        hx = hi(x);
    }
    if hx >= 0x7ff0_0000 {
        return x + x;
    }
    k += (hx >> 20) - 1023;
    hx &= 0x000f_ffff;
    let i = (hx + 0x95f64) & 0x10_0000;
    x = with_hi(x, hx | (i ^ 0x3ff0_0000));
    k += i >> 20;
    let f = x - 1.0;
    let dk = f64::from(k);
    if (0x000f_ffff & (2 + hx)) < 3 {
        if f == 0.0 {
            return if k == 0 {
                0.0
            } else {
                dk * LN2_HI + dk * LN2_LO
            };
        }
        let r = f * f * (0.5 - THIRD * f);
        return if k == 0 {
            f - r
        } else {
            dk * LN2_HI - ((r - dk * LN2_LO) - f)
        };
    }
    let s = f / (2.0 + f);
    let z = s * s;
    let mut i = hx - 0x6147a;
    let w = z * z;
    let j = 0x6b851 - hx;
    let t1 = w * (LG2 + w * (LG4 + w * LG6));
    let t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)));
    i |= j;
    let r = t2 + t1;
    if i > 0 {
        let hfsq = 0.5 * f * f;
        if k == 0 {
            f - (hfsq - s * (hfsq + r))
        } else {
            dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)
        }
    } else if k == 0 {
        f - s * (f - r)
    } else {
        dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
    }
}

/// The value of a JS numeric literal from its SOURCE spelling (`raw`) —
/// what Babel's `NumericLiteral.value` holds. Read the raw text, never a
/// JSON-parsed `value`: serde_json's default float parser is up to 1 ulp
/// off (lesson 8; the 2.1.216 FLT_MAX literal `340282346638528860000…`
/// parsed one ulp high). Handles `_` separators, hex/octal/binary radix
/// prefixes, and sloppy-mode legacy octal (`017` = 15, `019` = 19).
/// `None` for a BigInt (`…n`) or anything that is not a numeric literal.
// default: a spelling the radix or float parser refuses is "not a numeric
// literal", this function's documented None (one gap: a radix literal
// above u128::MAX also reads None where JS has a float)
#[allow(clippy::disallowed_methods)]
pub fn numeric_literal_value(raw: &str) -> Option<f64> {
    let s: String = raw.chars().filter(|c| *c != '_').collect();
    if s.ends_with('n') {
        return None;
    }
    let radix = |digits: &str, r: u32| u128::from_str_radix(digits, r).ok().map(|v| v as f64);
    let lower = s.to_ascii_lowercase();
    if let Some(d) = lower.strip_prefix("0x") {
        return radix(d, 16);
    }
    if let Some(d) = lower.strip_prefix("0o") {
        return radix(d, 8);
    }
    if let Some(d) = lower.strip_prefix("0b") {
        return radix(d, 2);
    }
    let legacy = s.len() > 1 && s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit());
    if legacy && s.bytes().all(|b| (b'0'..=b'7').contains(&b)) {
        return radix(&s, 8);
    }
    s.parse::<f64>().ok()
}

/// `Number.prototype.toFixed(digits)` for finite |x| < 1e21: the integer n
/// minimizing |n / 10^f - x| over the EXACT binary value of x, the larger n
/// on a tie (half-up for positives) — Rust's `{:.N}` ties to even, which
/// is why this exists (1.25.toFixed(1) is "1.3" in JS, "1.2" in Rust).
pub fn to_fixed(x: f64, digits: usize) -> String {
    if x < 0.0 {
        let inner = to_fixed(-x, digits);
        return if inner.chars().all(|c| c == '0' || c == '.') {
            inner // (-0.001).toFixed(1) is "-0.0" in JS; not reachable here
        } else {
            format!("-{inner}")
        };
    }
    // The exact decimal expansion of a double terminates within 1074
    // fractional digits; this prints it exactly (no rounding happens).
    let exact = format!("{x:.1100}");
    let (int_part, frac) = exact.split_once('.').expect("fixed has a point");
    let kept = &frac[..digits];
    let rest = &frac[digits..];
    let round_up = rest.as_bytes().first().is_some_and(|d| *d >= b'5');
    let mut number: Vec<u8> = format!("{int_part}{kept}").into_bytes();
    if round_up {
        let mut i = number.len();
        loop {
            if i == 0 {
                number.insert(0, b'1');
                break;
            }
            i -= 1;
            if number[i] == b'9' {
                number[i] = b'0';
            } else {
                number[i] += 1;
                break;
            }
        }
    }
    let text = String::from_utf8(number).expect("ascii digits");
    if digits == 0 {
        return text;
    }
    let split = text.len() - digits;
    format!("{}.{}", &text[..split], &text[split..])
}

/// `formatDuration` (from src/llm/metrics.ts): 500ms / 5.0s / 2m 5s /
/// 2h 5m. The one owner: the LLM metrics and the profile summary both
/// print through it. Seconds are rounded and hours-tier minutes truncated,
/// as in the TS, except that a rounding which reaches the next unit now
/// carries into it: the TS printed 59,999 ms as "60.0s" and 3,599,999 ms
/// as "59m 60s" (finding #11, fixed: "1m 0s" and "1h 0m").
pub fn format_duration(ms: f64) -> String {
    if ms < 1000.0 {
        return format!("{}ms", number_to_string(ms));
    }
    if ms < 60_000.0 {
        let seconds = to_fixed(ms / 1000.0, 1);
        if seconds != "60.0" {
            return format!("{seconds}s");
        }
    }
    let mut mins = (ms / 60_000.0).floor();
    if mins < 60.0 {
        let mut secs = math_round((ms % 60_000.0) / 1000.0);
        if secs == 60.0 {
            mins += 1.0;
            secs = 0.0;
        }
        if mins < 60.0 {
            return format!("{}m {}s", number_to_string(mins), number_to_string(secs));
        }
    }
    let hours = (mins / 60.0).floor();
    format!(
        "{}h {}m",
        number_to_string(hours),
        number_to_string(mins % 60.0)
    )
}

/// JS WhiteSpace + LineTerminator — the set `String.prototype.trim`
/// strips (ECMA-262 §12.2/§12.3: TAB VT FF SP NBSP ZWNBSP, category Zs,
/// LF CR LS PS). NOT `char::is_whitespace`: Unicode White_Space includes
/// U+0085 (JS keeps it) and excludes U+FEFF (JS strips it). The set was
/// verified against every code point of the TS's when ported (WP4.2); the
/// per-code-point fixture went with the retired probe vectors (2026-09-28)
/// and the edge cases are pinned by js_test.
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// Node's `path.resolve(p)` (POSIX) from `cwd`, the working directory the
/// shell read: `p` joined to `cwd` (an absolute `p` replaces it), then `.`
/// and `..` normalized LEXICALLY (not through the filesystem), and
/// before any parent walk (a raw `parent()` walk from `a/x/../b` visits
/// `a/x`). An empty `p` (a bare file's directory) is `cwd`, as in Node.
pub fn node_path_resolve(cwd: &std::path::Path, p: &std::path::Path) -> std::path::PathBuf {
    use std::path::{Component, PathBuf};
    let mut out = PathBuf::from("/");
    for c in cwd.join(p).components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::ParentDir => {
                out.pop();
            }
            _ => {}
        }
    }
    out
}

/// `String.prototype.trim()`.
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_js_whitespace)
}

/// A string's JS `.length`: UTF-16 code units.
pub fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// UTF-8 byte offset → UTF-16 code-unit offset (a JS string index) for
/// every char boundary of one text — the one owner for writers that
/// reproduce a TS artifact's raw offsets from Rust byte spans (the rename
/// ledger, the diagnostics' placement trail, the scope dump). An ASCII
/// text is the identity and allocates nothing.
pub struct Utf16Offsets(Option<Vec<u32>>);

impl Utf16Offsets {
    pub fn new(text: &str) -> Self {
        if text.is_ascii() {
            return Utf16Offsets(None);
        }
        let mut table = vec![0u32; text.len() + 1];
        let mut units = 0u32;
        for (byte, ch) in text.char_indices() {
            table[byte] = units;
            units += ch.len_utf16() as u32;
        }
        table[text.len()] = units;
        Utf16Offsets(Some(table))
    }

    /// The JS index of byte offset `byte` (a char boundary).
    pub fn at(&self, byte: u32) -> u32 {
        match &self.0 {
            None => byte,
            Some(table) => table[byte as usize],
        }
    }
}

/// `s.slice(0, n)`: the first `n` UTF-16 code units. A cut inside a
/// surrogate pair would leave JS a lone high surrogate, which a Rust
/// `String` cannot hold — the prefix then stops before the pair (never
/// observed in the oracle texts; 07 OQ3).
pub fn utf16_prefix(s: &str, n: usize) -> &str {
    let mut units = 0usize;
    for (byte, ch) in s.char_indices() {
        let next = units + ch.len_utf16();
        if next > n {
            return &s[..byte];
        }
        units = next;
    }
    s
}

/// `Math.round(x)`: the closest integer, ties toward +infinity.
pub fn math_round(x: f64) -> f64 {
    let floor = x.floor();
    if x - floor >= 0.5 { floor + 1.0 } else { floor }
}
