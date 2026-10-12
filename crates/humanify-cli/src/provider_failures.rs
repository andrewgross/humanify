//! The run's LLM calls that failed after every retry, per namer
//! (concurrency audit 2026-10, R2 + recommendation 1c).
//!
//! A failed call is a hole in the names: the run is fail-soft and still
//! exits 0 (the exit code is an undecided policy, audit recommendation
//! 1a). So the hole must be loud — ONE WARNING line for every namer — and
//! machine-readable: the `--stats-json` `providerErrors` block, which the
//! eval reads to refuse a card from a run that lost answers
//! (experiments/lib/invariants.ts). Print-only and record-only: no
//! decision reads it.
//!
//! The namers, and where each counts its failures:
//! - `naming`: the function/identifier waves and sweeps (`outcome.errors`);
//! - `modules`: the module namer (`ModuleNamingReport::failed_calls`);
//! - `vendor`: the vendor namer (`VendorNamingStats::batches_failed`);
//! - `split`: the split's file/folder namer (`failed_batches`);
//! - `tree_reviser`: the split's tree reviser (`failed_calls`).

use humanify_model::stats::ProviderErrorStats;

/// Failed calls (a batch is one call), summed over the run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProviderFailures {
    pub naming: usize,
    pub modules: usize,
    pub vendor: usize,
    pub split: usize,
    pub tree_reviser: usize,
}

impl ProviderFailures {
    pub fn total(&self) -> usize {
        self.naming + self.modules + self.vendor + self.split + self.tree_reviser
    }

    /// The `--stats-json` `providerErrors` block (always written).
    pub fn stats(&self) -> ProviderErrorStats {
        ProviderErrorStats {
            total: self.total() as f64,
            naming: self.naming as f64,
            modules: self.modules as f64,
            vendor: self.vendor as f64,
            split: self.split as f64,
            tree_reviser: self.tree_reviser as f64,
        }
    }

    /// The one WARNING, naming each namer that lost calls; None on a run
    /// that lost none.
    pub fn warning(&self) -> Option<String> {
        let total = self.total();
        if total == 0 {
            return None;
        }
        let parts: Vec<String> = [
            ("naming", self.naming),
            ("modules", self.modules),
            ("vendor", self.vendor),
            ("split", self.split),
            ("tree reviser", self.tree_reviser),
        ]
        .iter()
        .filter(|(_, n)| *n > 0)
        .map(|(name, n)| format!("{name} {n}"))
        .collect();
        Some(format!(
            "WARNING: {total} LLM call(s) failed after every retry (provider errors: {}) — \
their names are missing from this output; re-run when the endpoint is healthy",
            parts.join(", ")
        ))
    }
}

#[cfg(test)]
mod provider_failures_test;
