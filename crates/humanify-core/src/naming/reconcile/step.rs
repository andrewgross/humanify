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

use super::{ReconcileOptions, ReconcileResult, collect_word_tokens, hunks, reconcile_diff_noise};
use crate::ingest::Ingest;
use crate::naming::waves::render::{program_edits, render_program};
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
    /// The rename ledger's stage for this pass (`--rename-ledger`, only
    /// when a rename applied): its renames over the text it parsed.
    pub ledger: Option<crate::rename::validated::ledger::RenameLedger>,
}

/// When the rename ledger's walk of this pass's AST happens (`--rename-
/// ledger`): right after the pass, or after a LATER parse of this pass's
/// output (the deferred sweep's), which on a full bundle clears Babel's
/// scope cache — the walk then re-crawls (declaration order).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LedgerWalk {
    Live,
    AfterLaterParse,
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
    ledger: Option<LedgerWalk>,
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
    let ph = crate::profiling::phase("reconcile:diff");
    let diff_text = match hunks::compute_normal_diff(prior_text, code) {
        Ok(d) => d,
        Err(e) => return Err((e, trail)),
    };
    drop(ph);
    let ph = crate::profiling::phase("reconcile:options");
    let mut state = RenameState::with_trail(semantic, Anchor::Generated, trail);
    let opts = pipeline_options(prior_text);
    drop(ph);
    let ph = crate::profiling::phase("reconcile:apply");
    let result = reconcile_diff_noise(semantic, &mut state, &diff_text, eligible, &opts);
    drop(ph);
    let code = (!result.renames.is_empty()).then(|| render_program(semantic, &state));
    let ledger = ledger.filter(|_| code.is_some()).map(|walk| {
        use crate::rename::validated::ledger::{build_rename_ledger, parse_clears_scope_cache};
        if walk == LedgerWalk::AfterLaterParse
            && code.as_deref().is_some_and(parse_clears_scope_cache)
        {
            state.recrawl_order(|_| true);
        }
        let rendered = program_edits(semantic, &state, &[]);
        build_rename_ledger(semantic.source_text(), &state, &rendered)
    });
    Ok(PriorDiffOutcome {
        result,
        code,
        trail: state.finish().trail,
        ledger,
    })
}
