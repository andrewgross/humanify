//! Eligibility tests — fixture generated from the TS (test/parity/skip-list.json)
//! plus the two pattern rules' TS-derived cases.

use crate::rename::eligibility::{create_skip_set, is_eligible};

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
        let bundler = if b == "none" { None } else { Some(b) };
        let minifier = if m == "none" { None } else { Some(m) };
        let mine: Vec<&str> = {
            let mut v: Vec<&str> = create_skip_set(bundler, minifier).into_iter().collect();
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
    assert!(!is_eligible("require", None, None));
    assert!(!is_eligible("__esModule", None, None)); // hmm — is __esModule in the set? no — pattern
    // word-like dunder: __esm (short after __) reserved only when word-like
    assert!(
        !is_eligible("__esm", None, None),
        "__esm is word-like (3 chars after __)"
    );
    assert!(!is_eligible("__commonJS", None, None));
    assert!(
        is_eligible("__c", None, None),
        "short dunder is a minified app binding"
    );
    assert!(is_eligible("__ab", None, None));
    // SWC helper shape
    assert!(!is_eligible("_interop_require_default", None, None));
    assert!(!is_eligible("_ts_generator", None, None));
    assert!(
        is_eligible("_foo", None, None),
        "single segment is not the helper shape"
    );
    assert!(
        is_eligible("_myHelper", None, None),
        "uppercase breaks the shape"
    );
    // short names ARE eligible (the inverted heuristic's point)
    assert!(is_eligible("get", None, None));
    assert!(is_eligible("A9_", None, None));
    assert!(!is_eligible("", None, None));
}
