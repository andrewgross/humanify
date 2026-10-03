//! The name predicates validated rename reads, pinned to the TS probe
//! (`test/parity/wp31-name-probe.mjs` → `test/parity/wp31-names.json`):
//! every name in the probe's table, every predicate, exact.

use serde_json::Value;

use crate::rename::eligibility::is_eligible;
use crate::rename::floor::{is_below_floor_name, is_decorated_descriptive, is_minifier_token};
use crate::rename::name_profile::NameProfile;

/// The truth table pins the BUN profile (the TS `isBunToken` it froze).
const BUN: NameProfile = NameProfile::Bun;
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
            ("isBunToken", is_minifier_token(BUN, name)),
            (
                "isDecoratedDescriptive",
                is_decorated_descriptive(BUN, name),
            ),
            ("isBelowFloorName", is_below_floor_name(BUN, name)),
            (
                "eligible",
                is_eligible(name, crate::rename::eligibility::NeverRename::UNIVERSAL),
            ),
            (
                "eligibleBun",
                is_eligible(
                    name,
                    crate::rename::eligibility::NeverRename::for_verdicts(
                        humanify_model::detection::BundlerType::Bun,
                        humanify_model::detection::MinifierType::Bun,
                    ),
                ),
            ),
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
        assert!(is_minifier_token(BUN, name), "{name:?} is minted shape");
        assert!(
            is_below_floor_name(BUN, name),
            "{name:?} is below the floor"
        );
        assert!(is_single_letter(name), "{name:?} is a single letter");
    }
    // The real short words stay real.
    for name in [
        "get", "set", "ctx", "err", "fn", "id", "ok", "db", "abs", "url",
    ] {
        assert!(
            !is_minifier_token(BUN, name),
            "{name:?} is a real short word"
        );
    }
    // A letter with the conflict ladder's tail is a plain mint both sides
    // of the change (it was `is_bun_token` via the trailing `_` before).
    assert!(is_minifier_token(BUN, "x_"));
    assert!(!is_single_letter("x_"));

    // WHAT ANSWER MAY LAND: junk is refused, a single letter is not junk.
    for name in ["i", "x", "Q"] {
        assert!(
            is_sweep_answer_acceptable(BUN, name),
            "{name:?}: a loop counter may land as a single letter"
        );
    }
    for name in ["count", "getValue", "MAX_SIZE", "ctx"] {
        assert!(
            is_sweep_answer_acceptable(BUN, name),
            "{name:?} is a real name"
        );
    }
    for name in ["a1b", "x_", "Kq$", "zz", "q7", "do7Function"] {
        assert!(
            !is_sweep_answer_acceptable(BUN, name),
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
    assert!(is_half_mint_head(BUN, "do7Function"));
    assert!(is_half_mint_head(BUN, "T7Class"));
    assert!(is_half_mint_head(BUN, "sm6Factory"));
    assert!(is_half_mint_head(BUN, "h06Result"));
    assert!(is_half_mint_head(BUN, "j3lResult"));
    assert!(!is_half_mint_head(BUN, "P2PConnection"));
    assert!(!is_half_mint_head(BUN, "v8Engine"));
    assert!(!is_half_mint_head(BUN, "options"));
}

/// The minifier name profile split (2026-10-03) must leave Bun runs
/// byte-identical: the Bun profile reproduces main's predicates EXACTLY
/// on a frozen battery (test/parity/name-profile-bun-battery.json —
/// 4,017 names: a sample of 2.1.119's minified bindings, every recorded
/// 2.1.119 answer the borrowed-stem check refused, and a sample of the
/// rest of the 991k recorded answers; written by main's floor.rs before
/// the split). The full corpus (562,438 names, 991,371 answer rows) was
/// replayed once off-repo with zero differences.
#[test]
fn the_bun_profile_reproduces_the_frozen_battery() {
    use crate::rename::floor::{
        MinifiedStems, borrowed_minified_stem, is_borrowable_stem, is_half_mint_head,
        is_minified_echo, is_sweep_answer_acceptable, is_wordless_mint_shape,
    };
    let battery: Value = serde_json::from_str(include_str!(
        "../../../../test/parity/name-profile-bun-battery.json"
    ))
    .expect("battery parses");
    let stems = MinifiedStems::from_names(BUN, strings(&battery["stems"]));
    let mut failures = Vec::new();
    let rows = battery["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 4017);
    for row in rows {
        let name = row[0].as_str().expect("name");
        let bits: String = [
            is_minifier_token(BUN, name),
            is_decorated_descriptive(BUN, name),
            is_below_floor_name(BUN, name),
            is_sweep_answer_acceptable(BUN, name),
            is_borrowable_stem(BUN, name),
            is_minified_echo(BUN, name, name),
            is_half_mint_head(BUN, name),
            is_wordless_mint_shape(name),
        ]
        .iter()
        .map(|b| if *b { '1' } else { '0' })
        .collect();
        if bits != row[1].as_str().expect("bits") {
            failures.push(format!("{name:?}: frozen {} now {bits}", row[1]));
        }
        let borrowed = borrowed_minified_stem(name, &stems);
        if borrowed != row[2].as_str() {
            failures.push(format!("{name:?}: frozen stem {} now {borrowed:?}", row[2]));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
