//! The two JSON spellings every committed harness record is written in
//! (docs/dev-harness.md §5 L1), each reproduced byte for byte:
//!
//! - JS: `JSON.stringify(x, null, 2)` — the 034 eval's cards,
//!   summaries, run manifests and pipeline records. The workspace already
//!   owns JS JSON (`humanify_ag_model::js`: object order, `String(number)`,
//!   escapes), so this module calls it rather than copying it.
//! - Python: `json.dumps(x, sort_keys=True, indent=1,
//!   ensure_ascii=True)` — the exp096 library scorer's cards and summaries
//!   ([`py`]: ints and floats kept apart, `repr(float)`, `\uXXXX` escapes).

pub mod py;

use humanify_ag_model::js::{JsValue, stringify_pretty};

/// The bytes `value` is written as in the JS spelling.
pub fn js_text(value: &JsValue, trailing_newline: bool) -> String {
    let mut out = stringify_pretty(value, 2);
    if trailing_newline {
        out.push('\n');
    }
    out
}

/// The bytes `value` is written as in the Python spelling.
pub fn py_text(value: &py::PyValue, trailing_newline: bool) -> String {
    let mut out = py::dumps(value);
    if trailing_newline {
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[path = "json_test.rs"]
mod json_test;
