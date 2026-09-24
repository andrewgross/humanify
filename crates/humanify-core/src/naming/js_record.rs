//! `record[key]` the way the TS reads a `Record<string, string>` — the one
//! owner of the question for the naming stage.
//!
//! The TS records (`priorNameHints`, `previousAttempt`, `suggestedNames`)
//! are plain object literals, so a key that is not an OWN property falls
//! through to `Object.prototype`: `priorNameHints["toString"]` is the
//! built-in FUNCTION — truthy, `!== id`, and it renders through a template
//! literal as `function toString() { [native code] }`. Probed on the real
//! builders (test/parity/wp42-probe.ts, `protoStrings` and the `*-proto`
//! prompt cases): a minified binding named `toString` with no prior hint is
//! told its prior name was `function toString() { [native code] }`.
//! Unreachable on the four oracle pairs (no identifier in any request is
//! an Object.prototype name — scanned 2026-09-24), reproduced so the
//! builders stay a pure function of the TS's inputs.

use std::borrow::Cow;

use humanify_model::llm::StrMap;

/// `String(Object.prototype[name])` for every own property of
/// `Object.prototype` (Node 24; pinned by wp42-vectors.json `protoStrings`).
pub const OBJECT_PROTOTYPE_STRINGS: &[(&str, &str)] = &[
    (
        "__defineGetter__",
        "function __defineGetter__() { [native code] }",
    ),
    (
        "__defineSetter__",
        "function __defineSetter__() { [native code] }",
    ),
    (
        "__lookupGetter__",
        "function __lookupGetter__() { [native code] }",
    ),
    (
        "__lookupSetter__",
        "function __lookupSetter__() { [native code] }",
    ),
    ("__proto__", "[object Object]"),
    ("constructor", "function Object() { [native code] }"),
    (
        "hasOwnProperty",
        "function hasOwnProperty() { [native code] }",
    ),
    (
        "isPrototypeOf",
        "function isPrototypeOf() { [native code] }",
    ),
    (
        "propertyIsEnumerable",
        "function propertyIsEnumerable() { [native code] }",
    ),
    (
        "toLocaleString",
        "function toLocaleString() { [native code] }",
    ),
    ("toString", "function toString() { [native code] }"),
    ("valueOf", "function valueOf() { [native code] }"),
];

/// The inherited `Object.prototype` member's string form, when `key` names
/// one.
pub fn inherited_string(key: &str) -> Option<&'static str> {
    OBJECT_PROTOTYPE_STRINGS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, v)| *v)
}

/// `record[key]` rendered as the TS template literal would render it:
/// the own value, else the inherited member's string, else None
/// (`undefined`). A present-but-empty own value is `Some("")` — callers
/// that test truthiness check emptiness themselves, as the TS does.
pub fn get<'a>(record: &'a StrMap, key: &str) -> Option<Cow<'a, str>> {
    match record.0.iter().find(|(k, _)| k == key) {
        Some((_, v)) => Some(Cow::Borrowed(v.as_str())),
        None => inherited_string(key).map(Cow::Borrowed),
    }
}

/// `record[key]` is truthy: a non-empty own string, or any inherited
/// member (functions and `Object.prototype` itself are truthy).
pub fn get_truthy<'a>(record: &'a StrMap, key: &str) -> Option<Cow<'a, str>> {
    get(record, key).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod js_record_test;
