//! The driver's on-disk metadata writers (TS: unified.ts writeEvalStats /
//! writeStageHashes / writePlacementStats / writeSplitLedger, and
//! `src/stage-fingerprint.ts`). Bytes gated against the TS writers
//! (test/parity/wpb4-vectors.json `writers`, `fingerprints`).
//!
//! Formats: stats, placement stats — indent 2, NO trailing newline; stage
//! hashes — indent 2 WITH a trailing newline; the split ledger — compact,
//! no trailing newline. The two `.humanify/` records are part of the
//! written tree: their bytes are pinned by `writers_test`.

use std::path::Path;

use humanify_ag_model::ordered::OrderedMap;
use humanify_ag_model::stats::EvalStats;
use sha2::{Digest, Sha256};

/// `.humanify/` paths (split/layout.ts).
pub const SPLIT_LEDGER_PATH: &str = ".humanify/split-ledger.json";
pub const PLACEMENT_STATS_PATH: &str = ".humanify/placement-stats.json";
pub const STAGE_HASHES_PATH: &str = ".humanify/stage-hashes.json";
/// `--library-matching report`'s report (config sweep step 6): a
/// diagnostics-only sidecar, read by no decision.
pub const LIBRARY_REPORT_PATH: &str = ".humanify/library-report.json";

/// `stageFingerprint(content)`: the first 16 hex of sha256 over the UTF-8
/// bytes — a stage-boundary fingerprint short enough to compare by eye.
pub fn stage_fingerprint(content: &str) -> String {
    let digest = Sha256::digest(content.as_bytes());
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

fn write_creating_parent(dest: &Path, text: &str) -> std::io::Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(dest, text)
}

/// Serde's pretty JSON (indent 2), no trailing newline.
fn pretty(value: &impl serde::Serialize) -> String {
    serde_json::to_string_pretty(value).expect("a record serializes")
}

/// `writeEvalStats(destPath, ...)`.
pub fn write_eval_stats(dest: &Path, stats: &EvalStats) -> std::io::Result<()> {
    write_creating_parent(dest, &stats.to_file_text())
}

/// The `{afterNaming, afterPlacement}` stage-boundary fingerprints.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageHashes {
    pub after_naming: String,
    pub after_placement: String,
}

/// `writeStageHashes`.
pub fn write_stage_hashes(output_dir: &Path, hashes: &StageHashes) -> std::io::Result<()> {
    write_creating_parent(
        &output_dir.join(STAGE_HASHES_PATH),
        &format!("{}\n", pretty(hashes)),
    )
}

/// The fields `writePlacementStats` copies out of `StableSplitStats`;
/// `byTier` is keyed by the placement registry, in its order.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacementStats {
    pub statements: u64,
    pub files: u64,
    pub folders: u64,
    pub inherited: u64,
    pub residue_locality: u64,
    pub by_tier: OrderedMap<u64>,
}

/// A JSON document, pretty (indent 2) with a trailing newline.
pub fn write_json_file(path: &Path, doc: &serde_json::Value) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(doc).expect("a JSON value serializes");
    write_creating_parent(path, &format!("{text}\n"))
}

/// `writePlacementStats`.
pub fn write_placement_stats(output_dir: &Path, stats: &PlacementStats) -> std::io::Result<()> {
    write_creating_parent(&output_dir.join(PLACEMENT_STATS_PATH), &pretty(stats))
}

/// `writeSplitLedger`: the ledger's compact JSON, no trailing newline.
pub fn write_split_ledger(
    output_dir: &Path,
    ledger: &humanify_ag_core::place::ledger::SplitLedger,
) -> std::io::Result<()> {
    write_creating_parent(&output_dir.join(SPLIT_LEDGER_PATH), &ledger.to_json())
}

/// `RENAME_LEDGER_APPLIER`: the self-contained replay script written next
/// to the ledger (plain Node, no humanify dependency). The same algorithm
/// as `rename::validated::ledger::apply_rename_ledger`: linear (ascending
/// slices joined once — finding #48), every recorded edit applied, and the
/// result checked against the ledger's `outputSha256` (finding #49).
pub const RENAME_LEDGER_APPLIER: &str = r#"#!/usr/bin/env node
// Apply this humanify rename ledger to its source snapshot, reproducing the
// naming stage's output byte for byte (checked against the ledger's
// outputSha256). Usage: node apply.mjs [outfile]  (stdout if no outfile).
import { readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import * as path from "node:path";
import { fileURLToPath } from "node:url";

const dir = path.dirname(fileURLToPath(import.meta.url));
const source = readFileSync(path.join(dir, "source.js"), "utf8");
const ledger = JSON.parse(
  readFileSync(path.join(dir, "rename-ledger.json"), "utf8")
);

const sha = (s) => createHash("sha256").update(s).digest("hex");
// Apply one stage to src, verifying the snapshot hash first: every edit in
// ascending order, the output assembled from the slices between them (one
// pass, linear). An occurrence is [start, end] (the entry's finalName) or
// [start, end, text]; a stage's `edits` are [start, end, text]. The base
// ledger is stage 0; each post stage edits the prior stage's output.
function applyStage(src, stage) {
  if (sha(src) !== stage.sourceSha256) {
    throw new Error("source does not match the stage's sourceSha256");
  }
  const edits = [];
  for (const e of stage.entries) {
    for (const [s, en, text] of e.occurrences) {
      edits.push([s, en, text ?? e.finalName]);
    }
  }
  for (const edit of stage.edits ?? []) edits.push(edit);
  edits.sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const parts = [];
  let pos = 0;
  for (const [s, en, text] of edits) {
    if (s < pos) throw new Error("overlapping edits");
    parts.push(src.slice(pos, s), text);
    pos = en;
  }
  parts.push(src.slice(pos));
  return parts.join("");
}

let out = applyStage(source, ledger);
for (const stage of ledger.post ?? []) out = applyStage(out, stage);
if (ledger.outputSha256 && sha(out) !== ledger.outputSha256) {
  throw new Error("the replay does not reproduce the recorded output");
}
const dest = process.argv[2];
if (dest) {
  writeFileSync(dest, out);
  console.error(`wrote ${dest}`);
} else {
  process.stdout.write(out);
}
"#;

/// `--rename-ledger <dir>`: the ledger (compact JSON, JS string offsets),
/// its source snapshot, and the standalone applier.
pub fn write_rename_ledger(
    dir: &Path,
    bundle: &humanify_ag_core::rename::validated::ledger::RenameLedgerBundle,
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(dir.join("rename-ledger.json"), bundle.to_json())?;
    std::fs::write(dir.join("source.js"), &bundle.source)?;
    std::fs::write(dir.join("apply.mjs"), RENAME_LEDGER_APPLIER)
}
