//! matchFossilModules' exact outputs (test/parity/wp51-fossil-match.json,
//! from test/parity/wp51-fossil-match-probe.ts): the TS spec's cases plus
//! an 800-case seeded sweep — matches in RECORD order, tier counts in
//! first-recorded order, and every pair's tier.

use super::{FossilSignature, match_fossil_modules};
use serde_json::Value;

const VECTORS: &str = include_str!("../../../../../../test/parity/wp51-fossil-match.json");

fn strings(v: Option<&Value>) -> Option<Vec<String>> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_string()).collect())
}

fn side(v: &Value) -> Vec<FossilSignature> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|m| FossilSignature {
            hashes: strings(m.get("hashes")).unwrap(),
            imports: m["imports"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i.as_u64().unwrap() as usize)
                .collect(),
            stem: m.get("stem").and_then(Value::as_str).map(str::to_string),
            tokens: strings(m.get("tokens")),
            declared: strings(m.get("declared")),
        })
        .collect()
}

#[test]
fn every_ts_case_matches_in_the_same_order_with_the_same_tiers() {
    let rows: Vec<Value> = serde_json::from_str(VECTORS).expect("vectors");
    assert!(rows.len() > 800);
    let mut failures = Vec::new();
    for (k, row) in rows.iter().enumerate() {
        let out = match_fossil_modules(&side(&row["prior"]), &side(&row["fresh"]));
        let matches: Vec<(usize, usize)> = row["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p[0].as_u64().unwrap() as usize,
                    p[1].as_u64().unwrap() as usize,
                )
            })
            .collect();
        let tiers: Vec<(String, usize)> = row["tiers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                (
                    p[0].as_str().unwrap().to_string(),
                    p[1].as_u64().unwrap() as usize,
                )
            })
            .collect();
        let pair_ok = row["pairTiers"].as_array().unwrap().iter().all(|p| {
            out.pair_tiers
                .get(&(p[0].as_u64().unwrap() as usize))
                .copied()
                == p[1].as_str()
        });
        if out.matches != matches || out.tiers != tiers || !pair_ok {
            failures.push(format!(
                "case {k}: ts {matches:?} {tiers:?}\n  rust {:?} {:?}",
                out.matches, out.tiers
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn sig(hashes: &[&str], declared: &[&str]) -> FossilSignature {
    let mut hashes: Vec<String> = hashes.iter().map(|h| h.to_string()).collect();
    hashes.sort();
    FossilSignature {
        hashes,
        imports: Vec::new(),
        stem: None,
        tokens: None,
        declared: Some(declared.iter().map(|d| d.to_string()).collect()),
    }
}

/// exp229: a fresh module whose code AND names are a prior module's is
/// never handed to another prior module that shares most of its shape
/// but none of its names. Two prior modules carry the fresh one's exact
/// signature (so the unique-signature tier cannot pair it), and a third
/// prior module, earlier in the bundle, holds that signature plus one
/// statement; the overlap tier used to give the fresh module to the
/// first prior module it scanned.
#[test]
fn a_fresh_module_is_not_paired_against_a_prior_module_its_names_disown() {
    let shape = ["h1", "h2", "h3", "h4", "h5"];
    let prior = vec![
        sig(
            &["h1", "h2", "h3", "h4", "h5", "x9"],
            &["checkerA", "checkerB", "initChecker"],
        ),
        sig(
            &shape,
            &["provA", "provB", "provC", "provD", "initProvider"],
        ),
        sig(&shape, &["otherA", "initOther"]),
    ];
    let fresh = vec![
        sig(
            &shape,
            &["provA", "provB", "provC", "provD", "initProviderX"],
        ),
        sig(
            &["h6", "h7", "h8"],
            &["checkerA", "newB", "newC", "newD", "initChecker"],
        ),
    ];
    let out = match_fossil_modules(&prior, &fresh);
    let prior_of = |fi: usize| out.matches.iter().find(|m| m.0 == fi).map(|m| m.1);
    assert_eq!(
        prior_of(0),
        Some(1),
        "{:?} {:?}",
        out.matches,
        out.pair_tiers
    );
}
