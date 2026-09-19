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

// A tiny re-export so the test can use json! without an unused warning.
#[allow(dead_code)]
fn _json_used() -> serde_json::Value {
    json!({})
}
