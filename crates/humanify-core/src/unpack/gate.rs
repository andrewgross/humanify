//! The `humanify unpack` verb's recording namer: what its `--llm-log`
//! writes (every vendor LLM batch's keys, evidence and answers).

use serde::Serialize;

use crate::modules::vendor_names::{VendorNameRequest, VendorNamer};

/// One LLM batch as the verb records it (`batches[]` of the log).
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
