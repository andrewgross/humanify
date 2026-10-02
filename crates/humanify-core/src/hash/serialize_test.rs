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

/// exp094 — the wrapper-spelling unification. The walk trees' flip population
/// (census, experiments/094-wrapper-spelling): 29 walked hops, wrapper flips
/// at exactly one (2.1.207→2.1.208) — 17 module wrappers, all call-arguments
/// to ONE receiver (`createModule`) that only CALLS the wrapper, every body
/// clean of own-scope `this`/`arguments`/`new.target`/`generator`/`id`, and
/// ZERO refused cousin pairs anywhere (nothing real is kept apart). The
/// MatchKey families may therefore serialize an arrow and a function
/// expression IDENTICALLY when the flip is provably semantics-preserving;
/// Verbatim (same-release identity) keeps the spellings apart.
mod wrapper_spelling {
    use super::*;

    /// THE red test: the same code spelled two ways — one MatchKey.
    #[test]
    fn matchkey_unifies_safe_wrapper_spelling() {
        assert_eq!(
            matchkey("var a = function (x, y) { return x + y; };"),
            matchkey("var b = (x, y) => { return x + y; };"),
            "a safe arrow/function pair must hash equal (exp094)"
        );
        // The paren-free single-parameter arrow form — the other 207 spelling.
        assert_eq!(
            matchkey("var a = function (x) { return x; };"),
            matchkey("var b = x => { return x; };"),
            "the single-param arrow form unifies too"
        );
        // async is preserved as a field: async pairs with async, not with sync.
        assert_eq!(
            matchkey("var a = async function (x) { await x; };"),
            matchkey("var b = async (x) => { await x; };"),
            "async wrappers pair under the unified spelling"
        );
        assert_ne!(
            matchkey("var a = async function (x) { await x; };"),
            matchkey("var b = (x) => { return x; };"),
            "async must not merge with sync"
        );
        // Nested flips inside the hashed subtree unify as part of the walk.
        assert_eq!(
            matchkey("var a = function (x) { return x.map(function (v) { return v + 1; }); };"),
            matchkey("var b = (x) => { return x.map((v) => { return v + 1; }); };"),
            "nested spelling flips bridge inside the enclosing hash"
        );
        // Rename invariance holds on the unified arm: the mapping's slot
        // ordinals are walk-order, unaffected by the type-token swap.
        assert_eq!(
            matchkey("var a = function (x, y) { return x + y ^ 2; };"),
            matchkey("var c = (p, q) => { return p + q ^ 2; };"),
            "a renamed flip still pairs (naming + spelling changed together)"
        );
    }

    /// The reliability half: flips the rule must REFUSE — a refusal keeps a
    /// REAL semantic difference unmatched, and that negative case is the
    /// proof the unification is reliable rather than merely bridge-shaped.
    #[test]
    fn matchkey_refuses_semantically_loaded_flips() {
        assert_ne!(
            matchkey("var a = function () { return this; };"),
            matchkey("var b = () => { return this; };"),
            "own-scope this: the flip changes binding — must stay apart"
        );
        assert_ne!(
            matchkey("var a = function () { return arguments; };"),
            matchkey("var b = () => { return arguments; };"),
            "own-scope arguments: must stay apart"
        );
        assert_ne!(
            matchkey("var a = function () { return new.target; };"),
            matchkey("var b = () => { return g(1); };"),
            "own-scope new.target keeps the pair apart (an arrow cannot even spell it)"
        );
        // A member access `x.arguments` is a property, not the binding.
        assert_eq!(
            matchkey("var a = function () { return x.arguments; };"),
            matchkey("var b = () => { return x.arguments; };"),
            "member-property arguments does not refuse"
        );
        // Generators have no arrow spelling at all (`yield` in an arrow is a
        // parse error); `generator:true` rides in the bytes.
        assert_ne!(
            matchkey("var a = function* () {};"),
            matchkey("var b = () => {};"),
            "generator must never unify"
        );
        // A named function expression can self-reference and carries an
        // observable fn.name; an arrow can reproduce neither.
        assert_ne!(
            matchkey("var a = function g() { return g; };"),
            matchkey("var b = () => { return b; };"),
            "named id must never unify"
        );
        // A concise arrow body is not re-spellable without restructuring.
        assert_ne!(
            matchkey("var a = function (x) { return x; };"),
            matchkey("var b = x => x;"),
            "concise arrow body must never unify with a block body"
        );
    }

