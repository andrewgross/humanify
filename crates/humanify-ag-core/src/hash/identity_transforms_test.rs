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
    let both = TransformSet::of(&[Transform::WrapperSpelling, Transform::ConciseArrowBody]);
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
