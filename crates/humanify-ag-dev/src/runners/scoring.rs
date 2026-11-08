//! THE SEAM between the runners and the eval scorer: one pair's card
//! (`analyze`), its evidence page (`trail_report`) and the label's summary
//! (`summarize`). The runner decides WHAT is scored (which two trees and
//! the fresh run's stats); the scorer (`crate::eval`, scorer version 2,
//! lane L2a) decides the numbers. It scores the layout block when both
//! trees have `src/` and vendor when both have `vendor/` — always (rule 8).

use std::path::{Path, PathBuf};

use super::Ctx;
use crate::eval::analyze::{PairInputs as ScorerInputs, score_pair};

/// The trees and record one card is scored from.
pub struct PairInputs<'a> {
    pub fresh_tree: &'a Path,
    pub prior_tree: &'a Path,
    pub stats: &'a Path,
    pub pair: &'a str,
}

impl PairInputs<'_> {
    fn scorer(&self) -> ScorerInputs {
        ScorerInputs {
            pair: self.pair.to_string(),
            fresh_tree: PathBuf::from(self.fresh_tree),
            prior_tree: PathBuf::from(self.prior_tree),
            stats: PathBuf::from(self.stats),
        }
    }
}

/// Score one pair: the card's text (JS spelling), or Err.
pub fn analyze(_ctx: &Ctx, p: &PairInputs) -> Result<String, String> {
    score_pair(&p.scorer()).map(|card| crate::json::js_text(&card, true))
}

/// The pair's human-readable evidence page from its diagnostics trail.
pub fn trail_report(_ctx: &Ctx, diag: &Path, page: &Path, p: &PairInputs) -> Result<(), String> {
    crate::eval::report::write_report(diag, page, &p.scorer())
}

/// The label's summary (summary.json + the table + the banners), printed;
/// the exit code.
pub fn summarize(ctx: &Ctx, label: &str) -> i32 {
    let dir = ctx.repo.join(crate::paths::EVAL_RESULTS).join(label);
    let summary = crate::committed::eval::self_hop_reference(&ctx.repo)
        .and_then(|r| crate::eval::summarize::summarize(&dir, label, &r.value));
    match summary {
        Ok(text) => {
            for line in text.lines() {
                ctx.say(line);
            }
            0
        }
        Err(e) => {
            ctx.say(format!("SUMMARY FAILED: {e}"));
            1
        }
    }
}
