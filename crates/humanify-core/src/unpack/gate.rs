//! The WPB.2 gate's seams (migration scaffolding — deleted at phase 6 with
//! the TS core, 02 §9).
//!
//! The Rust structural hash BYTES differ from the TS's by design (00-control
//! §3, the structuralSignature exemption): only the equivalence CLASSES are
//! comparable. Every hash-derived string the adapter writes — `lib_<hash8>`
//! fallback names and file names, `runtimeIdentifier`s and so every
//! rewritten reference inside the vendored bodies and runtime.js, the LLM
//! batch keys, the carry-over join against a TS-written prior manifest —
//! therefore differs from the TS run's output for a reason the gate already
//! accepts. `inject_ts_hashes` removes exactly that reason and nothing
//! else: after proving the Rust classification's hash classes are the TS's
//! (same factory count, same factory var per bundle position, and a
//! BIJECTION between the two hash partitions), it overwrites each record's
//! structural hash with the TS's bytes for the same bundle position. From
//! there every decision is the Rust's own, and the output tree must equal
//! the TS unpack stage's tree byte-for-byte.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::modules::BunModuleClassification;
use crate::modules::vendor_names::{VendorNameRequest, VendorNamer};

/// One TS unpack-site factory row (the dump's `modules.json` `unpack`
/// section): the factory var and the TS structural hash bytes, in bundle
/// order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsFactoryHash {
    pub factory_var: String,
    pub structural_hash: String,
}

/// Read the TS unpack-site factory rows from a dump's `modules.json`.
pub fn read_ts_factory_hashes(modules_json: &Path) -> Result<Vec<TsFactoryHash>, String> {
    let text = std::fs::read_to_string(modules_json)
        .map_err(|e| format!("{}: {e}", modules_json.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("modules.json: {e}"))?;
    let rows = value["unpack"]["factories"]
        .as_array()
        .ok_or("modules.json has no unpack.factories")?;
    rows.iter()
        .map(|row| {
            Ok(TsFactoryHash {
                factory_var: row["factoryVar"]
                    .as_str()
                    .ok_or("factory row without factoryVar")?
                    .to_string(),
                structural_hash: row["structuralHash"]
                    .as_str()
                    .ok_or("factory row without structuralHash")?
                    .to_string(),
            })
        })
        .collect()
}

/// What the injection proved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InjectionReport {
    pub factories: usize,
    /// Distinct hash classes (the same count on both sides — a bijection).
    pub classes: usize,
}

/// Prove the two hash partitions are the same partition of the same
/// factories, then substitute the TS bytes (see the module doc). Any
/// mismatch is an error: the gate must not paper over a class divergence.
pub fn inject_ts_hashes(
    classification: &mut BunModuleClassification,
    ts: &[TsFactoryHash],
) -> Result<InjectionReport, String> {
    let factories = &mut classification.factories;
    if factories.len() != ts.len() {
        return Err(format!(
            "factory count: Rust {} vs TS {}",
            factories.len(),
            ts.len()
        ));
    }
    let mut rust_to_ts: HashMap<&str, &str> = HashMap::new();
    let mut ts_to_rust: HashMap<&str, &str> = HashMap::new();
    for (i, (f, t)) in factories.iter().zip(ts).enumerate() {
        if f.factory_var != t.factory_var {
            return Err(format!(
                "factory {i}: Rust var {} vs TS var {}",
                f.factory_var, t.factory_var
            ));
        }
        let r = f.structural_hash.as_str();
        let s = t.structural_hash.as_str();
        if *rust_to_ts.entry(r).or_insert(s) != s || *ts_to_rust.entry(s).or_insert(r) != r {
            return Err(format!(
                "factory {i} ({}): the hash classes differ — not a bijection",
                f.factory_var
            ));
        }
    }
    let classes = rust_to_ts.len();
    for (f, t) in factories.iter_mut().zip(ts) {
        f.structural_hash = t.structural_hash.clone();
    }
    Ok(InjectionReport {
        factories: ts.len(),
        classes,
    })
}

/// One LLM batch as the gate records it — the TS probe's `.llm.json`
/// `batches[]` shape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RecordedBatch {
    pub keys: Vec<String>,
    pub evidence: Vec<String>,
    pub proposals: Vec<Option<String>>,
}

/// A namer that records every batch it forwards (finding 6's evidence: the
/// leftover set the pass asked, the prompt evidence, the answers).
pub struct RecordingNamer<'n> {
    pub inner: &'n mut dyn VendorNamer,
    pub batches: Vec<RecordedBatch>,
}

impl VendorNamer for RecordingNamer<'_> {
    fn name_batch(&mut self, requests: Vec<VendorNameRequest>) -> Vec<Option<String>> {
        let keys = requests.iter().map(|r| r.key.clone()).collect();
        let evidence = requests.iter().map(|r| r.evidence.clone()).collect();
        let proposals = self.inner.name_batch(requests);
        self.batches.push(RecordedBatch {
            keys,
            evidence,
            proposals: proposals.clone(),
        });
        proposals
    }
}
