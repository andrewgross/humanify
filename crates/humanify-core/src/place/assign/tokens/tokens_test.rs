//! The TS fossil modules' graded shape tokens (test/parity/wp51-tokens.json,
//! from test/parity/wp51-tokens-probe.ts): every module's token list —
//! values AND first-seen order — over a synthetic program covering every
//! node shape the ESTree→babel translation handles.

use crate::place::assign::fossil::{FossilOptions, assign_fossil};
use crate::twins::statement_inventory_with_values;
use serde_json::Value;

const VECTORS: &str = include_str!("../../../../../../test/parity/wp51-tokens.json");

#[test]
fn every_module_token_list_equals_the_ts() {
    let doc: Value = serde_json::from_str(VECTORS).expect("vectors");
    let code = doc["code"].as_str().unwrap();
    let (inv, values) = statement_inventory_with_values(code, "shipped", None).expect("inventory");
    let spans: Vec<(u32, u32)> = inv
        .statements
        .iter()
        .map(|s| (s.span.start, s.span.end))
        .collect();
    let hashes: Vec<String> = inv.statements.iter().map(|s| s.hash.clone()).collect();
    let out =
        assign_fossil(&values, &spans, &hashes, None, FossilOptions::default()).expect("assign");
    let want = doc["modules"].as_array().unwrap();
    assert_eq!(out.fossil_modules.len(), want.len());
    let mut failures = Vec::new();
    for (got, want) in out.fossil_modules.iter().zip(want) {
        let want_tokens: Vec<String> = want["tokens"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_string())
            .collect();
        if got.file != want["file"].as_str().unwrap()
            || got.tokens.as_deref() != Some(want_tokens.as_slice())
        {
            failures.push(format!(
                "{}: ts {} tokens {:?}\n  rust {} {:?}",
                want["file"],
                want_tokens.len(),
                want_tokens,
                got.file,
                got.tokens
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
