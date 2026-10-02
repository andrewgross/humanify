//! Statement-hash unit tests (the 06 §4 property classes at statement
//! scale): rename-invariance with content controls, the literal classes,
//! optional chains, holes.

use serde_json::json;

use crate::hash::statement_hash::statement_hash;

fn parse_to_estree(code: &str) -> serde_json::Value {
    // A tiny in-test parse through the same substrate the dump uses.
    let allocator = oxc_allocator::Allocator::default();
    let source_type = oxc_span::SourceType::from_path("x.js")
        .unwrap_or_default()
        .with_script(true);
    let ret = oxc_parser::Parser::new(&allocator, code, source_type).parse();
    assert!(
        ret.diagnostics.is_empty(),
        "test code must parse: {:?}",
        ret.diagnostics
    );
    let program = allocator.alloc(ret.program);
    let estree = program.to_estree_json(false, true);
    let mut de = serde_json::Deserializer::from_str(&estree);
    de.disable_recursion_limit();
    serde::Deserialize::deserialize(&mut de).unwrap()
}

fn wrapper_statements(program: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut expr = program["body"][0]["expression"].clone();
    while expr["type"] == "ParenthesizedExpression" {
        expr = expr["expression"].clone();
    }
    let mut callee = expr["callee"].clone();
    while callee["type"] == "ParenthesizedExpression" {
        callee = callee["expression"].clone();
    }
    callee["body"]["body"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

#[test]
fn statement_hash_is_rename_invariant_with_content_controls() {
    // Renaming BOUND identifiers must not move the hash (the names are
    // masked); changing a property key or a free identifier MUST (they are
    // hash content).
    let a = wrapper_statements(&parse_to_estree(
        "(function(){ var aa = obj.item; use(aa); })()",
    ));
    let b = wrapper_statements(&parse_to_estree(
        "(function(){ var zz = obj.item; use(zz); })()",
    ));
    assert_eq!(
        statement_hash(&a[0]),
        statement_hash(&b[0]),
        "renames don't move the hash"
    );
    assert_eq!(
        statement_hash(&a[1]),
        statement_hash(&b[1]),
        "renames don't move the hash (use site)"
    );

    // CONTROL: a literal/shape change DOES move it (the statement hash
    // masks ALL identifier names — property keys included, its design —
    // so the control must be a value-bearing class, not a name).
    let c = wrapper_statements(&parse_to_estree(
        "(function(){ var aa = obj.item; use(aa, 7); })()",
    ));
    assert_ne!(
        statement_hash(&a[1]),
        statement_hash(&c[1]),
        "literals are content — the control proves the mask isn't total"
    );
    // And the property-key rename really is masked (the statement hash's
    // own rule, distinct from the structural hash's):
    let d = wrapper_statements(&parse_to_estree(
        "(function(){ var aa = obj.item2; use(aa); })()",
    ));
    assert_eq!(
        statement_hash(&a[0]),
        statement_hash(&d[0]),
        "property identifiers are masked in the statement hash"
    );
}

#[test]
fn statement_hash_distinguishes_the_literal_classes() {
    // String "1" vs number 1: babel distinguishes by TYPE NAME; the Rust
    // content must too (the collision this suite caught).
    let s = wrapper_statements(&parse_to_estree("(function(){ var v = \"1\"; })()"));
    let n = wrapper_statements(&parse_to_estree("(function(){ var v = 1; })()"));
    assert_ne!(
        statement_hash(&s[0]),
        statement_hash(&n[0]),
        "string \"1\" vs number 1"
    );

    // Empty string vs null: both ESTree-Literal with distinct classes.
    let e = wrapper_statements(&parse_to_estree("(function(){ var v = \"\"; })()"));
    let nu = wrapper_statements(&parse_to_estree("(function(){ var v = null; })()"));
    assert_ne!(
        statement_hash(&e[0]),
        statement_hash(&nu[0]),
        "empty string vs null"
    );

    // Bigint: {value: null, raw: "0n", bigint: "0"} must not class with null.
    let bi = wrapper_statements(&parse_to_estree("(function(){ var v = 0n; })()"));
    assert_ne!(
        statement_hash(&bi[0]),
        statement_hash(&nu[0]),
        "bigint vs null"
    );
}

#[test]
fn statement_hash_distinguishes_optional_chain_shapes() {
    // `X.cache.clear?.()` vs `X.cache?.clear?.()` differ in the INNER
    // member's optionality — babel as a type name, oxc as a scalar.
    let a = wrapper_statements(&parse_to_estree("(function(){ X.cache.clear?.(); })()"));
    let b = wrapper_statements(&parse_to_estree("(function(){ X.cache?.clear?.(); })()"));
    assert_ne!(
        statement_hash(&a[0]),
        statement_hash(&b[0]),
        "the inner member's optionality is content"
    );
}

#[test]
fn statement_hash_marks_array_holes() {
    // [1, , 2] must not alias the hole-free spelling.
    let holes = wrapper_statements(&parse_to_estree("(function(){ var v = [1, , 2]; })()"));
    let solid = wrapper_statements(&parse_to_estree(
        "(function(){ var v = [1, undefined, 2]; })()",
    ));
    assert_ne!(
        statement_hash(&holes[0]),
        statement_hash(&solid[0]),
        "a hole cannot alias the undefined spelling"
    );
}

#[test]
fn statement_hash_is_deterministic() {
    let code = "(function(){ var v = [1, {a: 2}, f(x)]; })()";
    let stmts = wrapper_statements(&parse_to_estree(code));
    assert_eq!(statement_hash(&stmts[0]), statement_hash(&stmts[0]));
}

// ---------------------------------------------------------------------------
// exp094b — the wrapper-spelling unification mirrored into the statement
// hash. exp094 (the MatchKey arm) proved the flip population clean at the
// function level (experiments/094-wrapper-spelling/out-fn: 29 hops, 17
// flips at exactly 2.1.207→2.1.208, all safe, ZERO refused pairs). The
// statement arm must answer the SAME question with the SAME rule — the
// shared predicate lives in `hash::wrapper_spelling`, and a SAFE arrow's
// node line walks under the FunctionExpression token so the split
// inheritance / statement twins / family permute (every STATEMENT_HASH
// consumer) stop splitting classes on a bundler's wrapper re-serialization
// (fixture: statement-twin recall 2/3 → 3/3).
// ---------------------------------------------------------------------------
mod wrapper_spelling {
    use super::*;

    /// Hash one top-level statement of `code`.
    fn hash_of(code: &str) -> String {
        let program = parse_to_estree(code);
        statement_hash(&program["body"][0])
    }

    /// THE red test: the same statement twice-spelled — one hash.
    #[test]
    fn statement_hash_unifies_safe_wrapper_spelling() {
        assert_eq!(
            hash_of("var a = function (x, y) { return x + y; };"),
            hash_of("var b = (x, y) => { return x + y; };"),
            "a safe arrow/function pair must hash equal (exp094b)"
        );
        // The paren-free single-parameter arrow form.
        assert_eq!(
            hash_of("var a = function (x) { return x; };"),
            hash_of("var b = x => { return x; };"),
            "the single-param arrow form unifies too"
        );
        // async is a head FIELD of both spellings: pairs with async, not
        // with sync.
        assert_eq!(
            hash_of("var a = async function (x) { await x; };"),
            hash_of("var b = async (x) => { await x; };"),
            "async wrappers pair under the unified spelling"
        );
        assert_ne!(
            hash_of("var a = async function (x) { await x; };"),
            hash_of("var b = (x) => { return x; };"),
            "async must not merge with sync"
        );
        // Nested flips inside the statement unify as part of the walk.
        assert_eq!(
            hash_of("var a = function (x) { return x.map(function (v) { return v + 1; }); };"),
            hash_of("var b = (x) => { return x.map((v) => { return v + 1; }); };"),
            "nested spelling flips bridge inside the enclosing statement"
        );
        // Renaming plus the flip together still pairs (names are masked).
        assert_eq!(
            hash_of("var a = function (x, y) { return x + y ^ 2; };"),
            hash_of("var c = (p, q) => { return p + q ^ 2; };"),
            "a renamed flip still pairs"
        );
    }

    /// The reliability half: flips the rule must REFUSE. A refusal keeps a
    /// REAL semantic difference in different classes — the proof the mirror
    /// is the same rule, not merely bridge-shaped.
    #[test]
    fn statement_hash_refuses_semantically_loaded_flips() {
        assert_ne!(
            hash_of("var a = function () { return this; };"),
            hash_of("var b = () => { return this; };"),
            "own-scope this: the flip changes binding — must stay apart"
        );
        assert_ne!(
            hash_of("var a = function () { return arguments; };"),
            hash_of("var b = () => { return arguments; };"),
            "own-scope arguments: must stay apart"
        );
        assert_ne!(
            hash_of("var a = function () { return new.target; };"),
            hash_of("var b = () => { return g(1); };"),
            "own-scope new.target keeps the pair apart (an arrow cannot spell it)"
        );
        // A member access `x.arguments` is a property, not the binding.
        assert_eq!(
            hash_of("var a = function () { return x.arguments; };"),
            hash_of("var b = () => { return x.arguments; };"),
            "member-property arguments does not refuse"
        );
        // A generator has no arrow spelling: `function*` carries the head
        // field the statement stream must see (v2 could not — the stream
        // carries no scalars — which is why the head fields ARE the node
        // line's content now).
        assert_ne!(
            hash_of("var a = function* () { yield 1; };"),
            hash_of("var b = () => { return 1; };"),
            "generator must never unify with an arrow"
        );
        // A named function expression carries an observable `id` (an extra
        // child node in this stream); an arrow has none.
        assert_ne!(
            hash_of("var a = function g() { return g; };"),
            hash_of("var b = () => { return b; };"),
            "named id must never unify"
        );
        // A concise arrow body is not re-spellable without restructuring.
        assert_ne!(
            hash_of("var a = function (x) { return x; };"),
            hash_of("var b = x => x;"),
            "concise arrow body must never unify with a block body"
        );
    }

    /// The two head fields the unified spelling must carry: `async` and
    /// `generator`. The MatchKey arm gets them for free (it serializes
    /// every scalar); this stream hashes only node types + node content, so
    /// the fields ride as the function node's CONTENT — and that fixes a
    /// pre-existing coarseness the flip would otherwise have amplified:
    /// v2 hashed `function () {}`, `async function () {}` and
    /// `function* () {}` as ONE class.
    #[test]
    fn statement_hash_carries_the_function_head_fields() {
        let plain = "var a = function () { return 1; };";
        assert_ne!(
            hash_of(plain),
            hash_of("var a = async function () { return 1; };"),
            "async is content, not trivia"
        );
        assert_ne!(
            hash_of(plain),
            hash_of("var a = function* () { yield 1; };"),
            "generator is content, not trivia"
        );
        assert_ne!(
            hash_of("var a = function f() { return 1; };"),
            hash_of(plain),
            "a binding id is an extra node — content"
        );
        // Same for declarations and for arrows that keep their own token.
        assert_ne!(
            hash_of("function f() { return 1; }"),
            hash_of("async function f() { return 1; }"),
            "declaration head fields are content too"
        );
        assert_ne!(
            hash_of("var a = () => { return 1; };"),
            hash_of("var a = async () => { return 1; };"),
            "arrow head fields are content too"
        );
    }

    /// Where `this` (and `arguments`) bind: occurrences behind a nested
    /// CLASSIC function or class shell are bound there and do not refuse;
    /// occurrences behind a nested ARROW still observe the flip; a class's
    /// extends clause and computed keys evaluate OUTER and observe it.
    #[test]
    fn statement_hash_this_binders_do_not_refuse() {
        assert_eq!(
            hash_of("var a = function () { return function () { return this; }; };"),
            hash_of("var b = () => { return function () { return this; }; };"),
            "this behind a nested classic function does not refuse"
        );
        assert_ne!(
            hash_of("var a = function () { return () => this; };"),
            hash_of("var b = () => { return () => this; };"),
            "this behind a nested arrow still observes the flip"
        );
        assert_eq!(
            hash_of("var a = function () { class C { m() { return this.x; } } return C; };"),
            hash_of("var b = () => { class C { m() { return this.x; } } return C; };"),
            "this in a class method does not refuse"
        );
        assert_eq!(
            hash_of("var a = function () { class C { f = this.y; } return C; };"),
            hash_of("var b = () => { class C { f = this.y; } return C; };"),
            "this in a class field initializer does not refuse"
        );
        assert_eq!(
            hash_of("var a = function () { class C { static { this.z = 1; } } return C; };"),
            hash_of("var b = () => { class C { static { this.z = 1; } } return C; };"),
            "this in a static block does not refuse"
        );
        assert_ne!(
            hash_of("var a = function () { class C extends this.Base {} return C; };"),
            hash_of("var b = () => { class C extends this.Base {} return C; };"),
            "this in an extends clause observes the flip"
        );
        assert_ne!(
            hash_of("var a = function () { var o = { [this.k]: 1 }; return o; };"),
            hash_of("var b = () => { var o = { [this.k]: 1 }; return o; };"),
            "this in a computed object key observes the flip"
        );
    }
}

// A tiny re-export so the test can use json! without an unused warning.
#[allow(dead_code)]
fn _json_used() -> serde_json::Value {
    json!({})
}
