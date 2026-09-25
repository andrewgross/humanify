//! Bun's lazy-init helper, identified STRUCTURALLY — TS
//! `identifyBunLazyInit` (src/shared/bun-helpers.ts). The rest of that
//! file has owners already: `identifyBunCjsFactory` →
//! [`crate::modules::identify_bun_cjs_factory`] (pulled forward by WP1.5),
//! `identifyBunRequire` → [`crate::unpack::bun::identify_bun_require`].
//!
//! The TS is two regexes, hand-matched here with their JS semantics
//! (lesson 15; no `regex` crate, and the first pattern has a
//! backreference):
//!
//! 1. `([$\w]+)\s*&&\s*\([$\w]+\s*=\s*\1\(\1\s*=\s*0\)\)` — the LEFTMOST
//!    match (`String.prototype.match` without /g);
//! 2. over `source.slice(0, match.index)`, the binding regex the factory
//!    detection shares — one owner,
//!    [`crate::modules::leftmost_binding_match`].

use crate::detect::js_text::skip_js_space;
use crate::modules::leftmost_binding_match;

/// `[$\w]` (non-unicode `\w` plus `$`).
fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// End of the `[$\w]+` run starting at `at` (== `at` when there is none).
fn ident_run_end(s: &str, at: usize) -> usize {
    let bytes = s.as_bytes();
    let mut j = at;
    while j < bytes.len() && is_ident_byte(bytes[j]) {
        j += 1;
    }
    j
}

/// The regex's tail after `\1\s*&&` has matched with group 1 = `ident`,
/// from byte `at`: `\s*\([$\w]+\s*=\s*\1\(\1\s*=\s*0\)\)`.
fn lazy_tail_matches(s: &str, at: usize, ident: &str) -> bool {
    let mut p = skip_js_space(s, at);
    if !s[p..].starts_with('(') {
        return false;
    }
    p += 1;
    // `[$\w]+\s*=`: the greedy run can only be followed by `\s` or `=` when
    // it is the whole run (a shorter prefix is followed by a word char).
    let run_end = ident_run_end(s, p);
    if run_end == p {
        return false;
    }
    p = skip_js_space(s, run_end);
    if !s[p..].starts_with('=') {
        return false;
    }
    p = skip_js_space(s, p + 1);
    let Some(rest) = s[p..].strip_prefix(ident) else {
        return false;
    };
    let Some(rest) = rest.strip_prefix('(') else {
        return false;
    };
    let Some(rest) = rest.strip_prefix(ident) else {
        return false;
    };
    let q = s.len() - rest.len();
    let q = skip_js_space(s, q);
    if !s[q..].starts_with('=') {
        return false;
    }
    let q = skip_js_space(s, q + 1);
    s[q..].starts_with("0))")
}

/// The leftmost match start of the lazy-init shape and its group 1.
fn leftmost_lazy_match(source: &str) -> Option<(usize, &str)> {
    let bytes = source.as_bytes();
    let mut from = 0;
    while let Some(rel) = source[from..].find("&&") {
        let amp = from + rel;
        from = amp + 1;
        // `\s*` before `&&`, then the `[$\w]+` run that group 1 must END
        // (a match starting inside the run takes a suffix of it; `\1` must
        // then reappear verbatim).
        let mut e = amp;
        while e > 0 {
            let prev = source[..e].chars().next_back().expect("non-empty");
            if !crate::detect::js_text::is_js_space(prev) {
                break;
            }
            e -= prev.len_utf8();
        }
        let mut s = e;
        while s > 0 && is_ident_byte(bytes[s - 1]) {
            s -= 1;
        }
        // Candidate starts in ascending order: the leftmost wins.
        for start in s..e {
            let ident = &source[start..e];
            if lazy_tail_matches(source, amp + 2, ident) {
                return Some((start, ident));
            }
        }
    }
    None
}

/// `identifyBunLazyInit(source)`: the name of the binding whose body holds
/// Bun's `x && (y = x(x = 0))` lazy-init shape, or None.
pub fn identify_bun_lazy_init(source: &str) -> Option<String> {
    let (at, _) = leftmost_lazy_match(source)?;
    leftmost_binding_match(&source[..at]).map(|(_, name)| name)
}

#[cfg(test)]
mod bun_helpers_test;
