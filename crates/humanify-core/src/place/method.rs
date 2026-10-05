//! Which split method a bundle gets — the ONE owner of that question
//! (docs/responsibility.md). Chosen from what the bundle CONTAINS, never
//! from which bundler wrote it (findings C1/C2 of the 2026-10-05
//! app-specific scan; Andrew's rule: bundler-specific code is fine as a
//! plugin piece, app-specific code is not).
//!
//! - MODULE MARKERS ([`Regime::Fossil`]) — the bundle's own lazy-init
//!   module records are the files. Only when the toolchain's unpack
//!   adapter offers them (P11) AND they describe at least
//!   [`MARKER_COVERAGE_THRESHOLD`] of the app code
//!   ([`crate::twins::fossil::marker_coverage`]).
//! - otherwise PRIOR LAYOUT ([`Regime::Tiers`]) when a prior ledger exists,
//!   else the FRESH GROUPING ([`Regime::Cluster`]) — for the WHOLE bundle.
//!
//! The mixed case (some lazy modules, the rest eager) gets the fresh
//! grouping for all of it, not markers for the covered part: the covered
//! part is only an upper bound (an eager module of functions and constants
//! has a lazy module's shape), so its file boundaries are not knowable
//! from the text; and the uncovered statements interleave with the
//! segments, so no contiguous remainder exists to group. Evidence: the
//! `bun-mixed` e2e fixture, where the marker method glued an eager module
//! into a lazy module's file and piled the rest into `src/index.js`.

use crate::place::input::{SplitInput, split_input};
use crate::place::placement_dump::Regime;
use crate::toolchain::{ModuleWrapperGrammar, Toolchain};
use crate::twins::fossil::{MarkerCoverage, marker_coverage};

/// The share of the app code the markers must describe for the marker
/// method. Measured 2026-10-05 with this module's own measure on the
/// eight Claude Code versions the eval walks, on the text a run splits
/// (unpacked, formatted): 2.1.85 99.963%, .86 99.963%, .118 99.959%, .119
/// 99.925%, .197 99.900%, .198 99.902%, .215 99.907%, .216 99.908% — the
/// uncovered rest is the 4-8 statement entry tail. 99% leaves the
/// least-covered version (2.1.197) 10x headroom on the uncovered share
/// (0.100% vs 1%), and admits at most 1% provably-misplaced code —
/// strict on purpose, because the measure is a LOWER bound on the eager
/// code. Every other measured bundle sits far under it: the bundle
/// fixtures with one planted lazy module 19-41% (bun-bundle,
/// esbuild-bundle, esbuild-bundle-small, esbuild-kept-factory), the
/// `bun-mixed` fixture (two of six modules lazy) and `bun-plain` (none).
pub const MARKER_COVERAGE_THRESHOLD: f64 = 0.99;

/// Whether the run's toolchain offers the module markers at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkerOffer {
    /// The unpack adapter's bundles carry them (P11) — the coverage decides.
    Offered,
    /// The unpack adapter records none.
    NotProvided,
    /// `--disable fossil-split`.
    Disabled,
}

/// The split method the bundle gets, and why — what the run log prints
/// and `--stats-json` records (`splitMethod`).
#[derive(Clone, Debug, PartialEq)]
pub struct SplitChoice {
    pub regime: Regime,
    /// The markers' coverage of the app code, when measured.
    pub coverage: Option<MarkerCoverage>,
    pub reason: String,
}

/// The method's plain name (`--stats-json`'s `splitMethod.method`).
pub fn method_name(regime: Regime) -> &'static str {
    match regime {
        Regime::Fossil => "module-markers",
        Regime::Tiers => "prior-layout",
        Regime::Cluster => "fresh-grouping",
    }
}

fn percent(share: f64) -> String {
    format!("{:.2}%", share * 100.0)
}

/// Choose the split method. `coverage` is the bundle's
/// [`MarkerCoverage`] (None: not measured).
pub fn choose_split_method(
    offer: MarkerOffer,
    prior_present: bool,
    coverage: Option<MarkerCoverage>,
) -> SplitChoice {
    let fallback = if prior_present {
        Regime::Tiers
    } else {
        Regime::Cluster
    };
    let threshold = percent(MARKER_COVERAGE_THRESHOLD).replace(".00", "");
    let reason = match (offer, coverage) {
        (MarkerOffer::Disabled, _) => "module markers disabled (--disable fossil-split)".into(),
        (MarkerOffer::NotProvided, _) => "the unpack adapter records no module markers".into(),
        (MarkerOffer::Offered, None) => "the module markers were not measured".into(),
        (MarkerOffer::Offered, Some(c)) if c.modules == 0 => {
            "the bundle records no module markers (no lazy-init modules)".into()
        }
        (MarkerOffer::Offered, Some(c)) => {
            let share = c.share();
            let line = format!(
                "{} module marker(s) cover {} of the app code",
                c.modules,
                percent(share)
            );
            if share >= MARKER_COVERAGE_THRESHOLD {
                return SplitChoice {
                    regime: Regime::Fossil,
                    coverage: Some(c),
                    reason: format!("{line} (threshold {threshold})"),
                };
            }
            format!(
                "{line}, under the {threshold} threshold ({} eager statement(s) inside modules, {} after the last)",
                c.glued_statements, c.tail_statements
            )
        }
    };
    SplitChoice {
        regime: fallback,
        coverage,
        reason,
    }
}

