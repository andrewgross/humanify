//! Prior-version orchestration helpers (WP2.4) — TS original:
//! `src/prior-version/prior-version.ts`. Only the pipeline-called input
//! contract lives here; the rename-time application surface (exact-match
//! placeholder application, the ambiguity probe, the lifecycle state
//! machine) lands with WP3's rename port.

/// Minimum prior functions before the same-program sanity floor applies
/// (TS `SAME_PROGRAM_FLOOR_MIN_FUNCTIONS` :723).
pub const SAME_PROGRAM_FLOOR_MIN_FUNCTIONS: usize = 50;
/// Minimum fraction of prior functions whose hash exists in the new
/// version (TS `SAME_PROGRAM_PRESENCE_FLOOR` :724).
pub const SAME_PROGRAM_PRESENCE_FLOOR: f64 = 0.05;

/// TS `assertPriorLooksLikeSameProgram` (:732): a prior that shares
/// (nearly) no structural hashes with the new version is a wrong file, not
/// an aggressive refactor — matched AND ambiguous prior functions both
/// count as presence (only the cascade's `unmatched` are absent), so even
/// a version where nothing disambiguates passes. Fails fast instead of
/// letting a full-cost run transfer nothing.
///
/// `prior_function_count` is the prior side's function-row count; the TS
/// passes `priorFnMap.size`. `unmatched` is the FUNCTION cascade result's
/// `unmatched` length.
pub fn assert_prior_looks_like_same_program(
    prior_function_count: usize,
    unmatched: usize,
) -> Result<(), String> {
    if prior_function_count < SAME_PROGRAM_FLOOR_MIN_FUNCTIONS {
        return Ok(());
    }
    let present = prior_function_count.saturating_sub(unmatched);
    let fraction = present as f64 / prior_function_count as f64;
    if fraction < SAME_PROGRAM_PRESENCE_FLOOR {
        return Err(format!(
            "prior version does not appear to be the same program: only {present} of \
             {prior_function_count} prior functions have a matching structural hash in \
             the new version. Check the --prior-version file; drop the flag to run \
             without transfer."
        ));
    }
    Ok(())
}
