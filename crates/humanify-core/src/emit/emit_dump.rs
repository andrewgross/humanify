//! The WP5.3 gate's Rust side: from a TS dump's SHIPPED text (the split's
//! input, names already settled), run the split stage
//! ([`super::stable_split::stable_split`] — the pipeline's own owner of
//! the order, fossil regime, the prior ledger) and write
//!
//! - `<out>/emit.json` — the emitted layout (`compare --sections emit`);
//! - `<out>/tree/<path>` — every emitted file, byte for byte;
//! - `<out>/ledger.json` — the persisted ledger, the TS writer's bytes
//!   (`JSON.stringify`, compact);
//! - `<out>/runnable.txt` — the runnable map's keys in emission order;
//! - `<out>/facts.jsonl` — every statement's load-order facts.
//!
//! The statement-hash BYTES are the Rust's own (WP5.6e ended the TS-byte
//! injection): a TS-written prior ledger is re-keyed from its sibling
//! `humanified.js` ([`crate::place::ledger::settle_prior_hashes`]), and
//! the written ledger carries the Rust's bytes — so the ledger and any
//! `module-<hash8>` stem no longer equal a TS dump's.
//! Migration scaffolding — deleted at phase 6 with the TS core.

use std::fs;
use std::path::Path;

use serde_json::Value;

use humanify_model::dump::{EmitFileRow, EmitLayoutFile, EmitStatement, SpanKey};
use humanify_model::js::{cmp_utf16, stringify};

use crate::place::ledger::{StableSplitLedger, read_ledger, settle_prior_hashes};
use crate::place::placement_dump::Regime;
use crate::place::tiers::PlacementSwitches;

use super::align::AlignSwitches;
use super::stable_split::{SplitOptions, stable_split};

/// What the verb was asked to do.
pub struct EmitGate<'a> {
    pub prior_ledger: Option<&'a Path>,
    pub namer: Option<&'a mut dyn crate::place::assign::namer::SplitNamer>,
    pub switches: AlignSwitches,
    pub registrar_exemption_disabled: bool,
}

/// What the verb did.
#[derive(Debug, Default)]
pub struct EmitReport {
    pub files: usize,
    pub tree_files: usize,
    /// The runnable emit's decline reason (the review tree was written).
    pub declined: Option<String>,
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| format!("mkdir {}: {e}", dir.display()))?;
    }
    fs::write(path, content).map_err(|e| format!("write {}: {e}", path.display()))
}

/// The layout rows, sorted by path as the dump writer sorts them.
pub fn layout_rows(
    layout: &[(String, Vec<usize>)],
    spans: &[(u32, u32)],
    alias_of: impl Fn(&str) -> Option<String>,
) -> EmitLayoutFile {
    let mut files: Vec<EmitFileRow> = layout
        .iter()
        .map(|(path, idxs)| EmitFileRow {
            path: path.clone(),
            alias: alias_of(path),
            statements: idxs
                .iter()
                .enumerate()
                .map(|(slot, &i)| EmitStatement {
                    span: SpanKey {
                        text: "shipped".into(),
                        start: i64::from(spans[i].0),
                        end: i64::from(spans[i].1),
                    },
                    slot_index: slot as u64,
                    bundle_index: i as u64,
                })
                .collect(),
        })
        .collect();
    files.sort_by(|a, b| cmp_utf16(&a.path, &b.path));
    EmitLayoutFile {
        schema_version: 1,
        files,
    }
}

/// Every statement's load-order facts, one JSON line each, in the TS
/// probe's shape (`{i, hoisted, effects, reads, writes}`, names sorted by
/// UTF-16 code units — their order is unobservable).
fn facts_jsonl(facts: &[super::load_order::LoadOrderFacts]) -> String {
    let names = |v: &[String]| {
        let mut v: Vec<&String> = v.iter().collect();
        v.sort_by(|a, b| cmp_utf16(a, b));
        serde_json::to_string(&v).expect("json")
    };
    facts
        .iter()
        .enumerate()
        .map(|(i, f)| {
            format!(
                "{{\"i\":{i},\"hoisted\":{},\"effects\":{},\"reads\":{},\"writes\":{}}}",
                f.hoisted,
                f.effects,
                names(&f.reads),
                names(&f.writes)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Run placement + emit over a TS dump and write the gate's outputs.
pub fn dump_emit(ts_dump_dir: &Path, out_dir: &Path, gate: EmitGate) -> Result<EmitReport, String> {
    let meta: Value = read_json(&ts_dump_dir.join("meta.json"))?;
    let shipped = fs::read_to_string(ts_dump_dir.join("text").join("shipped.js"))
        .map_err(|e| format!("shipped text: {e}"))?;
    let mut prior: Option<StableSplitLedger> = gate.prior_ledger.map(read_ledger).transpose()?;
    if let (Some(ledger), Some(path)) = (prior.as_mut(), gate.prior_ledger) {
        // A TS-written ledger re-keys from its sibling humanified.js (the
        // text it was written from), as the pipeline's split stage does.
        let text = fs::read_to_string(path.with_file_name("humanified.js")).ok();
        eprintln!(
            "{}",
            settle_prior_hashes(ledger, text.as_deref()).describe()
        );
    }
    fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;

    let outcome = stable_split(
        &shipped,
        SplitOptions {
            regime: Regime::Fossil,
            prior: prior.as_ref(),
            carry: None,
            namer: gate.namer,
            reviser: None,
            placement: PlacementSwitches::default(),
            align: gate.switches,
            registrar_exemption_disabled: gate.registrar_exemption_disabled,
            split_pure: false,
            trail: None,
        },
    )?;
    write(&out_dir.join("facts.jsonl"), &facts_jsonl(&outcome.facts))?;
    let tree_dir = out_dir.join("tree");
    for (path, content) in &outcome.files {
        write(&tree_dir.join(path), content)?;
    }
    if let Some(keys) = &outcome.runnable {
        // The runnable map's keys in emission order — the finishing
        // stage's split-file list and entry lookup (WP5.4).
        write(&out_dir.join("runnable.txt"), &keys.join("\n"))?;
    }
    let aliases: std::collections::HashMap<&str, &str> = outcome
        .aliases
        .iter()
        .map(|(f, a)| (f.as_str(), a.as_str()))
        .collect();
    let layout_file = layout_rows(&outcome.layout, &outcome.spans, |p| {
        aliases.get(p).map(|a| a.to_string())
    });
    write(
        &out_dir.join("meta.json"),
        &serde_json::to_string(&meta).expect("json"),
    )?;
    write(
        &out_dir.join("emit.json"),
        &serde_json::to_string(&layout_file).expect("json"),
    )?;
    write(&out_dir.join("ledger.json"), &stringify(&outcome.ledger))?;
    Ok(EmitReport {
        files: outcome.stats.files,
        tree_files: outcome.files.len(),
        declined: outcome.declined,
    })
}
