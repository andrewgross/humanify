//! The LLM boundary's bytes: the cache key material, the response bytes and
//! the JS record order of a rename map. Pinned before the JSON writer moved
//! to serde, so the key a standing cache was written under cannot move.

use crate::llm::{
    BatchRenameResponse, CacheKeyParams, Renames, Usage, cache_key_material, cache_key_of,
};
use sha2::{Digest, Sha256};

fn params(t: Option<f64>, max: Option<u64>, effort: Option<&str>) -> CacheKeyParams {
    CacheKeyParams {
        model: "gpt-oss-20b".to_string(),
        temperature: t,
        max_tokens: max,
        reasoning_effort: effort.map(str::to_string),
    }
}

fn sha(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A prompt with every character class the JSON escaper treats apart.
const AWKWARD: &str = "é\u{1}\u{1f}\u{7f}\u{2028}\u{2029}\t\n\r\u{8}\u{c}\"\\/ 😀 \u{ff5e}";

/// The cache key (version 2) is the canonical JSON of what the endpoint
/// receives: key version, both messages, the model parameters, the
/// response format. Pinned byte for byte, so a change to the key is a
/// deliberate act (it invalidates every standing cache).
#[test]
fn cache_key_material_is_the_wire_content_and_nothing_else() {
    let p = params(Some(-0.0), None, Some("low"));
    let mut p = p;
    p.model = "m".to_string();
    assert_eq!(
        cache_key_material("sys", "user: f(a, b)", &p),
        "{\"cacheVersion\":2,\"messages\":[{\"content\":\"sys\",\"role\":\"system\"},\
         {\"content\":\"user: f(a, b)\",\"role\":\"user\"}],\
         \"params\":{\"model\":\"m\",\"reasoningEffort\":\"low\",\"temperature\":0},\
         \"responseFormat\":\"json_object\"}"
    );
}

/// The material over a grid of parameters and awkward prompts, pinned as
/// written before the serde writer (every temperature form, both optional
/// parameters present and absent, every escape class).
#[test]
fn cache_key_material_is_pinned_across_parameters_and_escapes() {
    assert_eq!(
        sha(&cache_key_material(
            AWKWARD,
            "u",
            &params(Some(0.0), Some(6000), Some("low"))
        )),
        "83fe9efab24116fbe09943963ffd332dace1dff163d9e6936d62bc59b59f840e"
    );
    let mut all = String::new();
    for t in [
        None,
        Some(0.0),
        Some(-0.0),
        Some(0.2),
        Some(1.0),
        Some(0.7),
        Some(1.5),
        Some(2.0),
    ] {
        for max in [None, Some(0), Some(6000)] {
            for effort in [None, Some("high")] {
                let p = params(t, max, effort);
                all.push_str(&cache_key_material(AWKWARD, "user\u{0}", &p));
                all.push_str(&cache_key_of("s", AWKWARD, &p));
                all.push('\n');
            }
        }
    }
    assert_eq!(
        sha(&all),
        "1b5222cf44f8a02d399cb874ea11ba1c40646d928144765f5d1bebd873f485e2"
    );
}

/// Every input the endpoint receives moves the key: each prompt, and each
/// model parameter (an absent temperature differs from 0; 0 and -0 agree,
/// as JS numbers do).
#[test]
fn every_wire_input_moves_the_key() {
    let base = params(Some(0.0), None, None);
    let key = |s: &str, u: &str, p: &CacheKeyParams| cache_key_of(s, u, p);
    let k0 = key("s", "u", &base);
    assert_ne!(k0, key("s2", "u", &base));
    assert_ne!(k0, key("s", "u2", &base));
    assert_ne!(k0, key("u", "s", &base), "the roles are part of the key");
    for p in [
        CacheKeyParams {
            model: "m2".to_string(),
            ..base.clone()
        },
        params(None, None, None),
        params(Some(0.7), None, None),
        params(Some(0.0), Some(6000), None),
        params(Some(0.0), None, Some("low")),
    ] {
        assert_ne!(k0, key("s", "u", &p), "{p:?}");
    }
    assert_eq!(k0, key("s", "u", &params(Some(-0.0), None, None)));
}

/// A rename map built by assignment keeps JS record order: array-index
/// keys first, ascending; a repeated key keeps its first slot and takes
/// its last value; "01" and "-1" are ordinary keys.
#[test]
fn renames_keep_js_record_order() {
    let r = Renames::from_entries(
        [
            ("b", Some("x")),
            ("10", Some("ten")),
            ("01", Some("zero-one")),
            ("2", None),
            ("b", Some("w")),
            ("-1", Some("neg")),
        ]
        .map(|(k, v)| (k.to_string(), v.map(str::to_string))),
    );
    let keys: Vec<&str> = r.entries().iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["2", "10", "b", "01", "-1"]);
    assert_eq!(r.get("b"), Some("w"));
}

/// `JSON.stringify(response)`: renames, finishReason, usage; absent fields
/// left out.
#[test]
fn response_bytes_are_pinned() {
    let r = BatchRenameResponse {
        renames: Renames::from_entries([
            ("a".to_string(), Some("alpha".to_string())),
            ("k".to_string(), None),
            ("q".to_string(), Some(AWKWARD.to_string())),
        ]),
        finish_reason: Some("stop".to_string()),
        usage: Some(Usage {
            total_tokens: Some(15),
            input_tokens: None,
            output_tokens: Some(5),
        }),
    };
    assert_eq!(
        sha(&r.to_json()),
        "4b545f55cc166d389830c268dd3f578fe4e5c07dbb08b09aa66768860cbf2cf2"
    );
    assert_eq!(BatchRenameResponse::default().to_json(), "{\"renames\":{}}");
}

/// The JSON number writer agrees with `String(x)` on every value a
/// temperature or a count takes, including the integral and signed-zero
/// forms and decimals down to 1e-5. Below 1e-5 serde switches to exponent
/// form earlier than JS does (`1e-6` against `0.000001`); no temperature
/// is that small.
#[test]
fn js_numbers_print_as_javascript_does() {
    #[derive(serde::Serialize)]
    struct N(#[serde(serialize_with = "crate::llm::serialize_js_number")] f64);
    let mut values: Vec<f64> = (0..=400).map(|i| f64::from(i) / 100.0).collect();
    values.extend([
        -0.0,
        0.1 + 0.2,
        1e-3,
        1e-4,
        1e-5,
        0.7,
        123456.789,
        6000.0,
        1e15,
        8.9e15,
        -3.5,
    ]);
    for x in values {
        assert_eq!(
            serde_json::to_string(&N(x)).unwrap(),
            crate::js::number_to_string(x),
            "{x:e}"
        );
    }
    assert_eq!(serde_json::to_string(&N(f64::NAN)).unwrap(), "null");
}
