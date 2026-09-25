//! `record[key]` for the naming stage's `Record<string, string>` inputs
//! (`priorNameHints`, `previousAttempt`, `suggestedNames`) — the one owner
//! of the question for the naming stage: the OWN value, never an inherited
//! one.
//!
//! The TS used to read these with a bare `record[key]`, which falls through
//! to `Object.prototype`: a binding named `toString` with no hint of its own
//! was told its prior name was `function toString() { [native code] }`, and
//! a module identifier named `hasOwnProperty` crashed the prompt build.
//! Fixed TS-first (`ownEntry`, src/shared/own-entry.ts; 16-findings-queue
//! #12, 2026-09-25); this module used to reproduce the fall-through.

use std::borrow::Cow;

use humanify_model::llm::StrMap;

/// `ownEntry(record, key)`: the own value, else None (`undefined`). A
/// present-but-empty own value is `Some("")` — callers that test
/// truthiness check emptiness themselves, as the TS does.
pub fn get<'a>(record: &'a StrMap, key: &str) -> Option<Cow<'a, str>> {
    record
        .0
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| Cow::Borrowed(v.as_str()))
}

/// `ownEntry(record, key)` is truthy: a non-empty own string.
pub fn get_truthy<'a>(record: &'a StrMap, key: &str) -> Option<Cow<'a, str>> {
    get(record, key).filter(|v| !v.is_empty())
}

#[cfg(test)]
mod js_record_test;
