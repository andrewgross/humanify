//! The WPB.2 gate's seams (migration scaffolding — deleted at phase 6 with
//! the TS core, 02 §9).
//!
//! The TS structural-hash injection that bridged the hash-byte difference
//! (the structuralSignature exemption) ended at WP5.6e: the unpack names
//! vendor files by the Rust's OWN factory hashes, so this verb's tree no
//! longer equals a TS unpack tree wherever a hash-derived string reaches
//! it (`lib_<hash8>` names and file names, runtime identifiers, the LLM
//! batch keys). What is left is the recording namer the verb's
//! `--llm-log` writes.

use serde::Serialize;

use crate::modules::vendor_names::{VendorNameRequest, VendorNamer};

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
