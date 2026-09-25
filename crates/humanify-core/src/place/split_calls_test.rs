//! Every `stableSplitFromCode` call the TS split's own test suite makes
//! (src/split/stable-split.test.ts, split-boots.test.ts,
//! fossil-split-integration.test.ts), replayed through the Rust placement:
//! all three regimes, the kill-switch ablations, the namer/reviser
//! exchanges (requests compared field for field, answers replayed), and
//! the result compared EXACTLY — the per-statement assignment and every
//! placement-trail row. Captured by test/parity/wp51-capture-hook.mjs; the
//! TS statement hashes ride along (the hash-byte seam, lesson 16).

use std::collections::HashMap;

use serde_json::{Map, Value, json};

use super::assign::cluster::{
    ClusterConfig, ClusterNamers, DEFAULT_CLUSTER_CONFIG, assign_clustered,
};
use super::assign::fossil::{FossilOptions, assign_fossil};
use super::assign::namer::{
    FolderSummary, NameKind, NameLevel, SplitNameRequest, SplitNamer, TreeReviser,
};
use super::input::split_input;
use super::ledger::StableSplitLedger;
use super::tiers::{PlacementSwitches, PriorCarry, TierInput, assign_with_prior};
use super::trail::PlacementTrail;

const CALLS: &str = include_str!("../../../../test/parity/wp51-split-calls.jsonl");

/// The TS request object (`undefined` fields omitted, as JSON.stringify).
fn request_json(r: &SplitNameRequest) -> Value {
    let mut m = Map::new();
    m.insert(
        "kind".into(),
        json!(match r.kind {
            NameKind::File => "file",
            NameKind::Folder => "folder",
        }),
    );
    m.insert("mechanicalStem".into(), json!(r.mechanical_stem));
    m.insert("siblings".into(), json!(r.siblings));
    m.insert("bindings".into(), json!(r.bindings));
    if let Some(members) = &r.members {
        m.insert("members".into(), json!(members));
    }
    if let Some(level) = r.level {
        m.insert(
            "level".into(),
            json!(match level {
                NameLevel::Top => "top",
                NameLevel::Sub => "sub",
            }),
        );
    }
    if let Some(evidence) = &r.evidence {
        m.insert("evidence".into(), json!(evidence));
    }
    Value::Object(m)
}

/// Replays the captured namer/reviser exchanges in order, recording any
/// request that differs from the TS's.
struct ReplayState {
    log: Vec<Value>,
    at: usize,
    mismatches: Vec<String>,
}

/// One log serves both hooks (the namer and reviser calls interleave in
/// call order), so the hooks are implemented for `&Replay`.
struct Replay(std::cell::RefCell<ReplayState>);

impl ReplayState {
    fn next(&mut self, kind: &str, arg: Value) -> Value {
        let Some(entry) = self.log.get(self.at) else {
            self.mismatches.push(format!("extra {kind} call"));
            return Value::Null;
        };
        self.at += 1;
        if entry["kind"] != kind || entry["arg"] != arg {
            self.mismatches.push(format!(
                "{kind} call {}: ts {} {}\n  rust {arg}",
                self.at - 1,
                entry["kind"],
                entry["arg"]
            ));
        }
        entry["out"].clone()
    }
}

impl SplitNamer for &Replay {
    fn name(&mut self, requests: &[SplitNameRequest]) -> Vec<Option<String>> {
        let mut state = self.0.borrow_mut();
        // The fossil path calls its namer `mintNamer`; the cluster path
        // `namer` — the capture labels each by its option name.
        let kind = state
            .log
            .get(state.at)
            .and_then(|e| e["kind"].as_str())
            .filter(|k| *k == "mintNamer")
            .unwrap_or("namer")
            .to_string();
        let arg = Value::Array(requests.iter().map(request_json).collect());
        let out = state.next(&kind, arg);
        requests
            .iter()
            .enumerate()
            .map(|(i, _)| out.get(i).and_then(Value::as_str).map(str::to_string))
            .collect()
    }
}

