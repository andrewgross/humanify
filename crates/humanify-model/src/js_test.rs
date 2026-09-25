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

/// JS `String(NaN)` / `String(Infinity)`: the one owner of number text
/// must cover non-finite values (the profile summary prints a NaN
/// percentage when a run's total duration is zero).
#[test]
fn number_to_string_covers_non_finite_values() {
    assert_eq!(number_to_string(f64::NAN), "NaN");
    assert_eq!(number_to_string(f64::INFINITY), "Infinity");
    assert_eq!(number_to_string(f64::NEG_INFINITY), "-Infinity");
}

/// TS `formatDuration` (src/llm/metrics.ts), one owner for the LLM
/// metrics and the profile summary — accidents included (59,999 ms →
/// "60.0s"; 3,599,999 ms → "59m 60s").
#[test]
fn format_duration_matches_the_ts() {
    use crate::js::format_duration;
    assert_eq!(format_duration(500.0), "500ms");
    assert_eq!(format_duration(5000.0), "5.0s");
    assert_eq!(format_duration(1250.0), "1.3s");
    assert_eq!(format_duration(59_999.0), "60.0s");
    assert_eq!(format_duration(125_000.0), "2m 5s");
    assert_eq!(format_duration(3_599_999.0), "59m 60s");
    assert_eq!(format_duration(7_500_000.0), "2h 5m");
}

/// `String.prototype.trim`'s whitespace set, every code point, recorded
/// from the real JS (test/parity/wp42-vectors.json `jsWhitespace`, WP4.2):
/// it differs from Rust's `char::is_whitespace` on U+FEFF (JS strips it)
/// and U+0085 (JS keeps it).
#[test]
fn js_whitespace_equals_the_probed_set() {
    let v: serde_json::Value = serde_json::from_str(&parity_file("wp42-vectors.json")).unwrap();
    let probed: Vec<u32> = v["jsWhitespace"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n.as_u64().unwrap() as u32)
        .collect();
    let ours: Vec<u32> = (0..=0x10FFFFu32)
        .filter_map(char::from_u32)
        .filter(|c| crate::js::is_js_whitespace(*c))
        .map(u32::from)
        .collect();
    assert_eq!(ours, probed);
    assert_eq!(crate::js::trim("\u{feff} a b\u{85}\n"), "a b\u{85}");
    assert_eq!(crate::js::utf16_len("a😀é"), 4);
}

/// `Math.log` bit for bit with V8 (test/parity/wp51-math-log.json): the
/// platform libm disagrees on ~3% of these inputs by an ulp.
#[test]
fn math_log_matches_v8_bit_for_bit() {
    let rows: Vec<(String, String)> =
        serde_json::from_str(&parity_file("wp51-math-log.json")).expect("vectors");
    assert!(rows.len() > 5000);
    let mut libm_differs = 0;
    for (input, output) in &rows {
        // This probe writes the u64 bit pattern, big-endian hex.
        let bits = |h: &str| f64::from_bits(u64::from_str_radix(h, 16).unwrap());
        let x = bits(input);
        let want = bits(output);
        let got = crate::js::math_log(x);
        assert!(
            got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()),
            "Math.log({x:e}): v8 {want:e} rust {got:e}"
        );
        if x.ln().to_bits() != want.to_bits() && !want.is_nan() {
            libm_differs += 1;
        }
    }
    assert!(
        libm_differs > 0,
        "the vectors must separate fdlibm from libm"
    );
}

