//! The harness's JSON spellings:
//!
//! - [`text`]: plain serde JSON (two-space indent, a trailing newline): every
//!   record the library scorer writes.
//! - JS: `JSON.stringify(x, null, 2)` — the eval's cards, summaries, run
//!   manifests and pipeline records. The workspace already owns JS JSON
//!   (`humanify_ag_model::js`: object order, `String(number)`, escapes),
//!   so this module calls it rather than copying it.
//! - [`frozen`]: the identity spelling a recorded library sha is taken over.

pub mod frozen;

use humanify_ag_model::js::{JsValue, stringify_pretty};
use serde::Serialize;

/// A record's bytes: serde's pretty JSON and a trailing newline.
pub fn text(value: &impl Serialize) -> String {
    let mut out = serde_json::to_string_pretty(value).expect("a JSON value serializes");
    out.push('\n');
    out
}

/// A JSON file's value; an error names the file.
pub fn read(path: &std::path::Path) -> Result<serde_json::Value, String> {
    let t = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&t).map_err(|e| format!("{}: {e}", path.display()))
}

/// The bytes `value` is written as in the JS spelling.
pub fn js_text(value: &JsValue, trailing_newline: bool) -> String {
    let mut out = stringify_pretty(value, 2);
    if trailing_newline {
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[path = "json_test.rs"]
mod json_test;