impl TreeReviser for &Replay {
    fn revise(&mut self, folders: &[FolderSummary]) -> Vec<(String, String)> {
        let arg = Value::Array(
            folders
                .iter()
                .map(|f| json!({"name": f.name, "members": f.members}))
                .collect(),
        );
        let out = self.0.borrow_mut().next("reviser", arg);
        out.as_object()
            .map(|o| {
                o.iter()
                    .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn switches_of(disabled: &Value) -> PlacementSwitches {
    let has = |n: &str| {
        disabled
            .as_array()
            .is_some_and(|a| a.iter().any(|d| d == n))
    };
    PlacementSwitches {
        content_anchor: has("content-anchor"),
        anchor_preempt: has("anchor-preempt"),
        anchor_nearident: has("anchor-nearident"),
        allsame_vote: has("allsame-vote"),
        empty_decl_hash_guard: has("empty-decl-hash-guard"),
    }
}

fn cluster_config(overrides: &Value) -> ClusterConfig {
    let mut c = DEFAULT_CLUSTER_CONFIG;
    let get = |k: &str, d: usize| {
        overrides
            .get(k)
            .and_then(Value::as_u64)
            .map_or(d, |v| v as usize)
    };
    c.window = get("window", c.window);
    c.min_gap = get("minGap", c.min_gap);
    c.target_files = get("targetFiles", c.target_files);
    c.max_lines = get("maxLines", c.max_lines);
    c.max_seg = get("maxSeg", c.max_seg);
    c.min_lines = get("minLines", c.min_lines);
    c.min_top = get("minTop", c.min_top);
    c.max_top = get("maxTop", c.max_top);
    c.min_sub = get("minSub", c.min_sub);
    c.max_sub = get("maxSub", c.max_sub);
    c.flat_top = get("flatTop", c.flat_top);
    c.folder_window = get("folderWindow", c.folder_window);
    c
}

/// A Rust trail row in the TS entry's JSON shape.
fn trail_json(trail: &PlacementTrail) -> Vec<Value> {
    trail
        .rows
        .iter()
        .map(|r| {
            let mut m = Map::new();
            m.insert("index".into(), json!(r.index));
            m.insert(
                "span".into(),
                json!({"start": r.key.start, "end": r.key.end}),
            );
            m.insert("names".into(), json!(r.names));
            if let Some(n) = r.name_count {
                m.insert("nameCount".into(), json!(n));
            }
            m.insert("placedBy".into(), json!(r.placed_by));
            m.insert("file".into(), json!(r.file));
            if let Some(p) = &r.prior_file {
                m.insert("priorFile".into(), json!(p));
            }
            if let Some(p) = &r.prior_file_from {
                m.insert("priorFileFrom".into(), json!(p));
            }
            if let Some(h) = &r.hash_miss {
                m.insert("hashMiss".into(), json!(h));
            }
            if let Some(a) = &r.alternatives {
                m.insert("alternatives".into(), a.clone());
            }
            m.insert("evidence".into(), r.evidence.clone());
            Value::Object(m)
        })
        .collect()
}

/// Run one captured call through the Rust regime; `Err` = the regime
/// failed (the TS threw).
fn run(row: &Value, replay: &Replay) -> Result<(Vec<String>, Vec<Value>), String> {
    let code = row["code"].as_str().unwrap();
    let mut input = split_input(code)?;
    let ts_hashes: Vec<String> = row["hashes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|h| h.as_str().unwrap().to_string())
        .collect();
    assert_eq!(ts_hashes.len(), input.hashes.len(), "statement count");
    input.hashes = ts_hashes;
    let mut prior: Option<StableSplitLedger> =
        (!row["prior"].is_null()).then(|| serde_json::from_value(row["prior"].clone()).unwrap());
    // The capture is ONE hash universe: the fresh side replays the TS bytes
    // too, so a TS ledger's `hashVersion: 1` names the bytes this call
    // really uses — stamp it current. (Other versions stay: a capture that
    // exercises a stale ledger keeps exercising it.)
    if let Some(p) = prior.as_mut().filter(|p| p.hash_version == Some(1)) {
        p.hash_version = Some(crate::hash::statement_hash::STATEMENT_HASH_VERSION);
    }
    let mut trail = PlacementTrail::default();
    let assignment = if row["fossil"] == true {
        let has_mint = row["mintNamer"] == true;
        let mut namer = replay;
        assign_fossil(
            &input.body,
            &input.spans,
            &input.hashes,
            prior.as_ref(),
            FossilOptions {
                mint_namer: has_mint.then_some(&mut namer as &mut dyn SplitNamer),
                trail: Some(&mut trail),
                ..FossilOptions::default()
            },
        )?
        .assignment
    } else if let Some(prior) = &prior {
        let carry = (!row["priorCarry"].is_null()).then(|| PriorCarry {
            statement_texts: serde_json::from_value(row["priorCarry"]["statementTexts"].clone())
                .unwrap(),
            match_map: serde_json::from_value::<Vec<(String, String)>>(
                row["priorCarry"]["matchMap"].clone(),
            )
            .unwrap()
            .into_iter()
            .collect::<HashMap<_, _>>(),
        });
        assign_with_prior(
            &TierInput {
                body: &input.body,
                spans: &input.spans,
                hashes: &input.hashes,
                code,
                prior,
                carry: carry.as_ref(),
                switches: switches_of(&row["disabled"]),
            },
            Some(&mut trail),
        )?
        .0
    } else {
        let (mut namer, mut reviser) = (replay, replay);
        assign_clustered(
            &input.body,
            Some((code, input.spans.as_slice())),
            &cluster_config(&row["clusterConfig"]),
            ClusterNamers {
                namer: (row["namer"] == true).then_some(&mut namer as &mut dyn SplitNamer),
                reviser: (row["reviser"] == true).then_some(&mut reviser as &mut dyn TreeReviser),
            },
        )
    };
    Ok((assignment, trail_json(&trail)))
}

#[test]
fn every_captured_ts_split_call_replays_exactly() {
    let rows: Vec<Value> = CALLS
        .lines()
        .map(|l| serde_json::from_str(l).expect("row"))
        .collect();
    assert!(rows.len() >= 70, "the capture covers the TS split suite");
    let mut failures: Vec<String> = Vec::new();
    for (k, row) in rows.iter().enumerate() {
        let replay = Replay(std::cell::RefCell::new(ReplayState {
            log: row["namerLog"].as_array().cloned().unwrap_or_default(),
            at: 0,
            mismatches: Vec::new(),
        }));
        if row["hashes"].is_null() {
            // Not wrapper-shaped: the TS returned null.
            if split_input(row["code"].as_str().unwrap()).is_ok() {
                failures.push(format!("call {k}: TS found no wrapper, Rust did"));
            }
            continue;
        }
        match run(row, &replay) {
            Err(e) => {
                if row["error"].is_null() {
                    failures.push(format!("call {k}: Rust failed ({e}), TS did not"));
                }
            }
            Ok((assignment, trail)) => {
                if !row["error"].is_null() {
                    failures.push(format!("call {k}: TS threw {}, Rust did not", row["error"]));
                    continue;
                }
                if json!(assignment) != row["assignment"] {
                    failures.push(format!("call {k}: assignment differs"));
                }
                if Value::Array(trail) != row["trail"] {
                    failures.push(format!("call {k}: trail differs"));
                }
            }
        }
        let state = replay.0.into_inner();
        if state.at != state.log.len() {
            failures.push(format!(
                "call {k}: {} of {} namer exchanges replayed",
                state.at,
                state.log.len()
            ));
        }
        failures.extend(state.mismatches.iter().map(|m| format!("call {k}: {m}")));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
