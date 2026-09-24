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
