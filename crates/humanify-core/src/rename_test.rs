//! Eligibility tests — fixture generated from the TS (test/parity/skip-list.json)
//! plus the two pattern rules' TS-derived cases.

use crate::rename::eligibility::{NeverRename, create_skip_set, is_eligible};
use humanify_model::detection::{BundlerType, MinifierType};

/// The skip-sets are a VERBATIM port — the committed fixture (generated
/// from src/rename/skip-list.js over all six bundler+minifier combos) is
/// the exact-equality oracle.
#[test]
fn skip_sets_match_the_ts_fixture_exactly() {
    let raw = include_str!("../../../test/parity/skip-list.json");
    let ts: std::collections::BTreeMap<String, Vec<String>> =
        serde_json::from_str(raw).expect("fixture parses");
    for (combo, names) in &ts {
        let (b, m) = combo.split_once(':').expect("combo shape");
        // The fixture's "none" bundler is no verdict; its "none" minifier
        // is the not-minified verdict — neither has a list.
        let bundler: BundlerType = if b == "none" {
            BundlerType::Unknown
        } else {
            serde_json::from_value(serde_json::json!(b)).expect("a bundler name")
        };
        let minifier: MinifierType =
            serde_json::from_value(serde_json::json!(m)).expect("a minifier name");
        let lists = NeverRename::for_verdicts(bundler, minifier);
        let mine: Vec<&str> = {
            let mut v: Vec<&str> = create_skip_set(lists).into_iter().collect();
            v.sort();
            v
        };
        let theirs: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        assert_eq!(mine, theirs, "combo {combo} diverges");
    }
}

/// TS-derived pattern cases: rename-eligibility.test.ts's shapes.
#[test]
fn pattern_rules_match_the_ts() {
    // hard skip-set
    assert!(!is_eligible("require", NeverRename::UNIVERSAL));
    assert!(!is_eligible("__esModule", NeverRename::UNIVERSAL)); // hmm — is __esModule in the set? no — pattern
    // word-like dunder: __esm (short after __) reserved only when word-like
    assert!(
        !is_eligible("__esm", NeverRename::UNIVERSAL),
        "__esm is word-like (3 chars after __)"
    );
    assert!(!is_eligible("__commonJS", NeverRename::UNIVERSAL));
    assert!(
        is_eligible("__c", NeverRename::UNIVERSAL),
        "short dunder is a minified app binding"
    );
    assert!(is_eligible("__ab", NeverRename::UNIVERSAL));
    // SWC helper shape
    assert!(!is_eligible(
        "_interop_require_default",
        NeverRename::UNIVERSAL
    ));
    assert!(!is_eligible("_ts_generator", NeverRename::UNIVERSAL));
    assert!(
        is_eligible("_foo", NeverRename::UNIVERSAL),
        "single segment is not the helper shape"
    );
    assert!(
        is_eligible("_myHelper", NeverRename::UNIVERSAL),
        "uppercase breaks the shape"
    );
    // short names ARE eligible (the inverted heuristic's point)
    assert!(is_eligible("get", NeverRename::UNIVERSAL));
    assert!(is_eligible("A9_", NeverRename::UNIVERSAL));
    assert!(!is_eligible("", NeverRename::UNIVERSAL));
}

/// Toolchain review R20: swc's helper names are ONE list. The never-rename
/// set holds all of them; minifier DETECTION fires only on the ones marked
/// as markers — `_inherits` and `_extends` are Babel's helper names too
/// (`_extends({}, y)` is Babel output), so seeing one says nothing about
/// swc. The two used to be separate lists (15 vs 17 names) with nothing
/// saying why they differed.
#[test]
fn swc_helpers_are_one_list_and_detection_reads_only_its_markers() {
    use crate::detect::signals::detect_swc_minifier;
    use crate::rename::eligibility::SWC_HELPERS;

    let swc = create_skip_set(NeverRename::for_verdicts(
        BundlerType::Unknown,
        MinifierType::Swc,
    ));
    let universal = create_skip_set(NeverRename::UNIVERSAL);
    let mut swc_only: Vec<&str> = swc.difference(&universal).copied().collect();
    swc_only.sort();
    let mut listed: Vec<&str> = SWC_HELPERS.iter().map(|h| h.name).collect();
    listed.sort();
    assert_eq!(
        swc_only, listed,
        "the never-rename swc list IS the helper list"
    );
    let mut not_markers: Vec<&str> = Vec::new();
    for helper in SWC_HELPERS {
        let fires = !detect_swc_minifier(&format!("var a = {}(b);", helper.name)).is_empty();
        assert_eq!(fires, helper.detection_marker, "{}", helper.name);
        if !helper.detection_marker {
            not_markers.push(helper.name);
        }
    }
    not_markers.sort();
    assert_eq!(not_markers, ["_extends", "_inherits"]);
}
