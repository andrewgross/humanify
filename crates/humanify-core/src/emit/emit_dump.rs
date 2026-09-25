//! The WP5.3 gate's Rust side: from a TS dump's SHIPPED text (the split's
//! input, names already settled), place (fossil regime, the prior ledger),
//! emit, and write
//!
//! - `<out>/emit.json` — the emitted layout (`compare --sections emit`);
//! - `<out>/tree/<path>` — every emitted file, byte for byte;
//! - `<out>/ledger.json` — the ledger's layout fields after the emit.
//!
//! The one seam is the statement-hash BYTES (lesson 16): the prior ledger's
//! emitted layout carries TS hashes, so the TS bytes are injected behind a
//! proven bijection before alignment. Everything after is the Rust's own.
//! Migration scaffolding — deleted at phase 6 with the TS core.

use std::fs;
use std::path::Path;

use oxc_allocator::Allocator;
use serde_json::{Value, json};

use humanify_model::dump::{EmitFileRow, EmitLayoutFile, EmitStatement, PartitionsFile, SpanKey};
use humanify_model::js::cmp_utf16;

use crate::ingest::Ingest;
use crate::modules::wrapper::find_wrapper_function;
use crate::place::declared::declared_names;
use crate::place::input::split_input;
use crate::place::ledger::{StableSplitLedger, read_ledger};
use crate::place::placement_dump::{
    PlacementGate, Regime, assign_regime, inject_ts_statement_hashes,
};
use crate::place::tiers::PlacementSwitches;
use crate::place::trail::PlacementTrail;
use crate::rename::validated::scopes::BabelScopes;

use super::align::AlignSwitches;
use super::cjs::{RunnableInput, emit_runnable_cjs, wrapper_view};
use super::load_order::bundle_load_order_facts;
use super::review::{review_split, statement_align_name};

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
fn layout_rows(
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
    let mut input = split_input(&shipped)?;
    let partitions: PartitionsFile = read_json(&ts_dump_dir.join("partitions.json"))?;
    let (ts_hashes, _) = inject_ts_statement_hashes(&input, &partitions)?;
    input.hashes = ts_hashes;
    let prior: Option<StableSplitLedger> = gate.prior_ledger.map(read_ledger).transpose()?;
    fs::create_dir_all(out_dir).map_err(|e| format!("mkdir: {e}"))?;

    // Placement (fossil: every Bun bundle).
    let mut trail = PlacementTrail::default();
    let (assignment, _) = assign_regime(
        &input,
        &shipped,
        PlacementGate {
            regime: Regime::Fossil,
            prior_ledger: None,
            prior_text: None,
            match_map: None,
            switches: PlacementSwitches::default(),
            namer: gate.namer,
            reviser: None,
            inject_ts_hashes: true,
        },
        prior.as_ref(),
        &mut trail,
        None,
    )?;

    // The typed parse the emit walks (same text, same spans).
    let allocator = Allocator::default();
    let ingest = Ingest::parse(&allocator, &shipped, "shipped.js");
    let wrapper = find_wrapper_function(ingest.program, ingest.semantic())
        .ok_or("no recognizable bundle wrapper")?;
    let view = wrapper_view(ingest.semantic(), wrapper.span).ok_or("wrapper node not found")?;
    let statements = &view.body.statements;
    let typed_spans: Vec<(u32, u32)> = statements
        .iter()
        .map(|s| {
            let sp = oxc_span::GetSpan::span(s);
            (sp.start, sp.end)
        })
        .collect();
    if typed_spans != input.spans {
        return Err("the typed wrapper body and the split input disagree on spans".into());
    }

    let names: Vec<Option<String>> = input
        .body
        .iter()
        .map(|s| statement_align_name(declared_names(s)))
        .collect();
    let facts = bundle_load_order_facts(statements, &shipped, gate.registrar_exemption_disabled);
    write(&out_dir.join("facts.jsonl"), &facts_jsonl(&facts))?;
    let review = review_split(
        &shipped,
        &input.spans,
        &assignment,
        &input.hashes,
        &names,
        &facts,
        prior.as_ref(),
        gate.switches,
    );

    let scopes = BabelScopes::build(ingest.semantic());
    let runnable = emit_runnable_cjs(&RunnableInput {
        code: &shipped,
        semantic: ingest.semantic(),
        scopes: &scopes,
        wrapper: &view,
        files: &review.files,
        order: &assignment,
        emit_hashes: &review.emit_hashes,
        emit_names: &review.emit_names,
        prior_aliases: prior.as_ref().and_then(|p| p.aliases.as_ref()),
        bundle_hashes: &input.hashes,
        bundle_names: &names,
        facts: &facts,
        switches: gate.switches,
    });

    let mut report = EmitReport {
        files: review.files.len(),
        ..EmitReport::default()
    };
    let tree_dir = out_dir.join("tree");
    let (layout_file, ledger) = match runnable {
        Ok(tree) => {
            for (path, content) in &tree.files {
                write(&tree_dir.join(path), content)?;
            }
            report.tree_files = tree.files.len();
            let aliases: std::collections::HashMap<&str, &str> = tree
                .aliases
                .iter()
                .map(|(f, a)| (f.as_str(), a.as_str()))
                .collect();
            let layout = layout_rows(&tree.layout, &input.spans, |p| {
                aliases.get(p).map(|a| a.to_string())
            });
            let alias_obj: serde_json::Map<String, Value> = tree
                .aliases
                .iter()
                .map(|(f, a)| (f.clone(), Value::String(a.clone())))
                .collect();
            let ledger = json!({
                "files": review.files,
                "order": assignment,
                "hashes": input.hashes,
                "emitHashes": tree.emit_hashes,
                "emitNames": tree.emit_names,
                "emitIndexes": tree.emit_indexes,
                "aliases": alias_obj,
            });
            (layout, ledger)
        }
        Err(reason) => {
            // The TS writes the byte-exact review tree instead, LOUDLY.
            for (path, content) in &review.contents {
                write(&tree_dir.join(path), content)?;
            }
            report.tree_files = review.contents.len();
            report.declined = Some(reason);
            let layout = layout_rows(&review.layout, &input.spans, |_| None);
            let ledger = json!({
                "files": review.files,
                "order": assignment,
                "hashes": input.hashes,
                "emitHashes": review.emit_hashes,
                "emitNames": review.emit_names,
            });
            (layout, ledger)
        }
    };
    write(
        &out_dir.join("meta.json"),
        &serde_json::to_string(&meta).expect("json"),
    )?;
    write(
        &out_dir.join("emit.json"),
        &serde_json::to_string(&layout_file).expect("json"),
    )?;
    write(
        &out_dir.join("ledger.json"),
        &serde_json::to_string(&ledger).expect("json"),
    )?;
    Ok(report)
}
