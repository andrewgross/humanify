//! src/rename/prior-name-snap.test.ts, case for case.

use super::{build_prior_stem_index, name_stem, snap_suggestion_to_prior, snap_to_known_prior};

fn index(names: &[&str]) -> std::collections::HashMap<String, String> {
    build_prior_stem_index(&names.iter().map(|s| s.to_string()).collect::<Vec<_>>())
}

fn snaps(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect()
}

#[test]
fn strips_trailing_decorations_and_lowercases() {
    assert_eq!(name_stem("identityVal"), "identity");
    assert_eq!(name_stem("identityVar"), "identity");
    assert_eq!(name_stem("appStateValue"), "appstate");
    assert_eq!(name_stem("normalizedSchemaInstance"), "normalizedschema");
    assert_eq!(name_stem("React95"), "react");
    assert_eq!(name_stem("RpcRequestSchema"), "rpcrequestschema");
    assert_eq!(name_stem("rpcRequestSchema"), "rpcrequestschema");
    assert_eq!(name_stem("config"), "config");
    assert_eq!(name_stem("Val"), "");
    assert_eq!(name_stem("configResult"), "configresult");
    assert_eq!(name_stem("configInstance"), "config");
    assert_eq!(name_stem("config_2"), "config");
    // Non-ASCII names never split a char.
    assert_eq!(name_stem("éVal"), "é");
}

#[test]
fn snaps_redecorated_suggestions_to_the_unique_same_stem_prior() {
    let i = index(&["identityVal", "config", "first"]);
    assert_eq!(
        snap_suggestion_to_prior("identityVar", &i, None, None),
        "identityVal"
    );
    assert_eq!(
        snap_suggestion_to_prior("configVar", &i, None, None),
        "config"
    );
    assert_eq!(
        snap_suggestion_to_prior("firstValue", &i, None, None),
        "first"
    );
    let s = index(&["rpcRequestSchema"]);
    assert_eq!(
        snap_suggestion_to_prior("RpcRequestSchema", &s, None, None),
        "rpcRequestSchema"
    );
    let amb = index(&["React95", "React103", "ink8"]);
    assert_eq!(
        snap_suggestion_to_prior("React99", &amb, None, None),
        "React99"
    );
    let one = index(&["identityVal"]);
    assert_eq!(
        snap_suggestion_to_prior("whollyOther", &one, None, None),
        "whollyOther"
    );
    let up = index(&["upstreamConfigVal"]);
    assert_eq!(
        snap_suggestion_to_prior("upstreamConfigValVal", &up, None, None),
        "upstreamConfigVal"
    );
    let r = index(&["React219"]);
    assert_eq!(
        snap_suggestion_to_prior("react23", &r, None, None),
        "React219"
    );
    let em = index(&["errorMessage"]);
    assert_eq!(
        snap_suggestion_to_prior("errorMessageText", &em, None, None),
        "errorMessageText"
    );
}

#[test]
fn the_exact_slot_snap_wins_and_reads_own_entries_only() {
    let i = index(&["caughtError"]);
    let s = snaps(&[("x", "caughtError")]);
    assert_eq!(
        snap_suggestion_to_prior("decisionOutcome", &i, Some("x"), Some(&s)),
        "caughtError"
    );
    assert_eq!(
        snap_suggestion_to_prior("decisionOutcome", &i, Some("y"), Some(&s)),
        "decisionOutcome"
    );
    let other = snaps(&[("other", "x")]);
    assert_eq!(
        snap_suggestion_to_prior("stringify", &index(&[]), Some("toString"), Some(&other)),
        "stringify"
    );
    let iv = index(&["identityVal"]);
    assert_eq!(
        snap_suggestion_to_prior("identityVar", &iv, Some("z"), Some(&s)),
        "identityVal"
    );
}

#[test]
fn snap_to_known_prior_only_snaps_redecorations() {
    assert_eq!(
        snap_to_known_prior("identityVal", "identityVar"),
        "identityVal"
    );
    assert_eq!(
        snap_to_known_prior("configTable", "configTable2"),
        "configTable"
    );
    assert_eq!(
        snap_to_known_prior("retryCount", "attemptTally"),
        "attemptTally"
    );
}
