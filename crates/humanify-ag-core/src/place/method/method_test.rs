//! Which split method a bundle gets ([`super::choose_split_method`]): the
//! module markers only when they describe the bundle, read from what the
//! bundle CONTAINS — never from which bundler wrote it.

use super::{MARKER_COVERAGE_THRESHOLD, MarkerOffer, choose_split_method};
use crate::place::placement_dump::Regime;
use crate::twins::fossil::MarkerCoverage;

fn coverage(modules: usize, covered: u64, app: u64) -> MarkerCoverage {
    MarkerCoverage {
        modules,
        app_bytes: app,
        covered_bytes: covered,
        glued_statements: 0,
        tail_statements: 0,
    }
}

/// Claude Code's least-covered measured version (2.1.197, 99.900%) keeps
/// the markers, with and without a prior.
#[test]
fn a_bundle_its_markers_describe_keeps_the_marker_method() {
    let cc = coverage(4456, 99_900, 100_000);
    for prior in [false, true] {
        let choice = choose_split_method(MarkerOffer::Offered, prior, Some(cc));
        assert_eq!(choice.regime, Regime::Fossil);
        assert!(choice.reason.contains("99.90%"), "{}", choice.reason);
    }
}

/// C1: a Bun app with plain imports has no markers. It used to be handed
/// the marker method because its BUNDLER writes markers, and the split
/// failed after naming. It gets the fresh grouping (no prior) or the
/// prior's layout (with one).
#[test]
fn a_bundle_without_markers_gets_the_fresh_grouping() {
    let none = coverage(0, 0, 50_000);
    let fresh = choose_split_method(MarkerOffer::Offered, false, Some(none));
    assert_eq!(fresh.regime, Regime::Cluster);
    assert!(
        fresh.reason.contains("no module markers"),
        "{}",
        fresh.reason
    );
    let prior = choose_split_method(MarkerOffer::Offered, true, Some(none));
    assert_eq!(prior.regime, Regime::Tiers);
}

/// C2: a mixed bundle (some lazy modules, the rest eager) under the
/// threshold gets the fresh grouping for ALL of it.
#[test]
fn a_mixed_bundle_under_the_threshold_gets_the_fresh_grouping() {
    let mixed = coverage(3, 40_000, 100_000);
    let choice = choose_split_method(MarkerOffer::Offered, false, Some(mixed));
    assert_eq!(choice.regime, Regime::Cluster);
    assert!(choice.reason.contains("40.00%"), "{}", choice.reason);
    assert!(choice.reason.contains("99%"), "{}", choice.reason);
    // Just under the line is still under it.
    let near = coverage(10, 98_999, 100_000);
    assert_eq!(
        choose_split_method(MarkerOffer::Offered, false, Some(near)).regime,
        Regime::Cluster
    );
    // Exactly at the line is over it.
    let at = coverage(10, 99_000, 100_000);
    assert_eq!(at.share(), MARKER_COVERAGE_THRESHOLD);
    assert_eq!(
        choose_split_method(MarkerOffer::Offered, false, Some(at)).regime,
        Regime::Fossil
    );
}

/// The adapter records no markers, or `--disable fossil-split`: never the
/// marker method, whatever the coverage.
#[test]
fn markers_not_offered_never_pick_the_marker_method() {
    let full = coverage(10, 100, 100);
    for offer in [MarkerOffer::NotProvided, MarkerOffer::Disabled] {
        let choice = choose_split_method(offer, false, Some(full));
        assert_eq!(choice.regime, Regime::Cluster);
        let with_prior = choose_split_method(offer, true, Some(full));
        assert_eq!(with_prior.regime, Regime::Tiers);
    }
    let disabled = choose_split_method(MarkerOffer::Disabled, false, None);
    assert!(
        disabled.reason.contains("--disable fossil-split"),
        "{}",
        disabled.reason
    );
}
