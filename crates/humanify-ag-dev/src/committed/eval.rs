//! The 034 eval's committed files: `pairs.json` (read by FOUR parsers
//! today — eval.ts twice, run.sh, launch_parity_test.rs), `noise-bands.json`,
//! `self-hop-reference.json` and the launch goldens.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Deserializer};

use super::{Loaded, load_json, load_text};
use crate::paths::{EVAL_DIR, LAUNCH_GOLDENS};

/// `pairs.json`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Pairs {
    #[serde(rename = "_comment")]
    pub comment: String,
    #[serde(rename = "_paths")]
    pub paths_note: String,
    pub inputs_base: String,
    pub priors_base: String,
    #[serde(rename = "_libraryIndex")]
    pub library_index_note: String,
    pub library_index: String,
    pub llm: PairsLlm,
    pub pairs: Vec<Pair>,
}

/// `pairs.json` `llm`: the eval's model settings (never the endpoint).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PairsLlm {
    pub model: String,
    pub api_key: String,
    pub reasoning_effort: String,
    /// Recorded as a string ("472"); typed here, once.
    #[serde(deserialize_with = "u32_from_string")]
    pub concurrency: u32,
}

/// One version transition.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Pair {
    pub from: String,
    pub to: String,
    pub kind: String,
}

impl Pair {
    /// `from->to`, as cards and manifests spell a pair.
    pub fn name(&self) -> String {
        format!("{}->{}", self.from, self.to)
    }
}

fn u32_from_string<'de, D: Deserializer<'de>>(d: D) -> Result<u32, D::Error> {
    let s = String::deserialize(d)?;
    s.parse()
        .map_err(|_| serde::de::Error::custom(format!("not a count: {s:?}")))
}

/// Load `pairs.json`.
pub fn pairs(repo: &Path) -> Result<Loaded<Pairs>, String> {
    let loaded: Loaded<Pairs> = load_json(&repo.join(EVAL_DIR).join("pairs.json"))?;
    if loaded.value.pairs.is_empty() {
        return Err(format!("{}: no pairs", loaded.path.display()));
    }
    Ok(loaded)
}

/// `noise-bands.json`: the measured bands, per naming mode. The top level
/// (and its `byMode`) is scorer 1's, the TS scorer every band before the
/// Rust scorer was measured with; a later scorer version's bands live
/// under `byScorer.<version>` in the same shape, never mixed in.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct NoiseBands {
    pub provenance: BandProvenance,
    pub bands: BTreeMap<String, f64>,
    pub by_mode: BTreeMap<String, ModeBands>,
    #[serde(default)]
    pub by_scorer: BTreeMap<String, ScorerBands>,
}

/// One scorer version's bands: the off mode at its top, other modes under
/// `byMode`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ScorerBands {
    pub provenance: BandProvenance,
    pub bands: BTreeMap<String, f64>,
    #[serde(default)]
    pub by_mode: BTreeMap<String, ModeBands>,
}

/// One naming mode's bands.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModeBands {
    pub provenance: BandProvenance,
    pub bands: BTreeMap<String, f64>,
}

/// Where a band came from.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BandProvenance {
    pub provisional: bool,
    pub sources: Vec<String>,
    pub commit: String,
    pub labels: Vec<String>,
}

impl NoiseBands {
    /// Scorer 1's bands of a naming mode: `off` is the top level.
    pub fn for_mode(&self, mode: &str) -> Option<&BTreeMap<String, f64>> {
        self.for_scorer_mode(1, mode)
    }

    /// The bands one scorer version measured in a naming mode; None when
    /// that version has none for it (never another version's or mode's).
    pub fn for_scorer_mode(&self, scorer: u64, mode: &str) -> Option<&BTreeMap<String, f64>> {
        self.provenance_and_bands(scorer, mode).map(|(_, b)| b)
    }

    /// The provenance and bands of one scorer version's naming mode.
    pub fn provenance_and_bands(
        &self,
        scorer: u64,
        mode: &str,
    ) -> Option<(&BandProvenance, &BTreeMap<String, f64>)> {
        let (top, top_bands, by_mode) = if scorer == 1 {
            (&self.provenance, &self.bands, &self.by_mode)
        } else {
            let s = self.by_scorer.get(&scorer.to_string())?;
            (&s.provenance, &s.bands, &s.by_mode)
        };
        match mode {
            "off" => Some((top, top_bands)),
            other => by_mode.get(other).map(|m| (&m.provenance, &m.bands)),
        }
    }
}

/// Load `noise-bands.json`.
pub fn noise_bands(repo: &Path) -> Result<Loaded<NoiseBands>, String> {
    load_json(&repo.join(EVAL_DIR).join("noise-bands.json"))
}

/// `self-hop-reference.json`: the cold self-hop's reference range.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SelfHopReference {
    #[serde(rename = "_comment")]
    pub comment: String,
    pub version: String,
    pub cold_diff_lines: BTreeMap<String, u64>,
    pub commits: BTreeMap<String, String>,
    pub by_mode: BTreeMap<String, SelfHopMode>,
}

/// One naming mode's self-hop reference.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SelfHopMode {
    #[serde(rename = "_comment")]
    pub comment: String,
    pub version: String,
    pub cold_diff_lines: BTreeMap<String, u64>,
    pub commits: BTreeMap<String, String>,
}

impl SelfHopReference {
    /// The (min, max) cold diff-line range for a naming mode.
    pub fn range(&self, mode: &str) -> Option<(u64, u64)> {
        let lines = match mode {
            "off" => &self.cold_diff_lines,
            other => &self.by_mode.get(other)?.cold_diff_lines,
        };
        Some((*lines.values().min()?, *lines.values().max()?))
    }
}

/// Load `self-hop-reference.json`.
pub fn self_hop_reference(repo: &Path) -> Result<Loaded<SelfHopReference>, String> {
    load_json(&repo.join(EVAL_DIR).join("self-hop-reference.json"))
}

/// Every launch golden (`dev/eval/goldens/`), by file name.
pub fn launch_goldens(repo: &Path) -> Result<BTreeMap<String, Loaded<String>>, String> {
    let mut out = BTreeMap::new();
    for dir in [repo.join(LAUNCH_GOLDENS)] {
        let entries = std::fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            if name.ends_with(".golden.txt") {
                out.insert(name, load_text(&path)?);
            }
        }
    }
    Ok(out)
}
