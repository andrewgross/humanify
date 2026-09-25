//! The pipeline step (reconcile-step.ts `runPriorDiffReconciliation`):
//! parse the GENERATED output (so positions are output coordinates, the
//! diff's), reconcile it against the prior version's text with the
//! pipeline's options, and re-render when a rename applied.
//!
//! The TS re-checks the pure-rename structural invariant after its
//! Babel-AST mutation and discards the pass on a violation. The Rust
//! applier writes only the name overlay (`rename::validated`), and the
//! render rewrites only identifier occurrences and babel's name-dependent
//! property forms — the invariant holds by construction, so there is no
//! discard branch to port (the oracle pairs never took it either: every
//! reconciled text is present in the dumps).

use oxc_allocator::Allocator;

use super::{
    ReconcileOptions, ReconcilePlant, ReconcileResult, collect_word_tokens, hunks,
    reconcile_diff_noise,
};
use crate::ingest::Ingest;
use crate::naming::waves::render::render_program;
use crate::rename::eligibility::Eligibility;
use crate::rename::validated::RenameState;
use crate::trail::{Anchor, StrategyTrail};

/// What the step hands on (`PriorDiffReconcileOutcome` + the full result).
pub struct PriorDiffOutcome {
    pub result: ReconcileResult,
    /// The re-rendered text — set only when a rename applied.
    pub code: Option<String>,
    /// The strategy trail, continued through this pass.
    pub trail: StrategyTrail,
}

/// The pipeline's options for the prior-diff step (reconcileInternal).
pub fn pipeline_options(prior_text: &str) -> ReconcileOptions {
    ReconcileOptions {
        apply: true,
        descriptive_tier: true,
        last_resort_tier: true,
        skeleton_vote_tier: true,
        consumer_tier: true,
        prior_names: Some(collect_word_tokens(prior_text)),
        // `priorVersionCode.split("\n").length`.
        prior_line_count: Some(prior_text.split('\n').count()),
        ..ReconcileOptions::default()
    }
}

/// `runPriorDiffReconciliation(code, prior)`. Err when the step could not
/// run (unparseable text, a `diff` failure) — the caller ships the
/// pre-reconcile output, as the TS returns undefined.
pub fn run_prior_diff_reconciliation(
    code: &str,
    prior_text: &str,
    eligible: &Eligibility,
    trail: StrategyTrail,
    plant: Option<ReconcilePlant>,
) -> Result<PriorDiffOutcome, (String, StrategyTrail)> {
    let allocator = Allocator::default();
    let ingest = Ingest::parse_unambiguous(&allocator, code);
    if !ingest.errors.is_empty() {
        return Err((
            format!("generated text does not parse: {}", ingest.errors[0]),
            trail,
        ));
    }
    let semantic = ingest.semantic();
    let diff_text = match hunks::compute_normal_diff(prior_text, code) {
        Ok(d) => d,
        Err(e) => return Err((e, trail)),
    };
    let mut state = RenameState::with_trail(semantic, Anchor::Generated, trail);
    let opts = ReconcileOptions {
        plant,
        ..pipeline_options(prior_text)
    };
    let result = reconcile_diff_noise(semantic, &mut state, &diff_text, eligible, &opts);
    let code = (!result.renames.is_empty()).then(|| render_program(semantic, &state));
    Ok(PriorDiffOutcome {
        result,
        code,
        trail: state.finish().trail,
    })
}