    /// Where `this` (and `arguments`) bind: occurrences behind a nested
    /// CLASSIC function or class shell are bound there and do not refuse;
    /// occurrences behind a nested ARROW still observe the flipped binding.
    #[test]
    fn matchkey_this_binders_do_not_refuse() {
        // Nested classic function: binds its own this.
        assert_eq!(
            matchkey("var a = function () { return function () { return this; }; };"),
            matchkey("var b = () => { return function () { return this; }; };"),
            "this behind a nested classic function does not refuse"
        );
        // Nested arrow: passes the wrapper's this through — flipping the
        // wrapper changes the arrow's binding, so this DOES refuse.
        assert_ne!(
            matchkey("var a = function () { return () => this; };"),
            matchkey("var b = () => { return () => this; };"),
            "this behind a nested arrow still observes the flip"
        );
        // Class methods and field initializers run under the instance's this.
        assert_eq!(
            matchkey("var a = function () { class C { m() { return this.x; } } return C; };"),
            matchkey("var b = () => { class C { m() { return this.x; } } return C; };"),
            "this in a class method does not refuse"
        );
        assert_eq!(
            matchkey("var a = function () { class C { f = this.y; } return C; };"),
            matchkey("var b = () => { class C { f = this.y; } return C; };"),
            "this in a class field initializer does not refuse"
        );
        assert_eq!(
            matchkey("var a = function () { class C { static { this.z = 1; } } return C; };"),
            matchkey("var b = () => { class C { static { this.z = 1; } } return C; };"),
            "this in a static block does not refuse"
        );
        // The extends clause evaluates in the OUTER scope: a flipped wrapper
        // changes what `this.Base` resolves to. (The one DELIBERATE widening
        // over exp037's detector, which treats whole classes as barriers.)
        assert_ne!(
            matchkey("var a = function () { class C extends this.Base {} return C; };"),
            matchkey("var b = () => { class C extends this.Base {} return C; };"),
            "this in an extends clause observes the flip"
        );
        assert_ne!(
            matchkey("var a = function () { var o = { [this.k]: 1 }; return o; };"),
            matchkey("var b = () => { var o = { [this.k]: 1 }; return o; };"),
            "this in a computed object key observes the flip"
        );
    }

    /// Verbatim answers the SAME-RELEASE identity question ("the exact same
    /// declaration"): a spelling difference is a difference, so the
    /// unification is a MatchKey-family rule only.
    #[test]
    fn verbatim_keeps_wrapper_spellings_apart() {
        assert_ne!(
            verbatim("var a = function (x) { return x; };"),
            verbatim("var b = (x) => { return x; };"),
            "Verbatim must keep the two spellings distinct"
        );
    }

    /// The emission itself: a SAFE arrow walks under the FunctionExpression
    /// token; a loaded one keeps its own. (The observable surface of the
    /// refusal — an arrow is the only side whose bytes change, since a safe
    /// function expression's stream was already the unified spelling.)
    #[test]
    fn matchkey_emits_safe_arrows_under_the_function_token() {
        let parts_of = |code: &str| {
            let allocator = Allocator::default();
            let ingest = Ingest::parse_unambiguous(&allocator, code);
            assert!(ingest.errors.is_empty());
            let tables = SymbolTables::build(ingest.semantic());
            canonical_serialize(
                &program_estree_json(ingest.program),
                &tables,
                LiteralPolicy::MatchKey,
            )
            .parts
        };
        assert!(
            parts_of("var b = (x) => { return x; };").contains("FunctionExpression{"),
            "a safe arrow emits the unified token"
        );
        assert!(
            parts_of("var b = () => { return this; };").contains("ArrowFunctionExpression{"),
            "a this-loaded arrow keeps its own token"
        );
        assert!(
            parts_of("var b = x => x;").contains("ArrowFunctionExpression{"),
            "a concise-bodied arrow keeps its own token"
        );
    }
}
