//! The name predicates validated rename reads, pinned to the TS probe
//! (`test/parity/wp31-name-probe.mjs` → `test/parity/wp31-names.json`):
//! every name in the probe's table, every predicate, exact.

use serde_json::Value;

use crate::rename::eligibility::is_eligible;
use crate::rename::floor::{is_below_floor_name, is_bun_token, is_decorated_descriptive};
use crate::rename::validated::target::{
    GLOBAL_BUILTINS, RESERVED_WORDS, is_valid_identifier, is_valid_rename_target,
};

fn probe() -> Value {
    serde_json::from_str(include_str!("../../../../test/parity/wp31-names.json"))
        .expect("fixture parses")
}

fn strings(v: &Value) -> Vec<&str> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|s| s.as_str().expect("string"))
        .collect()
}

/// `RESERVED_WORDS` and `GLOBAL_BUILTINS` (the latter DERIVED from the
/// `globals` package in the TS) are the TS sets exactly.
#[test]
fn target_sets_match_the_ts_exactly() {
    let p = probe();
    assert_eq!(RESERVED_WORDS, strings(&p["reservedWords"]).as_slice());
    assert_eq!(GLOBAL_BUILTINS, strings(&p["globalBuiltins"]).as_slice());
}

/// Every probed name through every predicate: isValidIdentifier,
/// isValidRenameTarget, isBunToken, isDecoratedDescriptive,
/// isBelowFloorName, createIsEligible() and createIsEligible("bun","bun").
#[test]
fn name_predicates_match_the_ts_truth_table() {
    let p = probe();
    let mut failures = Vec::new();
    for row in p["names"].as_array().expect("names") {
        let name = row["name"].as_str().expect("name");
        let checks: [(&str, bool); 7] = [
            ("isValidIdentifier", is_valid_identifier(name)),
            ("isValidRenameTarget", is_valid_rename_target(name)),
            ("isBunToken", is_bun_token(name)),
            ("isDecoratedDescriptive", is_decorated_descriptive(name)),
            ("isBelowFloorName", is_below_floor_name(name)),
            ("eligible", is_eligible(name, None, None)),
            ("eligibleBun", is_eligible(name, Some("bun"), Some("bun"))),
        ];
        for (field, mine) in checks {
            let theirs = row[field].as_bool().expect("bool");
            if mine != theirs {
                failures.push(format!("{field}({name:?}): ts={theirs} rust={mine}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Andrew's 2026-09-30 decision: the single-letter exemption is GONE. The
/// ten letters (a, b, e, i, j, k, n, t, x, y) leave `SHORT_WORDS`, so they
/// are mint-SHAPED like every other minifier token — the census and the
/// coverage sweep can see a never-asked `i` (the exemption made it
/// invisible to the pass whose job is never-asked names, finding #62) —
/// while a single letter stays an acceptable ANSWER (a loop counter may
/// legitimately land as `i`). The letters' truth-table rows in
/// test/parity/wp31-names.json are re-cut with this change: post-cutover
/// the pin is ours (the TS that froze it is deleted).
#[test]
fn single_letters_are_minted_but_acceptable_sweep_answers() {
    use crate::rename::floor::{is_single_letter, is_sweep_answer_acceptable};

    // WHO GETS ASKED: every single letter is mint-shaped now.
    for name in [
        "a", "b", "e", "i", "j", "k", "n", "t", "x", "y", "z", "Q", "é",
    ] {
        assert!(is_bun_token(name), "{name:?} is minted shape");
        assert!(is_below_floor_name(name), "{name:?} is below the floor");
        assert!(is_single_letter(name), "{name:?} is a single letter");
    }
    // The real short words stay real.
    for name in [
        "get", "set", "ctx", "err", "fn", "id", "ok", "db", "abs", "url",
    ] {
        assert!(!is_bun_token(name), "{name:?} is a real short word");
    }
    // A letter with the conflict ladder's tail is a plain mint both sides
    // of the change (it was `is_bun_token` via the trailing `_` before).
    assert!(is_bun_token("x_"));
    assert!(!is_single_letter("x_"));

    // WHAT ANSWER MAY LAND: junk is refused, a single letter is not junk.
    for name in ["i", "x", "Q"] {
        assert!(
            is_sweep_answer_acceptable(name),
            "{name:?}: a loop counter may land as a single letter"
        );
    }
    for name in ["count", "getValue", "MAX_SIZE", "ctx"] {
        assert!(is_sweep_answer_acceptable(name), "{name:?} is a real name");
    }
    for name in ["a1b", "x_", "Kq$", "zz", "q7", "do7Function"] {
        assert!(
            !is_sweep_answer_acceptable(name),
            "{name:?}: the sweep refuses re-minted junk"
        );
    }
}

#[test]
fn wordless_mint_shape_and_half_mint_head() {
    use crate::rename::floor::{is_half_mint_head, is_wordless_mint_shape};
    // minted-census.ts: no 3-lowercase run and not SCREAMING_CASE.
    assert!(is_wordless_mint_shape("iIn"));
    assert!(is_wordless_mint_shape("Ab2"));
    assert!(!is_wordless_mint_shape("do7Function"));
    assert!(!is_wordless_mint_shape("MAX_SIZE"));
    assert!(!is_wordless_mint_shape("options"));
    // Camel half-mints (the census's shapes) vs acronym/domain heads.
    assert!(is_half_mint_head("do7Function"));
    assert!(is_half_mint_head("T7Class"));
    assert!(is_half_mint_head("sm6Factory"));
    assert!(is_half_mint_head("h06Result"));
    assert!(is_half_mint_head("j3lResult"));
    assert!(!is_half_mint_head("P2PConnection"));
    assert!(!is_half_mint_head("v8Engine"));
    assert!(!is_half_mint_head("options"));
}
