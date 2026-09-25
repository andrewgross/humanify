//! The driver's on-disk metadata writers (TS: unified.ts writeEvalStats /
//! writeStageHashes / writePlacementStats / writeSplitLedger, and
//! `src/stage-fingerprint.ts`). Bytes gated against the TS writers
//! (test/parity/wpb4-vectors.json `writers`, `fingerprints`).
//!
//! Formats (contract 14 §8.4): stats, placement stats — indent 2, NO
//! trailing newline; stage hashes — indent 2 WITH a trailing newline; the
//! split ledger — compact, no trailing newline.

use std::path::Path;

use humanify_model::js::{JsValue, stringify, stringify_pretty};
use humanify_model::js_record;
use humanify_model::jsshape::{CountMap, JsType};
use humanify_model::stats::EvalStats;
use sha2::{Digest, Sha256};

/// `.humanify/` paths (split/layout.ts).
pub const SPLIT_LEDGER_PATH: &str = ".humanify/split-ledger.json";
pub const PLACEMENT_STATS_PATH: &str = ".humanify/placement-stats.json";
pub const STAGE_HASHES_PATH: &str = ".humanify/stage-hashes.json";

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

/// `writeEvalStats(destPath, ...)`.
pub fn write_eval_stats(dest: &Path, stats: &EvalStats) -> std::io::Result<()> {
    write_creating_parent(dest, &stats.to_file_text())
}

js_record! {
    /// The `{afterNaming, afterPlacement}` stage-boundary fingerprints.
    pub struct StageHashes {
        after_naming: String = "afterNaming",
        after_placement: String = "afterPlacement",
    }
}

/// `writeStageHashes`.
pub fn write_stage_hashes(output_dir: &Path, hashes: &StageHashes) -> std::io::Result<()> {
    write_creating_parent(
        &output_dir.join(STAGE_HASHES_PATH),
        &format!("{}\n", stringify_pretty(&hashes.to_js(), 2)),
    )
}

js_record! {
    /// The fields `writePlacementStats` copies out of `StableSplitStats`;
    /// `byTier` is keyed by the placement registry, in its order.
    pub struct PlacementStats {
        statements: f64 = "statements",
        files: f64 = "files",
        folders: f64 = "folders",
        inherited: f64 = "inherited",
        residue_locality: f64 = "residueLocality",
        by_tier: CountMap = "byTier",
    }
}

/// `writePlacementStats`.
pub fn write_placement_stats(output_dir: &Path, stats: &PlacementStats) -> std::io::Result<()> {
    write_creating_parent(
        &output_dir.join(PLACEMENT_STATS_PATH),
        &stringify_pretty(&stats.to_js(), 2),
    )
}

/// `writeSplitLedger`: compact `JSON.stringify(ledger)`. The ledger's typed
/// model arrives with the split (WP5.1); the writer's format is fixed here.
pub fn write_split_ledger(output_dir: &Path, ledger: &JsValue) -> std::io::Result<()> {
    write_creating_parent(&output_dir.join(SPLIT_LEDGER_PATH), &stringify(ledger))
}
