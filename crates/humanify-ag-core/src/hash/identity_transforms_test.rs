//! The identity-transform registry: every transform is registered once,
//! a set iterates in registry order, and the rewrites fire only under
//! their conditions.

use std::collections::HashSet;

use serde_json::{Value, json};

use super::identity_transforms::*;

#[test]
fn every_transform_is_registered_once_under_a_unique_name() {
    let names: HashSet<&str> = TRANSFORMS.iter().map(|s| s.name).collect();
    assert_eq!(names.len(), TRANSFORMS.len());
    for spec in &TRANSFORMS {
        assert_eq!(spec.transform.spec().name, spec.name);
        assert!(!spec.walks.is_empty(), "{} names no walk", spec.name);
        assert!(
            !spec.condition.is_empty(),
            "{} states no condition",
            spec.name
        );
    }
}

#[test]
fn a_set_holds_exactly_what_it_was_given() {
    let both = TransformSet::of(&[
        Transform::WrapperSpelling,
        Transform::Parentheses,
        Transform::ConciseArrowBody,
    ]);
    assert!(both.contains(Transform::WrapperSpelling));
    assert!(both.contains(Transform::ConciseArrowBody));
    let one = TransformSet::of(&[Transform::WrapperSpelling]);
    assert!(!one.contains(Transform::ConciseArrowBody));
    assert!(TransformSet::NONE.is_empty());
    // Registry order, whatever order the set was built in.
    let order: Vec<Transform> = both.iter().collect();
    let registry: Vec<Transform> = TRANSFORMS.iter().map(|s| s.transform).collect();
    assert_eq!(order, registry);
}

fn map(v: &Value) -> &serde_json::Map<String, Value> {
    v.as_object().unwrap()
}

#[test]
fn the_empty_set_keeps_every_node() {
    let arrow = json!({"type": "ArrowFunctionExpression", "expression": true,
        "body": {"type": "Identifier", "name": "x"}, "params": []});
    assert_eq!(
        TransformSet::NONE.rewrite("ArrowFunctionExpression", map(&arrow)),
        Rewrite::Keep
    );
}

#[test]
fn the_concise_arrow_rewrite_is_the_braced_return() {
    let concise = json!({"type": "ArrowFunctionExpression", "expression": true, "async": false,
        "params": [], "body": {"type": "ParenthesizedExpression",
            "expression": {"type": "ObjectExpression", "properties": []}}});
    let Rewrite::Node(braced) =
        Transform::ConciseArrowBody.rewrite("ArrowFunctionExpression", map(&concise))
    else {
        panic!("a concise arrow is rewritten");
    };
    assert_eq!(
        braced,
        json!({"type": "ArrowFunctionExpression", "expression": false, "async": false,
            "params": [], "body": {"type": "BlockStatement", "body": [
                {"type": "ReturnStatement",
                 "argument": {"type": "ObjectExpression", "properties": []}}]}})
    );
    // A braced arrow, or any other node, is kept.
    assert_eq!(
        Transform::ConciseArrowBody.rewrite("ArrowFunctionExpression", map(&braced)),
        Rewrite::Keep
    );
}

/// The statement walk applies a transform set through the same `Rewrite`
/// (no form lists the concise arrow there yet: `statement/3` stays, since
/// the eval scorer reads it for its advisory spelling column — a later
/// version is one set entry away).
#[test]
fn the_statement_walk_applies_a_set_it_is_given() {
    use crate::hash::forms::STATEMENT;
    use crate::hash::statement_hash::statement_hash_in;
    use crate::ingest::{Ingest, program_estree_json};
    let stmt = |code: &str| {
        let allocator = oxc_allocator::Allocator::default();
        let ingest = Ingest::parse_unambiguous(&allocator, code);
        program_estree_json(ingest.program)["body"][0].clone()
    };
    let with_concise = crate::hash::forms::CanonicalForm {
        transforms: TransformSet::of(&[Transform::ConciseArrowBody, Transform::WrapperSpelling]),
        ..STATEMENT
    };
    let (a, b) = (
        stmt("var f = (a) => a.b;"),
        stmt("var f = (a) => { return a.b; };"),
    );
    assert_eq!(
        statement_hash_in(&with_concise, &a),
        statement_hash_in(&with_concise, &b)
    );
    assert_ne!(
        statement_hash_in(&STATEMENT, &a),
        statement_hash_in(&STATEMENT, &b)
    );
    let c = stmt("var f = (a) => { return a.c; };");
    // Masked names: `.b` vs `.c` is a property NAME, masked by this walk;
    // a real structural change still differs.
    let d = stmt("var f = (a) => { a.b; };");
    assert_eq!(
        statement_hash_in(&with_concise, &b),
        statement_hash_in(&with_concise, &c)
    );
    assert_ne!(
        statement_hash_in(&with_concise, &a),
        statement_hash_in(&with_concise, &d)
    );
}
