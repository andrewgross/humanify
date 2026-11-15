//! An [`Identity`] derived from the records a scored label holds.
//!
//! Eval labels: `commit.txt`, `pipeline.json`, each pair's run manifest
//! (`<v>-run.json`) and card (`<v>.json`), in the shapes `eval score`
//! writes them. `eval score` writes each card's `<v>.identity.json` from
//! these, and `eval rescore` does for a label scored before identities;
//! every reader then reads the records ([`super::read_eval_label`]).
//! Library labels: `summary.json` (the library leaderboard's reader).
//! A field the label did not record is UNKNOWN, naming what was missing —
//! never a guess.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use super::{
    Binary, CardKind, Corpus, F, Harness, Identity, LibraryIndex, Llm, LlmCache, PipelineSettings,
    SCHEMA, Scorer,
};
use crate::committed::sha256_hex;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PipelineJson {
    pipeline: Option<PipelineBlock>,
    naming_mode: Option<String>,
    library_index: Option<String>,
    run_config: Option<RunConfigRef>,
    boot_gate_model: Option<String>,
}

#[derive(Deserialize)]
struct PipelineBlock {
    bin: Option<BinBlock>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinBlock {
    path: Option<String>,
    sha256: Option<String>,
    commit: Option<String>,
    dirty: Option<bool>,
}

#[derive(Deserialize)]
struct RunConfigRef {
    sha256: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    pair: String,
    provenance: Option<Provenance>,
    inputs: Option<Inputs>,
    config: Option<ManifestConfig>,
    pipeline: Option<ManifestPipeline>,
    library: Option<LibraryBlock>,
}

#[derive(Deserialize)]
struct Provenance {
    dirty: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Inputs {
    input: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestConfig {
    endpoint: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
    concurrency: Option<u64>,
    resolved: Option<Resolved>,
    cache: Option<CacheBlock>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Resolved {
    config_file: Option<ConfigFileRef>,
    library: Option<ResolvedLibrary>,
}

#[derive(Deserialize)]
struct ConfigFileRef {
    sha256: Option<String>,
}

#[derive(Deserialize)]
struct ResolvedLibrary {
    matching: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheBlock {
    enabled: Option<bool>,
    entries_before: Option<u64>,
    entries_after: Option<u64>,
}

#[derive(Deserialize)]
struct ManifestPipeline {
    bin: Option<ManifestBin>,
}

#[derive(Deserialize)]
struct ManifestBin {
    sha256: Option<String>,
    commit: Option<String>,
    dirty: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryBlock {
    index_id: Option<String>,
}

#[derive(Deserialize)]
struct CardPair {
    pair: String,
    #[serde(rename = "scorerVersion")]
    scorer_version: u64,
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// The EFFECTIVE naming mode: pipeline.json's `namingMode`, else the
/// binary's own record (`config.resolved.library.matching`: names is
/// names, anything else is off's bytes).
fn naming_mode_of(p: &PipelineJson, m: Option<&Manifest>) -> F<Option<String>> {
    if let Some(mode) = &p.naming_mode {
        return F::Known(Some(mode.clone()));
    }
    let Some(m) = m else {
        return F::unknown("no run manifest and no pipeline.json namingMode");
    };
    let resolved = m.config.as_ref().and_then(|c| c.resolved.as_ref());
    if let Some(r) = resolved {
        let names = r.library.as_ref().and_then(|l| l.matching.as_deref()) == Some("names");
        return F::Known(Some(if names { "names" } else { "off" }.into()));
    }
    F::unknown("neither pipeline.json nor the run manifest records the naming mode")
}

fn is_index_id(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn library_index_of(
    mode: &F<Option<String>>,
    p: &PipelineJson,
    m: Option<&Manifest>,
) -> F<Option<LibraryIndex>> {
    match mode.known().and_then(|o| o.as_deref()) {
        Some("names") => {}
        Some(_) => return F::Known(None),
        None => return F::unknown("the naming mode is unknown"),
    }
    let dir = p.library_index.clone();
    let from_dir = dir
        .as_deref()
        .and_then(|d| Path::new(d).file_name()?.to_str().map(str::to_string))
        .filter(|n| is_index_id(n));
    let id = m
        .and_then(|m| m.library.as_ref()?.index_id.clone())
        .or(from_dir);
    F::Known(Some(LibraryIndex {
        id: F::from_option(
            id,
            "neither the manifest nor the index path records the index id",
        ),
        dir: F::from_option(dir, "pipeline.json records no libraryIndex"),
        bytes: F::unknown("not recorded before identities"),
    }))
}

fn binary_of(p: &PipelineJson, m: Option<&Manifest>) -> Binary {
    let label_bin = p.pipeline.as_ref().and_then(|b| b.bin.as_ref());
    let card_bin = m.and_then(|m| m.pipeline.as_ref()?.bin.as_ref());
    let why = "not recorded";
    let path = label_bin.and_then(|b| b.path.clone());
    let name = path
        .as_deref()
        .and_then(|p| Path::new(p).file_name()?.to_str().map(str::to_string));
    Binary {
        path: F::from_option(path, why),
        // The card's own record (the file AS LAUNCHED) wins over the label's.
        sha256: F::from_option(
            card_bin
                .and_then(|b| b.sha256.clone())
                .or_else(|| label_bin.and_then(|b| b.sha256.clone())),
            why,
        ),
        build_commit: F::from_option(
            card_bin
                .and_then(|b| b.commit.clone())
                .or_else(|| label_bin.and_then(|b| b.commit.clone()))
                .filter(|c| !c.is_empty()),
            why,
        ),
        dirty: F::from_option(
            card_bin
                .and_then(|b| b.dirty)
                .or_else(|| label_bin.and_then(|b| b.dirty)),
            why,
        ),
        name: F::from_option(name, why),
    }
}

fn llm_of(m: Option<&Manifest>) -> F<Option<Llm>> {
    let Some(c) = m.and_then(|m| m.config.as_ref()) else {
        return F::unknown("no run manifest");
    };
    let cache = match &c.cache {
        Some(CacheBlock {
            enabled: Some(false),
            ..
        }) => F::Known(None),
        Some(cb) => F::Known(Some(LlmCache {
            dir: F::unknown("the run manifest records no cache dir"),
            entries_before: F::from_option(cb.entries_before, "not recorded"),
            entries_after: F::from_option(cb.entries_after, "not recorded"),
        })),
        None => F::unknown("the run manifest records no cache block"),
    };
    F::Known(Some(Llm {
        model: F::from_option(c.model.clone(), "not recorded"),
        reasoning_effort: F::from_option(c.reasoning_effort.clone().map(Some), "not recorded"),
        concurrency: F::from_option(c.concurrency, "not recorded"),
        endpoint_id: F::from_option(
            c.endpoint.as_deref().map(|e| sha256_hex(e.as_bytes())),
            "not recorded",
        ),
        cache,
    }))
}

/// The scored input's sha, hashed now from the recorded path when the
/// file is still there (input bundles are immutable release artifacts).
fn inputs_of(m: Option<&Manifest>) -> F<BTreeMap<String, String>> {
    let Some(path) = m.and_then(|m| m.inputs.as_ref()?.input.clone()) else {
        return F::unknown("no run manifest records the input");
    };
    match std::fs::read(&path) {
        Ok(bytes) => F::Known(BTreeMap::from([(path, sha256_hex(&bytes))])),
        Err(_) => F::unknown("the recorded input is not on this host"),
    }
}

/// An identity for every pair card of the eval label in `dir`, derived
/// from its records. Err when a record is unreadable or a card records no
/// scorer version.
pub fn eval_label(dir: &Path) -> Result<Vec<Identity>, String> {
    let commit = std::fs::read_to_string(dir.join("commit.txt"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let pipeline: PipelineJson = read_json(&dir.join("pipeline.json"))?.unwrap_or_default();
    let mut out = Vec::new();
    for v in super::card_versions(dir)? {
        let manifest: Option<Manifest> = read_json(&dir.join(format!("{v}-run.json")))?;
        let card: CardPair = read_json(&dir.join(format!("{v}.json")))?
            .ok_or_else(|| format!("{}: no card {v}.json", dir.display()))?;
        let pair = manifest.as_ref().map_or(card.pair, |m| m.pair.clone());
        out.push(eval_card(
            &pipeline,
            commit.as_deref(),
            manifest.as_ref(),
            pair,
            card.scorer_version,
        ));
    }
    Ok(out)
}

fn eval_card(
    p: &PipelineJson,
    commit: Option<&str>,
    m: Option<&Manifest>,
    pair: String,
    scorer_version: u64,
) -> Identity {
    let naming_mode = naming_mode_of(p, m);
    let toml_sha = m
        .and_then(|m| {
            m.config
                .as_ref()?
                .resolved
                .as_ref()?
                .config_file
                .as_ref()?
                .sha256
                .clone()
        })
        .map(Some);
    Identity {
        schema: SCHEMA,
        kind: CardKind::EvalPair,
        harness: Harness {
            commit: F::from_option(commit.map(str::to_string), "no commit.txt"),
            dirty: F::from_option(
                m.and_then(|m| m.provenance.as_ref()?.dirty),
                "no run manifest records it",
            ),
        },
        binary: binary_of(p, m),
        library_index: library_index_of(&naming_mode, p, m),
        naming_mode,
        pipeline_settings: PipelineSettings {
            sets: F::unknown("the run config is not kept beside the label"),
            config_toml_sha256: F::from_option(toml_sha, "no resolved config record"),
        },
        llm: llm_of(m),
        corpus: Corpus {
            name: F::Known("034-pairs".into()),
            manifest_sha256: F::unknown("pairs.json was not recorded"),
            layers: F::Known(None),
            entry: pair,
            inputs: inputs_of(m),
        },
        scorer: Scorer {
            name: "034-kpi".into(),
            version: F::Known(scorer_version.to_string()),
        },
        boot_gate_model: F::from_option(p.boot_gate_model.clone().map(Some), "not recorded"),
        run_config_sha256: F::from_option(
            p.run_config
                .as_ref()
                .and_then(|r| r.sha256.clone())
                .map(Some),
            "not recorded",
        ),
    }
}

#[derive(Deserialize)]
struct LibrarySummary {
    schema: String,
    config: BTreeMap<String, serde_json::Value>,
}

/// An identity for every entry card of the library label in `dir`
/// (`summary.json` + `cards/<entry>.json`).
pub fn library_label(dir: &Path) -> Result<Vec<Identity>, String> {
    let summary: LibrarySummary = read_json(&dir.join("summary.json"))?
        .ok_or_else(|| format!("{}: no summary.json", dir.display()))?;
    let text = |k: &str| {
        summary
            .config
            .get(k)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    let (name, version) = match summary.schema.split_once('/') {
        Some((n, v)) => (n.to_string(), F::Known(v.to_string())),
        None => (
            summary.schema.clone(),
            F::unknown("the schema names no version"),
        ),
    };
    let layers: BTreeMap<String, String> = summary
        .config
        .iter()
        .filter(|(k, _)| k.ends_with("Sha256") && *k != "binarySha256" && *k != "corpusSha256")
        .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
        .collect();
    let settings: Option<Vec<String>> = summary
        .config
        .get("settings")
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    // A private split (test, test2, bun-held, remin2) commits its summary
    // only: its one identity stands for the whole corpus (entry `*`).
    let mut entries: Vec<String> = match std::fs::read_dir(dir.join("cards")) {
        Ok(read) => read
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter_map(|n| n.strip_suffix(".json").map(str::to_string))
            .collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => vec!["*".to_string()],
        Err(e) => return Err(format!("{}/cards: {e}", dir.display())),
    };
    entries.sort();
    let card = |entry: String| Identity {
        schema: SCHEMA,
        kind: CardKind::LibraryEntry,
        harness: Harness {
            commit: F::unknown("library labels record no harness commit"),
            dirty: F::unknown("library labels record no harness commit"),
        },
        binary: Binary {
            path: F::unknown("not recorded"),
            sha256: F::from_option(text("binarySha256"), "not recorded"),
            build_commit: F::from_option(text("buildCommit"), "not recorded"),
            dirty: F::unknown("not recorded"),
            name: F::unknown("not recorded"),
        },
        naming_mode: F::Known(None),
        library_index: F::Known(Some(LibraryIndex {
            id: F::from_option(text("indexId"), "not recorded"),
            dir: F::unknown("not recorded"),
            bytes: F::from_option(
                summary.config.get("indexBytes").and_then(|v| v.as_u64()),
                "not recorded",
            ),
        })),
        pipeline_settings: PipelineSettings {
            // No settings block: the label ran with none.
            sets: F::Known(settings.clone().unwrap_or_default()),
            config_toml_sha256: F::unknown("discover.toml is not kept beside the label"),
        },
        llm: F::Known(None),
        corpus: Corpus {
            name: F::Known(text("corpus").unwrap_or_else(|| "dev".to_string())),
            manifest_sha256: F::from_option(text("corpusSha256"), "not recorded"),
            layers: F::Known(Some(layers.clone())),
            entry,
            inputs: F::unknown("entry inputs are the corpus manifest's"),
        },
        scorer: Scorer {
            name: name.clone(),
            version: version.clone(),
        },
        boot_gate_model: F::Known(None),
        run_config_sha256: F::unknown("not recorded"),
    };
    Ok(entries.into_iter().map(card).collect())
}

#[cfg(test)]
#[path = "derive_test.rs"]
mod derive_test;
