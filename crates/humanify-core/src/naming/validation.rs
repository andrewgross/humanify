//! The identifier sanitizer and the conflict ladder — src/llm/validation.ts,
//! the DECORATION_WORDS owner.
//!
//! Name LEGALITY (a syntactic identifier, not a reserved word, not a host
//! global) has one owner, `rename::validated::target` — this module held a
//! second copy of the same two tables (same members, another order) until
//! toolchain review R25 (2026-10-04); it now reads that owner.

use crate::rename::validated::target::{is_global_builtin, is_reserved_word};

/// The decoration words the conflict ladder appends, in ladder order.
/// Single source: the prior-name snap's stem stripper (WP4.5) derives from
/// this list, so every producible decoration is also strippable.
pub const DECORATION_WORDS: &[&str] = &["Val", "Var", "Ref", "Item", "Data", "Result", "Value"];

fn is_ident_part(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$'
}

/// The fallback when validation fails: strip every character outside
/// `[a-zA-Z0-9_$]` (per UTF-16 unit in the TS — an astral character's two
/// surrogates both go, same as removing the char), `_`-prefix a leading
/// digit, `_unnamed` for nothing left, `_`-suffix a reserved word or
/// global builtin.
pub fn sanitize_identifier(name: &str) -> String {
    let mut s: String = name.chars().filter(|c| is_ident_part(*c)).collect();
    if s.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        s.insert(0, '_');
    }
    if s.is_empty() {
        s = "_unnamed".to_string();
    }
    if is_reserved_word(&s) || is_global_builtin(&s) {
        s.push('_');
    }
    s
}

/// Resolves a naming conflict by DECORATING, never by inventing: the
/// decoration words, then `name2..name999`, then `nameVal2, nameVal3, …`
/// (terminates because the used set is finite). Every rung is reducible
/// back to the input by the prior-name snap's stem stripper.
pub fn resolve_conflict(name: &str, is_used: impl Fn(&str) -> bool) -> String {
    for suffix in DECORATION_WORDS {
        let candidate = format!("{name}{suffix}");
        if !is_used(&candidate) {
            return candidate;
        }
    }
    for i in 2..=999 {
        let candidate = format!("{name}{i}");
        if !is_used(&candidate) {
            return candidate;
        }
    }
    (2u64..)
        .map(|i| format!("{name}Val{i}"))
        .find(|c| !is_used(c))
        .expect("the used set is finite")
}

#[cfg(test)]
mod validation_test;