/// `--stats-json`'s `splitMethod` block for a chosen method.
pub fn method_record(choice: &SplitChoice) -> humanify_model::stats::SplitMethodStats {
    humanify_model::stats::SplitMethodStats {
        method: method_name(choice.regime).to_string(),
        reason: choice.reason.clone(),
        marker_coverage: choice.coverage.map(|c| c.share()),
        marker_modules: choice.coverage.map(|c| c.modules as f64),
        threshold: MARKER_COVERAGE_THRESHOLD,
    }
}

/// The `splitMethod` block of a bundle whose layout the split does not
/// read: `not-split` (the named output is written unsplit).
pub fn not_split_record(why: &str) -> humanify_model::stats::SplitMethodStats {
    unchosen_record("not-split", format!("unsupported bundle layout: {why}"))
}

fn unchosen_record(method: &str, reason: String) -> humanify_model::stats::SplitMethodStats {
    humanify_model::stats::SplitMethodStats {
        method: method.to_string(),
        reason,
        marker_coverage: None,
        marker_modules: None,
        threshold: MARKER_COVERAGE_THRESHOLD,
    }
}

/// What `detect --split-method` can say about an input.
pub enum InputSplitMethod {
    /// The method a fresh run would choose.
    Chosen(SplitChoice),
    /// A layout the split does not read: the run finishes unsplit.
    NotSplit(String),
    /// The layout is the split's, but the input's statements could not be
    /// read here (the ESTree substrate refuses some raw literals — a lone
    /// surrogate escape, `1e400` — that a run's split never sees once the
    /// unpack has extracted the vendor code holding them).
    Unmeasured(String),
}

impl InputSplitMethod {
    /// The `splitMethod` block for it.
    pub fn record(&self) -> humanify_model::stats::SplitMethodStats {
        match self {
            InputSplitMethod::Chosen(choice) => method_record(choice),
            InputSplitMethod::NotSplit(why) => not_split_record(why),
            InputSplitMethod::Unmeasured(why) => unchosen_record(
                "unmeasured",
                format!("could not read the bundle's statements: {why}"),
            ),
        }
    }
}

/// The module markers' coverage of a split input's app code — `text` is
/// the text `input` was parsed from; the module factories the run's
/// module wrapper grammar recognises in it are set aside as vendor code.
pub fn bundle_marker_coverage(
    input: &SplitInput,
    text: &str,
    module_wrappers: ModuleWrapperGrammar,
) -> MarkerCoverage {
    let factory_helper = module_wrappers.identify_factory_helper(text);
    marker_coverage(
        &input.body,
        &input.spans,
        factory_helper.as_ref().map(|h| h.name.as_str()),
    )
}

/// The split method a bundle would get on a FRESH run (no prior),
/// measured on the input itself — `humanify detect --split-method`. A run
/// measures the text it splits (the post-unpack runtime, its vendor
/// factories already extracted); both set the factories aside, so they
/// read the same app code. The layout is judged first, by the gate the run
/// uses ([`crate::toolchain::BundleLayout::original_bundle_binding_count`]).
pub fn split_method_of_input(text: &str, toolchain: &Toolchain) -> InputSplitMethod {
    let layout = toolchain.layout.piece;
    if let Err(why) = layout.original_bundle_binding_count(text) {
        return InputSplitMethod::NotSplit(why);
    }
    let input = match split_input(text, layout) {
        Ok(input) => input,
        Err(why) => return InputSplitMethod::Unmeasured(why),
    };
    let coverage = bundle_marker_coverage(&input, text, toolchain.module_wrappers.piece);
    let offer = if toolchain.unpack.piece.provides_module_fossils() {
        MarkerOffer::Offered
    } else {
        MarkerOffer::NotProvided
    };
    InputSplitMethod::Chosen(choose_split_method(offer, false, Some(coverage)))
}

#[cfg(test)]
mod method_test;
