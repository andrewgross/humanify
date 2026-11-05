//! `core::format` against the TS beautify's own bytes: the committed
//! goldens (test/parity/format-goldens.json — each case's
//! `transformWithPlugins(code, [])` and `createBabelPlugin()(code)` output,
//! or the error Babel threw, captured by test/parity/format-probe.ts from a
//! frozen tree of the rust-port commit). The goldens are the formatter's
//! frozen spec: the beautifier's own __tests__ inputs, babel.test.ts,
//! finding #42's bugs, the traversal engine's queue/requeue/insert/remove
//! paths, the printer's ESM/class/paren paths, attached comments, and the
//! inputs Babel's module-mode parse rejects.

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::{FormatOptions, Plugins, format};

fn goldens() -> Vec<Value> {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/parity/format-goldens.json");
    serde_json::from_str(&fs::read_to_string(path).expect("goldens")).expect("json")
}

fn check_leg(name: &str, leg: &str, code: &str, want: &Value, plugins: Plugins) -> Option<String> {
    let got = format(
        code,
        &FormatOptions {
            plugins,
            plant: None,
        },
    );
    match (want.get("text").and_then(Value::as_str), got) {
        (Some(w), Ok(g)) if w == g => None,
        (Some(w), Ok(g)) => Some(format!("{name} [{leg}]\n--- ts\n{w}\n--- rust\n{g}\n")),
        (Some(_), Err(e)) => Some(format!("{name} [{leg}]: rust error {e}")),
        (None, Err(_)) => None,
        (None, Ok(g)) => Some(format!(
            "{name} [{leg}]: TS threw ({}), rust printed\n{g}",
            want["error"].as_str().unwrap_or("?")
        )),
    }
}

#[test]
fn every_golden_matches_the_ts_bytes() {
    let rows = goldens();
    assert!(rows.len() >= 140, "{} goldens", rows.len());
    let mut failures = Vec::new();
    for row in &rows {
        let name = row["name"].as_str().expect("name");
        let code = row["code"].as_str().expect("code");
        failures.extend(check_leg(name, "none", code, &row["none"], Plugins::NONE));
        failures.extend(check_leg(name, "full", code, &row["full"], Plugins::STAGE6));
    }
    assert!(
        failures.is_empty(),
        "{} of {} legs differ:\n{}",
        failures.len(),
        rows.len() * 2,
        failures.join("\n")
    );
}

#[test]
fn the_beautifier_spec_changes_what_it_says() {
    // A spot check that the goldens are not the identity: the stage-6
    // legs of the beautifier's own spec differ from the printer-only legs.
    let rows = goldens();
    let changed = rows
        .iter()
        .filter(|r| {
            r["name"]
                .as_str()
                .is_some_and(|n| n.starts_with("beautifier/"))
        })
        .filter(|r| r["none"]["text"] != r["full"]["text"])
        .count();
    assert!(changed >= 25, "{changed}");
}
