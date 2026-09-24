//! The JS-JSON and cache-key rules, pinned to vectors recorded from the
//! real TS functions: test/parity/wp41-js-vectors.json (generator
//! test/parity/wp41-js-probe.ts) and test/parity/cache-key-vectors.jsonl
//! (R4, generator test/parity/generate-cache-key-vectors.ts).

use crate::js::{JsValue, canonical, math_round, number_to_string, stringify, to_fixed};
use crate::llm::{BatchRenameRequest, CacheKeyParams, cache_key_material, cache_key_of};

fn parity_file(name: &str) -> String {
    let path = format!("{}/../../test/parity/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn vectors() -> serde_json::Value {
    serde_json::from_str(&parity_file("wp41-js-vectors.json")).unwrap()
}

fn f64_from_bits_hex(hex: &str) -> f64 {
    // The probe writes the little-endian bytes of a Float64Array.
    let mut bytes = [0u8; 8];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap();
    }
    f64::from_le_bytes(bytes)
}

/// `canonicalJson` on adversarial objects: index-like keys first
/// ("9" before "10", "-1"/"01" are ordinary), UTF-16 order (U+FF5E after
/// U+1F600), control/U+2028 escaping, JS number formatting (-0 → 0, 1e+21).
#[test]
fn canonical_matches_the_ts_on_every_probe_case() {
    let v = vectors();
    for case in v["canonical"].as_array().unwrap() {
        let input = JsValue::parse(case["input"].as_str().unwrap()).unwrap();
        assert_eq!(
            canonical(&input),
            case["canonical"].as_str().unwrap(),
            "case {}",
            case["name"]
        );
    }
}

/// `cacheKeyOf` on temperatures 0 / -0 / 0.7 / 1e-7 / absent, a callee
/// WITH its snippet (key material the prompt never shows), a UTF-16-sorted
/// Set, and index-keyed maps — material AND key.
#[test]
fn cache_key_matches_the_ts_on_every_probe_case() {
    let v = vectors();
    for case in v["keys"].as_array().unwrap() {
        let mut params: CacheKeyParams =
            serde_json::from_str(case["params"].as_str().unwrap()).unwrap();
        if case["tempIsNegZero"].as_bool().unwrap() {
            params.temperature = Some(-0.0);
        }
        let request: BatchRenameRequest =
            serde_json::from_str(case["request"].as_str().unwrap()).unwrap();
        let name = &case["name"];
        assert_eq!(
            cache_key_material(&request, &params),
            case["material"].as_str().unwrap(),
            "material {name}"
        );
        assert_eq!(
            cache_key_of(&request, &params),
            case["key"].as_str().unwrap(),
            "key {name}"
        );
    }
}

/// R4's adversarial vectors (unicode identifiers, empty sets, absent vs
/// present optionals, retry fields, prompt overrides, prior fields, absent
/// params, callee params, and the set-order pair that must share a key).
#[test]
fn cache_key_matches_every_r4_vector() {
    let text = parity_file("cache-key-vectors.jsonl");
    let mut n = 0;
    for line in text.lines().filter(|l| !l.is_empty()) {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        let params: CacheKeyParams = serde_json::from_value(row["params"].clone()).unwrap();
        let request: BatchRenameRequest = serde_json::from_value(row["request"].clone()).unwrap();
        assert_eq!(
            cache_key_of(&request, &params),
            row["expectedKey"].as_str().unwrap(),
            "vector {}",
            row["name"]
        );
        n += 1;
    }
    assert_eq!(n, 12);
}

/// `JSON.stringify(JSON.parse(raw))` — the cache write path's byte shape:
/// index keys move to the front, a duplicate key keeps its first slot and
/// its last value.
#[test]
fn stringify_of_a_parsed_entry_matches_the_ts() {
    let v = vectors();
    for case in v["stringify"].as_array().unwrap() {
        let parsed = JsValue::parse(case["raw"].as_str().unwrap()).unwrap();
        assert_eq!(stringify(&parsed), case["stringified"].as_str().unwrap());
    }
}

/// `String(x)` for the probe's numbers (bit patterns, so the parse is exact).
#[test]
fn number_to_string_matches_the_ts() {
    let v = vectors();
    for case in v["numbers"].as_array().unwrap() {
        let x = f64_from_bits_hex(case["bits"].as_str().unwrap());
        assert_eq!(
            number_to_string(x),
            case["string"].as_str().unwrap(),
            "{x:e}"
        );
    }
}

/// `toFixed` rounds the EXACT binary value, ties up: 1.15 → "1.1" (it is
/// 1.1499…), 1.25 → "1.3" (Rust's `{:.1}` says "1.2"), 2.5 → "3".
#[test]
fn to_fixed_matches_the_ts() {
    let v = vectors();
    for case in v["toFixed"].as_array().unwrap() {
        let x = f64_from_bits_hex(case["bits"].as_str().unwrap());
        let digits = case["digits"].as_u64().unwrap() as usize;
        assert_eq!(to_fixed(x, digits), case["fixed"].as_str().unwrap(), "{x}");
    }
}

#[test]
fn math_round_ties_toward_positive_infinity() {
    assert_eq!(math_round(2.5), 3.0);
    assert_eq!(math_round(-2.5), -2.0);
    assert_eq!(math_round(0.49999999999999994), 0.0);
    assert_eq!(math_round(1.4), 1.0);
}
