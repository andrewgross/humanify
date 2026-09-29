//! Canonical-serialization unit tests: the two literal policies' classes.
//!
//! The number tests are exp093's red tests: the number-magnitude buckets
//! (`numeric_magnitude`, the pre-exp093 `Blurred` policy) bridged 0 of
//! 64,061 real matched pairs while their bucket edge manufactured the only
//! known miss (`n * 2 + 50` vs `n * 3 + 100`: 2 and 3 share a bucket, 50
//! and 100 do not) — so MatchKey now keeps numbers EXACT and only the
//! string rules (volatile classes + length markers) stay blurred.

use oxc_allocator::Allocator;

use crate::hash::serialize::{LiteralPolicy, SymbolTables, canonical_serialize};
use crate::ingest::{Ingest, program_estree_json};

fn hash_of(code: &str, policy: LiteralPolicy) -> String {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, code);
    assert!(ingest.errors.is_empty(), "test code must parse: {code}");
    let tables = SymbolTables::build(ingest.semantic());
    canonical_serialize(&program_estree_json(ingest.program), &tables, policy).hash
}

fn matchkey(code: &str) -> String {
    hash_of(code, LiteralPolicy::MatchKey)
}

fn verbatim(code: &str) -> String {
    hash_of(code, LiteralPolicy::Verbatim)
}

#[test]
fn matchkey_numbers_are_exact() {
    // Same magnitude bucket, different value: 2 and 3 both blurred to N=0,
    // 9 and 10 both to N=1 — the classes whose merge hid real distinctions.
    // exact numbers: every distinct value is its own class.
    assert_ne!(
        matchkey("function f(a){ return a * 2; }"),
        matchkey("function f(a){ return a * 3; }"),
        "2 vs 3: same blur bucket, must differ (exp093)"
    );
    assert_ne!(
        matchkey("function f(a){ return a + 9; }"),
        matchkey("function f(a){ return a + 10; }"),
        "9 vs 10: same blur bucket, must differ (exp093)"
    );
    // Fractional values shared the 0 bucket with 0 itself.
    assert_ne!(
        matchkey("function f(a){ return a + 0; }"),
        matchkey("function f(a){ return a + 0.5; }"),
        "0 vs 0.5: same blur bucket, must differ (exp093)"
    );
    // DISTINCT buckets under the blur too — the manufactured-miss classes
    // from the exp092 fixture (`n * 2 + 50` vs `n * 3 + 100`). Still
    // distinct under exact numbers (the values differ); nothing regains
    // this pair except a looser tier.
    assert_ne!(
        matchkey("function f(n){ return n * 2 + 50; }"),
        matchkey("function f(n){ return n * 3 + 100; }"),
        "50 vs 100 must differ under every policy"
    );
    // Control: the same value under consistent renaming hashes equal — the
    // exactness must not break rename invariance.
    assert_eq!(
        matchkey("function f(a){ return a * 2; }"),
        matchkey("function g(b){ return b * 2; }"),
        "renames keep the hash"
    );
}

#[test]
fn matchkey_bigints_are_exact() {
    // The blur era classed EVERY bigint as B=0; a numeric literal class is
    // a numeric literal class — exact under MatchKey too.
    assert_ne!(
        matchkey("function f(a){ return a + 1n; }"),
        matchkey("function f(a){ return a + 2n; }"),
        "1n vs 2n blurred both to B=0; must differ (exp093)"
    );
    assert_eq!(
        matchkey("function f(a){ return a + 1n; }"),
        matchkey("function g(b){ return b + 1n; }"),
        "renames keep the hash"
    );
}

#[test]
fn matchkey_strings_stay_blurred() {
    // The string rules are UNTOUCHED by exp093: same length, different
    // content — one class.
    assert_eq!(
        matchkey("function f(){ return g(\"ab\"); }"),
        matchkey("function f(){ return g(\"cd\"); }"),
        "same-length strings still share the __STR_<len>__ class"
    );
    // Different lengths still split.
    assert_ne!(
        matchkey("function f(){ return g(\"ab\"); }"),
        matchkey("function f(){ return g(\"abc\"); }"),
        "different lengths still split"
    );
    // The volatile classes: semver, ISO-8601, hex digests.
    assert_eq!(
        matchkey("var v = \"2.1.215\";"),
        matchkey("var v = \"2.1.216\";"),
        "semver strings still share the volatile class"
    );
    assert_eq!(
        matchkey("var v = \"2026-09-18T00:00:00Z\";"),
        matchkey("var v = \"2026-09-28T00:00:00Z\";"),
        "ISO-8601 strings still share the volatile class"
    );
    assert_eq!(
        matchkey("var v = \"0123456789abcdef\";"),
        matchkey("var v = \"fedcba9876543210\";"),
        "hex digests still share the volatile class"
    );
    // Template element raw text: still the length marker (a string rule).
    assert_eq!(
        matchkey("var v = `ab${x}`;"),
        matchkey("var v = `cd${x}`;"),
        "template raw text of equal length still shares a class"
    );
    assert_ne!(
        matchkey("var v = `ab${x}`;"),
        matchkey("var v = `abc${x}`;"),
        "template raw text of different length still splits"
    );
}

#[test]
fn verbatim_is_unchanged_by_the_policy_split() {
    // The verbatim consumers (IdentityKey / declaration-body hash, the
    // naming validators, vendor inherit) see the SAME bytes as before:
    // exact numbers, exact strings.
    assert_ne!(
        verbatim("function f(){ return g(\"ab\"); }"),
        verbatim("function f(){ return g(\"cd\"); }"),
        "string content is identity under Verbatim"
    );
    assert_ne!(
        verbatim("function f(a){ return a * 2; }"),
        verbatim("function f(a){ return a * 3; }"),
        "number value is identity under Verbatim"
    );
    assert_eq!(
        verbatim("function f(a){ return a * 2; }"),
        verbatim("function g(b){ return b * 2; }"),
        "renames keep the hash under Verbatim"
    );
    assert_eq!(
        verbatim("function f(a){ return a * 2; }"),
        matchkey("function f(a){ return a * 2; }"),
        "number-exact MatchKey and Verbatim agree on number-free-strings code"
    );
    assert_ne!(
        verbatim("function f(){ return g(\"ab\"); }"),
        matchkey("function f(){ return g(\"ab\"); }"),
        "blurred strings split the policies apart"
    );
}
