//! The two JSON spellings every committed harness record is written in
//! (docs/dev-harness.md §5 L1), each reproduced byte for byte:
//!
//! - [`Spelling::Js`]: `JSON.stringify(x, null, 2)` — the 034 eval's cards,
//!   summaries, run manifests and pipeline records. The workspace already
//!   owns JS JSON (`humanify_ag_model::js`: object order, `String(number)`,
//!   escapes), so this module calls it rather than copying it.
//! - [`Spelling::Python`]: `json.dumps(x, sort_keys=True, indent=1,
//!   ensure_ascii=True)` — the exp096 library scorer's cards and summaries
//!   ([`py`]: ints and floats kept apart, `repr(float)`, `\uXXXX` escapes).
//!
//! A file's trailing newline is part of its bytes, so a spelling is
//! recorded with it. The round-trip test reads EVERY committed `.json`
//! under the two results trees and requires one spelling to reproduce it.

pub mod prettier;
pub mod py;

use humanify_ag_model::js::{JsValue, stringify_pretty};

/// How a JSON file was written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spelling {
    /// `JSON.stringify(x, null, 2)`.
    Js,
    /// `json.dumps(x, sort_keys=True, indent=1, ensure_ascii=True)`.
    Python,
    /// `JSON.stringify(x)`, compact (one legacy `self-hop.json`).
    JsCompact,
    /// A JS-written record reflowed by the repo's prettier commit hook
    /// ([`prettier`]; legacy only, never written by the harness).
    Prettier,
}

/// A spelling and whether the file ends with one `\n`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Written {
    pub spelling: Spelling,
    pub trailing_newline: bool,
}

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

/// Which spelling reproduces `text` exactly (parse, then write back), JS
/// first; None when neither does. A scalar or an empty container is
/// spelled the same both ways and reports JS.
pub fn spelling_of(text: &str) -> Option<Written> {
    let (body, trailing_newline) = match text.strip_suffix('\n') {
        Some(b) => (b, true),
        None => (text, false),
    };
    let js_value = JsValue::parse(body).ok();
    if js_value
        .as_ref()
        .is_some_and(|v| stringify_pretty(v, 2) == body)
    {
        return Some(Written {
            spelling: Spelling::Js,
            trailing_newline,
        });
    }
    let written = |spelling| {
        Some(Written {
            spelling,
            trailing_newline,
        })
    };
    if let Some(v) = js_value {
        if humanify_ag_model::js::stringify(&v) == body {
            return written(Spelling::JsCompact);
        }
        if prettier::reflow(&v, &prettier::expanded_objects(body)) == text {
            return written(Spelling::Prettier);
        }
    }
    let py = py::PyValue::parse(body)
        .ok()
        .filter(|v| py::dumps(v) == body);
    py.and_then(|_| written(Spelling::Python))
}

#[cfg(test)]
#[path = "json_test.rs"]
mod json_test;
