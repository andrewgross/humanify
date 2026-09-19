//! `selftest` (07 §4, measurement-pitfalls rule 3): ships known-diverged
//! fixture dump pairs — one per divergence class, including a partition
//! split and a prompt differing in one byte — and asserts the engine
//! reports each. A zero from an instrument that has never produced a one is
//! not a measurement: the check stage runs selftest before any green
//! `compare` counts.

use std::path::PathBuf;

use serde_json::json;

use crate::engine::{ALL_SECTIONS, compare_dumps};

/// A planted case: a builder for the base dump and a mutation.
struct PlantedCase {
    name: &'static str,
    /// Expected exit code of `compare` on this pair.
    expected: i32,
    mutate: fn(&mut serde_json::Value),
}

/// The base dump pair, as raw JSON (mutated per case, written to temp dirs).
fn base_dump() -> serde_json::Value {
    json!({
        "meta.json": {
            "schemaVersion": 1,
            "generatedAt": "2026-09-19T00:00:00.000Z",
            "commit": "1813577",
            "flags": {},
            "texts": { "fresh": "aaa", "prior": null, "minified": null }
        },
        "functions.json": {
            "schemaVersion": 1,
            "functions": [
                { "key": {"text": "fresh", "start": 0, "end": 10}, "sessionId": "input.js:1:0",
                  "kind": "function", "name": "old", "nameBinding": {"text": "fresh", "start": 4, "end": 7},
                  "structuralHash": "aaaa1111aaaa1111", "internalCallees": [], "scopeParent": null,
                  "bindings": [{"slot": "$0", "span": {"text": "fresh", "start": 20, "end": 21}, "name": "x"}] },
                { "key": {"text": "fresh", "start": 30, "end": 40}, "sessionId": "input.js:2:0",
                  "kind": "function", "name": "old2", "nameBinding": {"text": "fresh", "start": 34, "end": 38},
                  "structuralHash": "bbbb2222bbbb2222", "internalCallees": [{"text": "fresh", "start": 0, "end": 10}],
                  "scopeParent": null, "bindings": [] }
            ]
        },
        "partitions.json": {
            "schemaVersion": 1,
            "families": [
                { "family": "structuralHash", "members": [
                    { "member": {"text": "fresh", "start": 0, "end": 10}, "hash": "aaaa1111aaaa1111" },
                    // Shares the first member's hash: one class of two — the
                    // planted split below needs a class to split.
                    { "member": {"text": "fresh", "start": 30, "end": 40}, "hash": "aaaa1111aaaa1111" }
                ]}
            ]
        },
        "matches.json": {
            "schemaVersion": 1,
            "pairs": [ { "cascade": "function",
                "prior": {"text": "prior", "start": 0, "end": 10},
                "fresh": {"text": "fresh", "start": 0, "end": 10},
                "tier": "structuralHashUnique" } ],
            "rejections": [ { "cascade": "function",
                "prior": {"text": "prior", "start": 30, "end": 40},
                "kind": "stillAmbiguous",
                "candidates": [{"text": "fresh", "start": 0, "end": 10}, {"text": "fresh", "start": 30, "end": 40}] } ]
        },
        "transfers.json": {
            "schemaVersion": 1,
            "transfers": [ { "target": {"text": "fresh", "start": 4, "end": 7},
                "oldName": "old", "finalName": "newName", "settledBy": "llm",
                "attempts": [ { "tier": "llm", "outcome": "applied", "proposedName": "newName" } ] } ]
        },
        "votes.json": {
            "schemaVersion": 1,
            "votes": [ { "target": {"text": "fresh", "start": 4, "end": 7}, "targetKind": "module",
                "outcome": "accepted",
                "tally": [ {"name": "newName", "total": 2, "exact": 1} ],
                "witnesses": [ {"sourceFunctionId": "input.js:2:0", "oldName": "old2", "exactSlot": true} ] } ]
        },
        "names.json": {
            "schemaVersion": 1,
            "names": [ { "target": {"text": "fresh", "start": 4, "end": 7}, "oldName": "old",
                "newName": "newName", "kind": "function", "classified": "renamed",
                "round": 1, "functionId": "input.js:1:0" } ]
        },
        "placement.json": {
            "schemaVersion": 1,
            "placements": [ { "key": {"text": "fresh", "start": 0, "end": 10}, "index": 0,
                "names": ["old"], "placedBy": "name", "file": "src/thing.js", "evidence": {} } ]
        },
        "emit.json": {
            "schemaVersion": 1,
            "files": [ { "path": "src/thing.js", "statements": [
                { "span": {"text": "fresh", "start": 0, "end": 10}, "slotIndex": 0, "bundleIndex": 0 } ] } ]
        },
        "tree-manifest.json": { "files": [ {"path": "src/thing.js", "sha256": "cafe", "bytes": 10} ] },
        "cache-keys.jsonl": [ { "seq": 0, "params": {"model": "m", "temperature": 0},
          "request": {"code": "c", "identifiers": ["a"], "usedNames": ["x"], "calleeSignatures": [], "callsites": []},
          "cacheKey": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdead" } ],
        "regions.json": { "schemaVersion": 1, "commentRegions": [], "bannerClassifications": [] },
        "modules.json": { "schemaVersion": 1,
          "unpack": { "helperVar": "d",
            "wrapper": { "span": {"text": "minified", "start": 0, "end": 10},
              "bodySpan": {"text": "minified", "start": 2, "end": 10}, "bindingCount": 50 },
            "factories": [
              { "key": {"text": "minified", "start": 60, "end": 90}, "factoryVar": "tO8",
                "lineRange": [11, 11], "contentHash": "a7d5ad4d663d38f3",
                "structuralHash": "b030d374dcac6fa1", "bannerText": "@r/pkg v1.0",
                "bannerPackage": "@r/pkg", "bannerVersion": "1.0" },
              { "key": {"text": "minified", "start": 95, "end": 120}, "factoryVar": "eO8",
                "lineRange": [11, 11], "contentHash": "dd41426aa4f767df",
                // Shares factory[0]'s hash: one class of two — the planted
                // class-split below needs a class to split.
                "structuralHash": "b030d374dcac6fa1" }
            ] },
          "graph": null },
        "prompts.jsonl": [ { "seq": 0, "functionId": "input.js:1:0", "site": "naming", "round": 1,
            "isRetry": false, "cacheKey": "deadbeef", "systemPrompt": "SYSTEM", "userPrompt": "USER",
            "identifiers": ["old"], "targets": [ {"sessionId": "input.js:1:0", "start": 0, "end": 10} ] } ]
    })
}

fn planted_cases() -> Vec<PlantedCase> {
    vec![
        PlantedCase {
            name: "functions-row-missing",
            expected: 1,
            mutate: |v| {
                v["functions.json"]["functions"]
                    .as_array_mut()
                    .unwrap()
                    .remove(1);
            },
        },
        PlantedCase {
            name: "functions-value-changed",
            expected: 1,
            mutate: |v| {
                v["functions.json"]["functions"][0]["name"] = json!("different");
            },
        },
        PlantedCase {
            name: "partitions-class-split",
            expected: 1,
            mutate: |v| {
                v["partitions.json"]["families"][0]["members"][1]["hash"] =
                    json!("cccc3333cccc3333");
            },
        },
        PlantedCase {
            name: "matches-tier-changed",
            expected: 1,
            mutate: |v| {
                v["matches.json"]["pairs"][0]["tier"] = json!("ordinal");
            },
        },
        PlantedCase {
            name: "matches-rejection-kind-changed",
            expected: 1,
            mutate: |v| {
                v["matches.json"]["rejections"][0]["kind"] = json!("unmatched");
            },
        },
        PlantedCase {
            name: "transfers-attempt-changed",
            expected: 1,
            mutate: |v| {
                v["transfers.json"]["transfers"][0]["attempts"][0]["outcome"] = json!("rejected");
            },
        },
        PlantedCase {
            name: "votes-tally-changed",
            expected: 1,
            mutate: |v| {
                v["votes.json"]["votes"][0]["tally"][0]["total"] = json!(3);
            },
        },
        PlantedCase {
            name: "names-classified-changed",
            expected: 1,
            mutate: |v| {
                v["names.json"]["names"][0]["classified"] = json!("unchanged");
            },
        },
        PlantedCase {
            name: "placement-file-changed",
            expected: 1,
            mutate: |v| {
                v["placement.json"]["placements"][0]["file"] = json!("src/elsewhere.js");
            },
        },
        PlantedCase {
            name: "emit-slot-changed",
            expected: 1,
            mutate: |v| {
                v["emit.json"]["files"][0]["statements"][0]["slotIndex"] = json!(1);
            },
        },
        PlantedCase {
            name: "prompts-one-byte-diff",
            expected: 1,
            mutate: |v| {
                v["prompts.jsonl"][0]["userPrompt"] = json!("USER2");
            },
        },
        PlantedCase {
            name: "modules-factory-missing",
            expected: 1,
            mutate: |v| {
                v["modules.json"]["unpack"]["factories"]
                    .as_array_mut()
                    .unwrap()
                    .remove(1);
            },
        },
        PlantedCase {
            name: "modules-banner-changed",
            expected: 1,
            mutate: |v| {
                v["modules.json"]["unpack"]["factories"][0]["bannerPackage"] = json!("@other/pkg");
            },
        },
        PlantedCase {
            name: "modules-wrapper-changed",
            expected: 1,
            mutate: |v| {
                v["modules.json"]["unpack"]["wrapper"]["bindingCount"] = json!(51);
            },
        },
        PlantedCase {
            // The structuralHash BYTES are excluded from the row compare —
            // this proves a hash change is still caught (the class
            // partition: this row now represents its own class).
            name: "modules-hash-class-split",
            expected: 1,
            mutate: |v| {
                v["modules.json"]["unpack"]["factories"][0]["structuralHash"] =
                    json!("ffff0000ffff0000");
            },
        },
        PlantedCase {
            name: "modules-helper-changed",
            expected: 1,
            mutate: |v| {
                v["modules.json"]["unpack"]["helperVar"] = json!("e");
            },
        },
        PlantedCase {
            name: "anchors-differ",
            expected: 2,
            mutate: |v| {
                v["meta.json"]["texts"]["fresh"] = json!("bbb");
            },
        },
    ]
}

/// Write a dump pair to two temp dirs; `mutate` applies to the RIGHT side.
fn write_pair(case: &PlantedCase, root: &std::path::Path) -> (PathBuf, PathBuf) {
    let left = root.join("left");
    let right = root.join("right");
    std::fs::create_dir_all(left.join("text")).unwrap();
    std::fs::create_dir_all(right.join("text")).unwrap();

    let base = base_dump();
    let mutated = {
        let mut v = base.clone();
        (case.mutate)(&mut v);
        v
    };
    write_side(&left, &base);
    write_side(&right, &mutated);
    (left, right)
}

fn write_side(dir: &std::path::Path, files: &serde_json::Value) {
    for (name, value) in files.as_object().unwrap() {
        if name == "prompts.jsonl" {
            let lines: Vec<String> = value
                .as_array()
                .unwrap()
                .iter()
                .map(|p| serde_json::to_string(p).unwrap())
                .collect();
            std::fs::write(dir.join(name), format!("{}\n", lines.join("\n"))).unwrap();
        } else {
            std::fs::write(dir.join(name), serde_json::to_string(value).unwrap()).unwrap();
        }
    }
    // The excerpt source the engine reads spans from.
    std::fs::write(
        dir.join("text").join("fresh.js"),
        "0123456789abcdefghij0123456789abcdefghij0123456789",
    )
    .unwrap();
}

/// Run every planted case; returns Err listing the cases that failed to
/// produce the expected exit code. Exit 0 = every planted divergence was
/// DETECTED (the instrument works); exit 1 = a case went undetected.
/// How many planted cases the selftest runs (the stage's progress line).
pub fn planted_case_count() -> usize {
    planted_cases().len()
}

pub fn run_selftest() -> Result<(), String> {
    let root =
        std::env::temp_dir().join(format!("humanify-parity-selftest-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    // Control first: the identical pair must compare clean (exit 0).
    let (left, right) = write_pair(
        &PlantedCase {
            name: "control-identical",
            expected: 0,
            mutate: |_| {},
        },
        &root,
    );
    let outcome = compare_dumps(
        &left,
        &right,
        &ALL_SECTIONS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>(),
        20,
    )
    .map_err(|e| format!("control case errored: {e}"))?;
    if outcome.exit_code() != 0 {
        return Err(format!(
            "CONTROL FAILED: the identical pair reported exit {} with {} divergence(s) — the instrument cannot be trusted:\n{}",
            outcome.exit_code(),
            outcome.divergences.len(),
            outcome
                .divergences
                .iter()
                .map(|d| format!("  {}: {}", d.section, d.key))
                .collect::<Vec<_>>()
                .join("\n")
        ));
    }

    let mut failures = Vec::new();
    for case in planted_cases() {
        let (left, right) = write_pair(&case, &root);
        let outcome = compare_dumps(
            &left,
            &right,
            &ALL_SECTIONS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            20,
        )
        .map_err(|e| format!("case {} errored: {e}", case.name))?;
        if outcome.exit_code() != case.expected {
            failures.push(format!(
                "case {}: expected exit {}, got {} — planted divergence NOT detected",
                case.name,
                case.expected,
                outcome.exit_code()
            ));
        }
        let _ = std::fs::remove_dir_all(&left);
        let _ = std::fs::remove_dir_all(&right);
    }
    let _ = std::fs::remove_dir_all(&root);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "selftest: {} of {} planted case(s) undetected:\n{}",
            failures.len(),
            planted_cases().len(),
            failures.join("\n")
        ))
    }
}