/// A numeric literal's value from its source spelling — bit-exact where
/// serde_json's default float parser is not (the 2.1.216 FLT_MAX literal),
/// every radix, separators, sloppy legacy octal, and no BigInt.
#[test]
fn numeric_literal_value_reads_the_source_spelling_exactly() {
    use crate::js::numeric_literal_value as v;
    assert_eq!(
        v("340282346638528860000000000000000000000").map(f64::to_bits),
        Some(3.402_823_466_385_288_6e38_f64.to_bits())
    );
    assert_eq!(
        v("0x1fffffffffffff1"),
        Some(0x1ff_ffff_ffff_fff1_u64 as f64)
    );
    assert_eq!(v("0XFF"), Some(255.0));
    assert_eq!(v("0o777"), Some(511.0));
    assert_eq!(v("0b101"), Some(5.0));
    assert_eq!(v("1_000_000"), Some(1e6));
    assert_eq!(v(".5e1"), Some(5.0));
    assert_eq!(v("5."), Some(5.0));
    assert_eq!(v("017"), Some(15.0), "legacy octal");
    assert_eq!(v("019"), Some(19.0), "an 8/9 digit makes it decimal");
    assert_eq!(v("00"), Some(0.0));
    assert_eq!(v("5e-324"), Some(5e-324));
    assert_eq!(v("12n"), None, "a BigInt is not a Number");
}

#[test]
fn utf16_prefix_counts_code_units_like_js_slice() {
    // "a😀é".slice(0, 3) keeps the astral char whole (2 units) — 3 units.
    assert_eq!(crate::js::utf16_prefix("a😀é", 3), "a😀");
    assert_eq!(crate::js::utf16_prefix("abc", 10), "abc");
    assert_eq!(crate::js::utf16_prefix("abc", 0), "");
    // A cut inside a surrogate pair: JS keeps a lone high surrogate, which
    // a Rust String cannot hold — the prefix stops before the pair.
    assert_eq!(crate::js::utf16_prefix("a😀", 2), "a");
}

/// `Object.fromEntries` over a Map: index keys first (ascending), ordinary
/// keys in insertion order — the bulk constructor and the one-at-a-time
/// insert must agree, including on a repeated key.
#[test]
fn from_entries_matches_insert_order() {
    use crate::js::JsObject;
    let rows = |keys: &[&str]| -> Vec<(String, JsValue)> {
        keys.iter()
            .enumerate()
            .map(|(i, k)| (k.to_string(), JsValue::Number(i as f64)))
            .collect()
    };
    for keys in [
        &["b", "10", "a", "2", "__proto__", "01"][..],
        &["x", "y", "x", "3"][..],
        &[][..],
    ] {
        let bulk = JsObject::from_entries(rows(keys));
        let one: JsObject = rows(keys).into_iter().collect();
        assert_eq!(bulk, one, "{keys:?}");
    }
    let o = JsObject::from_entries(rows(&["b", "10", "a", "2"]));
    let keys: Vec<&str> = o.entries().iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["2", "10", "b", "a"]);
}

/// Exact ties at the shortest digit: ECMA-262 picks the EVEN digit (V8:
/// `String(1658206780088562.25)` is "1658206780088562.2"); Rust's `{:e}`
/// picks the upper one. 235,115 of 3.5 M sampled doubles differed before
/// the owner moved to `dragonbox_ecma` (finding #47).
#[test]
fn number_to_string_breaks_exact_ties_to_even_like_v8() {
    let cases = [
        (1658206780088562.0 + 0.25, "1658206780088562.2"),
        (-1052730259603333.0 - 0.25, "-1052730259603333.2"),
        (233115890514796.0 + 0.125, "233115890514796.12"),
        (271821092707313.0 + 0.625, "271821092707313.62"),
        (1e21, "1e+21"),
        (1e-7, "1e-7"),
        (5e-324, "5e-324"),
        (123456789012345680000.0, "123456789012345680000"),
        (-0.0, "0"),
    ];
    for (x, want) in cases {
        assert_eq!(number_to_string(x), want, "{x:e}");
    }
}

#[test]
fn utf16_offsets_convert_byte_offsets_to_js_string_indexes() {
    use crate::js::Utf16Offsets;
    // "a😀é" — bytes a=0, 😀=1..5, é=5..7; units a=0, 😀=1..3, é=3..4.
    let t = Utf16Offsets::new("a😀é");
    assert_eq!(
        [t.at(0), t.at(1), t.at(5), t.at(7)],
        [0, 1, 3, 4],
        "every char boundary, the end included"
    );
    // The ASCII fast path is the identity.
    let ascii = Utf16Offsets::new("abc");
    assert_eq!([ascii.at(0), ascii.at(3)], [0, 3]);
}
