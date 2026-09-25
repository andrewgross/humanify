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
