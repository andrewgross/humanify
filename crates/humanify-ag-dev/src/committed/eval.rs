//! The eval's committed files: `pairs.json` and
//! `self-hop-reference.json` (the launch goldens are read by the tests
//! that hold launches to them).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Deserializer};

use super::{Loaded, load_json};
use crate::paths::EVAL_DIR;

/// `pairs.json`.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Pairs {
    #[serde(rename = "_comment")]
    pub _comment: String,
    #[serde(rename = "_paths")]
    pub _paths_note: String,
    pub inputs_base: String,
    #[serde(rename = "_libraryIndex")]
    pub _library_index_note: String,
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
    /// The pair a full run self-hops on: exactly one carries it.
    #[serde(default, rename = "selfHop")]
    pub self_hop: bool,
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
    let marked = loaded.value.pairs.iter().filter(|p| p.self_hop).count();
    if marked != 1 {
        return Err(format!(
            "{}: {marked} pairs carry selfHop; exactly one must (the pair a full run self-hops on)",
            loaded.path.display()
        ));
    }
    Ok(loaded)
}

/// `self-hop-reference.json`: the cold self-hop's reference range.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SelfHopReference {
    #[serde(rename = "_comment")]
    pub _comment: String,
    pub version: String,
    pub cold_diff_lines: BTreeMap<String, u64>,
    #[serde(rename = "commits")]
    pub _commits: BTreeMap<String, String>,
    pub by_mode: BTreeMap<String, SelfHopMode>,
}

/// One naming mode's self-hop reference.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SelfHopMode {
    #[serde(rename = "_comment")]
    pub _comment: String,
    pub version: String,
    pub cold_diff_lines: BTreeMap<String, u64>,
    #[serde(rename = "commits")]
    pub _commits: BTreeMap<String, String>,
}

impl SelfHopReference {
    /// The (min, max) cold diff-line range for a naming mode.
    #[cfg(test)]
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
